//! Pure source metadata and address-resolution functions.
//! These are independently testable on Linux; network transport is introduced in phase 2.
use crate::config::SOURCES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind { Direct, Goes, Himawari, Epic, Wms }

#[derive(Debug, Clone, Copy)]
pub struct Source {
    pub name: &'static str,
    pub home_url: &'static str,
    pub kind: SourceKind,
    pub crop: Option<(u32, u32, u32)>,
}

pub const ALL: [Source; 6] = [
    Source { name: "风云4B", home_url: "https://img.nsmc.org.cn/CLOUDIMAGE/FY4B/AGRI/GCLR/FY4B_DISK_GCLR.JPG", kind: SourceKind::Direct, crop: Some((65, 80, 10835)) },
    Source { name: "GOES-East", home_url: "https://www.star.nesdis.noaa.gov/goes/fulldisk.php?sat=G19", kind: SourceKind::Goes, crop: Some((24, 24, 10800)) },
    Source { name: "GOES-West", home_url: "https://www.star.nesdis.noaa.gov/goes/fulldisk.php?sat=G18", kind: SourceKind::Goes, crop: Some((24, 24, 10800)) },
    Source { name: "Himawari-9", home_url: "https://himawari8-dl.nict.go.jp/himawari8/img/D531106", kind: SourceKind::Himawari, crop: None },
    Source { name: "NASA EPIC", home_url: "https://epic.gsfc.nasa.gov/api/natural", kind: SourceKind::Epic, crop: None },
    Source { name: "Meteosat (MTG)", home_url: "https://view.eumetsat.int/geoserver/wms", kind: SourceKind::Wms, crop: None },
];

pub fn by_name(name: &str) -> Source {
    ALL.iter().copied().find(|x| x.name == name).unwrap_or(ALL[0])
}

pub fn wms_url() -> String {
    // MTG full-disc imagery is in a geostationary projection. EPSG:4326
    // reprojects it to a flat map, which can render incorrectly or cause
    // expensive GetMap requests to fail with HTTP 500. Match EUMETView's
    // known working AUTO:97004 request and keep a square Earth disc.
    format!("{}?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetMap&LAYERS=mtg_fd%3Argb_geocolour&STYLES=&FORMAT=image%2Fjpeg&SRS=AUTO%3A97004%2C9001%2C0%2C0&BBOX=-6500000%2C-6500000%2C6500000%2C6500000&WIDTH=1600&HEIGHT=1600&TRANSPARENT=FALSE&BGCOLOR=0x000000", ALL[5].home_url)
}

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct ObservedImage { pub url: String, pub observed_at: String }

/// WMS without TIME can return a cached/default historic slice. Discover the
/// latest published observation in the target layer's capabilities instead.
/// The value must come from the server: never substitute our local clock.
pub fn meteosat_latest_time(xml: &str) -> Result<String,String> {
    let layer=xml.find("mtg_fd:rgb_geocolour").ok_or("Meteosat GeoColour WMS layer absent")?;
    let rest=&xml[layer..];
    let limit=rest.find("</Layer>").unwrap_or(rest.len()).min(750_000);
    let scope=&rest[..limit];
    let lower=scope.to_ascii_lowercase();
    for tag in ["extent","dimension"] {
        let mut start=0;
        while let Some(pos)=lower[start..].find(&format!("<{tag}")) {
            let pos=pos+start;
            let Some(close)=lower[pos..].find('>') else{break};
            let close=pos+close;
            let attrs=&lower[pos..close];
            start=close+1;
            if !attrs.contains("name=\"time\"") && !attrs.contains("name='time'"){continue;}
            let end_marker=format!("</{tag}>");
            let Some(end)=lower[start..].find(&end_marker) else{continue};
            let content=scope[start..start+end].trim();
            // Explicit enumerated times or start/end/interval range.
            let final_value=content.rsplit(',').next().unwrap_or(content).trim();
            let final_value=if final_value.contains('/') {
                final_value.split('/').nth(1).unwrap_or(final_value)
            }else{final_value};
            let timestamp=final_value.trim();
            let valid=timestamp.len()>=20 && timestamp.len()<=30
                && timestamp.as_bytes().get(4)==Some(&b'-')
                && timestamp.as_bytes().get(7)==Some(&b'-')
                && timestamp.as_bytes().get(10)==Some(&b'T')
                && timestamp.ends_with('Z')
                && timestamp.bytes().all(|c|c.is_ascii_digit()||
                    matches!(c,b'-'|b'T'|b':'|b'.'|b'Z'));
            if valid{return Ok(timestamp.into());}
        }
    }
    Err("Meteosat WMS layer has no usable latest TIME in GetCapabilities".into())
}

pub fn wms_capabilities_url() -> String {
    format!("{}?SERVICE=WMS&VERSION=1.3.0&REQUEST=GetCapabilities&namespace=mtg_fd",
        ALL[5].home_url)
}
/// The last advertised MTG TIME sometimes returns HTTP 500 while GeoServer is
/// still preparing it. Try at most three recent 10-minute slots, never an
/// unlimited retry loop or an untimed request (which can return old imagery).
pub fn wms_retry_times(latest: &str) -> Result<Vec<String>,String> {
    let b=latest.as_bytes();
    if !(20..=30).contains(&b.len()) || b[4]!=b'-' || b[7]!=b'-'
        || b[10]!=b'T' || b[13]!=b':' || b[16]!=b':'
        || !latest.ends_with('Z')
        || !latest.bytes().all(|c|c.is_ascii_digit()
            ||matches!(c,b'-'|b'T'|b':'|b'.'|b'Z'))
        || (b.len()>20 && (b[19]!=b'.' || !b[20..b.len()-1].iter().all(u8::is_ascii_digit)))
    {
        return Err("Invalid Meteosat latest TIME".into());
    }
    let parse=|start,end| -> Result<u32,String> {
        latest[start..end].parse::<u32>().map_err(|_|"Invalid Meteosat TIME".into())
    };
    let mut year=parse(0,4)?;
    let mut month=parse(5,7)?;
    let mut day=parse(8,10)?;
    let mut hour=parse(11,13)?;
    let mut minute=parse(14,16)?;
    let second=parse(17,19)?;
    fn days_in_month(year:u32,month:u32)->u32{
        match month {
            1|3|5|7|8|10|12=>31, 4|6|9|11=>30,
            2=>if year%4==0 && (year%100!=0 || year%400==0){29}else{28},
            _=>0,
        }
    }
    if year<1900 || month==0 || day==0 || day>days_in_month(year,month)
        || hour>23 || minute>59 || second>59 {
        return Err("Invalid Meteosat TIME calendar date".into());
    }
    let mut times=vec![latest.to_string()];
    for _ in 0..2 {
        if minute>=10 {minute-=10;}
        else {
            minute+=50;
            if hour>0 {hour-=1;}
            else {
                hour=23;
                if day>1 {day-=1;}
                else {
                    if month>1 {month-=1;}
                    else {
                        if year<=1900 {break;}
                        year-=1;month=12;
                    }
                    day=days_in_month(year,month);
                }
            }
        }
        times.push(format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}{}",
            &latest[16..]));
    }
    Ok(times)
}

pub fn wms_url_at(time: &str) -> Result<String,String> {
    // Treat provider metadata as untrusted input.
    if time.len()<20||time.len()>30||!time.ends_with('Z')
        ||!time.bytes().all(|c|c.is_ascii_digit()||matches!(c,b'-'|b'T'|b':'|b'.'|b'Z')){
        return Err("Invalid Meteosat observation TIME".into());
    }
    Ok(format!("{}&TIME={time}",wms_url()))
}

pub fn epic_latest_image(json: &str) -> Result<ObservedImage, String> {
    let list: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let records = list.as_array().ok_or("EPIC result is not a list")?;
    let latest = records.iter()
        .max_by_key(|i| i["date"].as_str().unwrap_or(""))
        .ok_or("EPIC returned no images")?;
    let image = latest["image"].as_str().ok_or("Missing image name")?;
    let date = latest["date"].as_str().ok_or("Missing date")?;
    if !image.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || image.is_empty() {
        return Err("Unsafe EPIC image identifier".into());
    }
    let day = date.get(..10).ok_or("Invalid EPIC date")?;
    let bytes = day.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-'
        || !day.chars().enumerate().all(|(i,c)| i == 4 || i == 7 || c.is_ascii_digit()) {
        return Err("Invalid EPIC date".into());
    }
    Ok(ObservedImage {
        url: format!("https://epic.gsfc.nasa.gov/archive/natural/{}/{}/{}/jpg/{}.jpg",
            &day[0..4], &day[5..7], &day[8..10], image),
        observed_at: date.to_string(),
    })
}

pub fn epic_image_url(json: &str) -> Result<String,String> {
    epic_latest_image(json).map(|selected|selected.url)
}

pub fn himawari_tiles(latest_json: &str) -> Result<Vec<String>, String> {
    let data: serde_json::Value = serde_json::from_str(latest_json).map_err(|e| e.to_string())?;
    let timestamp = data["date"].as_str().ok_or("Missing Himawari date")?;
    if timestamp.len() != 19 || !timestamp.bytes().all(|c| c.is_ascii_digit() || b" -:".contains(&c)) {
        return Err("Invalid Himawari timestamp".into());
    }
    let yyyy = &timestamp[0..4]; let mm = &timestamp[5..7]; let dd = &timestamp[8..10];
    let hh = &timestamp[11..13]; let mi = &timestamp[14..16]; let ss = &timestamp[17..19];
    let base = ALL[3].home_url;
    let mut out = Vec::with_capacity(16);
    for y in 0..4 { for x in 0..4 {
        out.push(format!("{base}/4d/550/{yyyy}/{mm}/{dd}/{hh}{mi}{ss}_{x}_{y}.png"));
    }}
    Ok(out)
}

/// NOAA STAR publishes stable full-disk GeoColor JPEG links on its CDN.
/// 5424px provides a high-quality image without 10848px decode costs.
/// If this path is unavailable, fall back to a smaller image on the same official CDN.
pub fn goes_cdn_urls(source_name: &str) -> Result<[String; 2], String> {
    let sat = match source_name {
        "GOES-East" => "GOES19",
        "GOES-West" => "GOES18",
        _ => return Err("Not a GOES source".into()),
    };
    let base = format!("https://cdn.star.nesdis.noaa.gov/{sat}/ABI/FD/GEOCOLOR");
    Ok([format!("{base}/5424x5424.jpg"), format!("{base}/1808x1808.jpg")])
}

/// Parse a GOES 10848px image anchor without needing a browser engine.
/// Relative paths are rooted at the NOAA host and never allowed to escape it.
pub fn goes_image_url(html: &str) -> Result<String, String> {
    let needle = "geocolor-10848x10848.jpg";
    let lower = html.to_ascii_lowercase();
    for (index, _) in lower.match_indices(needle) {
        let prefix = &html[..index];
        if let Some(href_start) = prefix.rfind("href=") {
            let fragment = &html[href_start + 5..];
            let quote = fragment.chars().next().unwrap_or(' ');
            if quote != '\'' && quote != '"' { continue; }
            if let Some(end) = fragment[1..].find(quote) {
                let href = &fragment[1..end + 1];
                if !href.to_ascii_lowercase().contains(needle) { continue; }
                if href.starts_with("https://www.star.nesdis.noaa.gov/") { return Ok(href.to_string()); }
                if href.starts_with('/') && !href.starts_with("//") {
                    return Ok(format!("https://www.star.nesdis.noaa.gov{href}"));
                }
                if !href.contains(':') && !href.starts_with("//") && !href.starts_with('#') {
                    return Ok(format!("https://www.star.nesdis.noaa.gov/goes/{href}"));
                }
            }
        }
    }
    Err("No supported GOES image link found".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn every_builtin_provider_uses_https(){
        for src in ALL {
            assert!(src.home_url.starts_with("https://"),
                "Provider {:?} must not accept unauthenticated satellite image transport",src.name);
        }
    }
    #[test] fn keeps_all_six_existing_sources() {
        assert_eq!(ALL.len(), SOURCES.len());
        for (a, b) in ALL.iter().zip(SOURCES) { assert_eq!(a.name,b); }
    }

    #[test] fn official_goes_links_do_not_depend_on_html() {
        let east=goes_cdn_urls("GOES-East").unwrap();
        let west=goes_cdn_urls("GOES-West").unwrap();
        assert_eq!(east[0],"https://cdn.star.nesdis.noaa.gov/GOES19/ABI/FD/GEOCOLOR/5424x5424.jpg");
        assert_eq!(west[0],"https://cdn.star.nesdis.noaa.gov/GOES18/ABI/FD/GEOCOLOR/5424x5424.jpg");
        assert!(west[1].ends_with("/1808x1808.jpg"));
        assert!(goes_cdn_urls("NASA EPIC").is_err());
    }
    #[test] fn wms_selects_last_observation_not_default(){
        let cap=r#"<WMS_Capabilities><Layer><Layer><Name>some:other</Name><Extent name="time">2020-01-01T00:00:00Z</Extent></Layer><Layer><Name>mtg_fd:rgb_geocolour</Name><Extent name="time" default="2026-10-08T12:00:00Z">2026-10-09T11:00:00Z,2026-10-09T11:10:00Z,2026-10-09T11:20:00Z</Extent></Layer></Layer></WMS_Capabilities>"#;
        let time=meteosat_latest_time(cap).unwrap();
        assert_eq!(time,"2026-10-09T11:20:00Z");
        assert!(wms_url_at(&time).unwrap().contains("TIME=2026-10-09T11:20:00Z"));
        assert!(wms_url_at("2026-10-09T11:20:00Z&LAYERS=evil").is_err());
        let range=r#"<Layer><Name>mtg_fd:rgb_geocolour</Name><Dimension name='time' units='ISO8601'>2026-10-08T10:00:00Z/2026-10-09T11:20:00Z/PT10M</Dimension></Layer>"#;
        assert_eq!(meteosat_latest_time(range).unwrap(),"2026-10-09T11:20:00Z");
        assert!(meteosat_latest_time("<Layer><Name>mtg_fd:rgb_geocolour</Name></Layer>").is_err());
    }
    #[test] fn wms_uses_geostationary_full_disc_projection(){
        let url=wms_url();
        assert!(url.contains("VERSION=1.3.0"));
        assert!(url.contains("SRS=AUTO%3A97004%2C9001%2C0%2C0"));
        assert!(url.contains("BBOX=-6500000%2C-6500000%2C6500000%2C6500000"));
        assert!(!url.contains("EPSG%3A4326"));
        assert!(!url.contains("TIME="));
    }
    #[test] fn wms_retry_slots_keep_utc_fraction_and_cross_month_boundaries(){
        assert_eq!(wms_retry_times("2026-10-10T10:40:00.000Z").unwrap(),
            vec!["2026-10-10T10:40:00.000Z",
                 "2026-10-10T10:30:00.000Z",
                 "2026-10-10T10:20:00.000Z"]);
        assert_eq!(wms_retry_times("2026-03-01T00:10:00Z").unwrap(),
            vec!["2026-03-01T00:10:00Z",
                 "2026-03-01T00:00:00Z",
                 "2026-02-28T23:50:00Z"]);
        assert_eq!(wms_retry_times("2024-03-01T00:00:00Z").unwrap()[1],
            "2024-02-29T23:50:00Z");
        assert!(wms_retry_times("2026-10-10T10:40:00Z&LAYERS=bad").is_err());
        assert!(wms_retry_times("2026-02-30T10:40:00Z").is_err());
    }
    #[test]fn epic_reports_observation_date(){
        let data=r#"[{"image":"epic_1b_20261008000000","date":"2026-10-08 00:00:00"},{"image":"epic_1b_20261008120000","date":"2026-10-08 12:00:00"}]"#;
        let record=epic_latest_image(data).unwrap();
        assert_eq!(record.observed_at,"2026-10-08 12:00:00");
        assert!(record.url.ends_with("/epic_1b_20261008120000.jpg"));
    }
    #[test] fn parses_nasa_json() {
        let url = epic_image_url(r#"[{"image":"epic_a","date":"2026-01-09 02:02:00"},{"image":"epic_b","date":"2026-01-10 00:00:00"}]"#).unwrap();
        assert_eq!(url, "https://epic.gsfc.nasa.gov/archive/natural/2026/01/10/jpg/epic_b.jpg");
        assert!(epic_image_url(r#"[{"image":"../secret","date":"2026-01-10"}]"#).is_err());
    }
    #[test] fn assembles_himawari_tile_urls() {
        let tiles = himawari_tiles(r#"{"date":"2026-10-08 02:20:00"}"#).unwrap();
        assert_eq!(tiles.len(), 16);
        assert!(tiles[15].ends_with("/022000_3_3.png"));
    }
    #[test] fn parses_goes_and_avoids_bad_domains() {
        let html = r#"<a href="/goes/2026/GEOCOLOR-10848x10848.jpg">full disk</a>"#;
        assert_eq!(goes_image_url(html).unwrap(), "https://www.star.nesdis.noaa.gov/goes/2026/GEOCOLOR-10848x10848.jpg");
        assert!(goes_image_url(r#"<a href="https://evil.test/GEOCOLOR-10848x10848.jpg">x</a>"#).is_err());
    }
}
