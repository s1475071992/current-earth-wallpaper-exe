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
    format!("{}?SERVICE=WMS&VERSION=1.1.1&REQUEST=GetMap&LAYERS=mtg_fd%3Argb_geocolour&STYLES=&FORMAT=image%2Fjpeg&SRS=EPSG%3A4326&BBOX=-77%2C-77%2C77%2C77&WIDTH=1600&HEIGHT=1600&TRANSPARENT=FALSE&BGCOLOR=0x000000", ALL[5].home_url)
}

pub fn epic_image_url(json: &str) -> Result<String, String> {
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
    Ok(format!("https://epic.gsfc.nasa.gov/archive/natural/{}/{}/{}/jpg/{}.jpg",
        &day[0..4], &day[5..7], &day[8..10], image))
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
