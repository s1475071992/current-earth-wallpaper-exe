//! End-to-end provider pipeline. Per-stage events are sent to the GUI and a local log.
use std::{
    path::{Path, PathBuf}, fs, io::Read,
    collections::{HashMap,HashSet},
    sync::atomic::{AtomicBool,Ordering},
    time::{Instant,SystemTime,UNIX_EPOCH},
};
use crate::{config::{AppConfig,app_dir},http,imaging,sources::{self,SourceKind},wallpaper,monitor::Monitor};

/// A shared raw satellite image is kept only when the same satellite occurs
/// in multiple desktop/monitor pairs in this scheduled cycle. Each monitor
/// still gets its own differently-sized rendered BMP, but repeated network
/// downloads (including 16 Himawari tiles) are avoided.
pub struct CycleSourceCache{
    repeated:HashSet<String>,
    images:HashMap<String,PathBuf>,
    observed:HashMap<String,String>,
}
impl CycleSourceCache{
    pub fn for_sources<'a>(sources:impl IntoIterator<Item=&'a str>)->Self{
        let mut counts=HashMap::<String,usize>::new();
        for source in sources{*counts.entry(source.to_string()).or_default()+=1;}
        Self{
            repeated:counts.into_iter().filter_map(|(name,n)|
                (n>1).then_some(name)).collect(),
            images:HashMap::new(),
            observed:HashMap::new(),
        }
    }
    fn find(&self,source:&str)->Option<&PathBuf>{self.images.get(source)}
    fn store(&mut self,source:&str,input:&Path,folder:&Path,observed_utc:Option<&str>)->Result<(),String>{
        if !self.repeated.contains(source)||self.images.contains_key(source){return Ok(());}
        let suffix=input.extension().and_then(|e|e.to_str()).unwrap_or("img");
        let filename=format!(".cew-{}-cycle-{}-{}.{}",
            std::process::id(),stamp(),self.images.len(),suffix);
        let path=folder.join(filename);
        fs::copy(input,&path).map_err(|e|format!("Satellite cycle cache copy: {e}"))?;
        self.images.insert(source.into(),path);
        if let Some(utc)=observed_utc {self.observed.insert(source.into(),utc.into());}
        Ok(())
    }
}
impl Drop for CycleSourceCache{
    fn drop(&mut self){
        for path in self.images.values(){let _=fs::remove_file(path);}
    }
}

/// Fingerprint the actual received satellite file along with render settings.
/// If the provider is still serving yesterday's image, it must not appear as
/// a new satellite photograph just because a timer tick fired.
fn source_fingerprint(path:&Path,source:&str,scale:&str,watermark:bool,w:u32,h:u32)
    ->Result<String,String>{
    let mut hash=0xcbf29ce484222325u64;
    for strval in [source,scale,if watermark{"watermark-obs-upd-v2"}else{"clean"}]{
        for b in strval.bytes().chain(std::iter::once(0)){
            hash=(hash^u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
    for b in w.to_le_bytes().into_iter().chain(h.to_le_bytes()){
        hash=(hash^u64::from(b)).wrapping_mul(0x100000001b3);
    }
    let mut input=fs::File::open(path).map_err(|e|e.to_string())?;
    let mut buf=[0u8;65536];
    loop{
        let n=input.read(&mut buf).map_err(|e|e.to_string())?;
        if n==0{break;}
        for &byte in &buf[..n]{hash=(hash^u64::from(byte)).wrapping_mul(0x100000001b3);}
    }
    Ok(format!("{hash:016x}"))
}

fn display_hash(input:&str)->u64{
    // Stable non-cryptographic device identifier for generated file names.
    let mut value:u64=0xcbf29ce484222325;
    for b in input.bytes(){value^=b as u64; value=value.wrapping_mul(0x100000001b3);}
    value
}
struct Scratch(Vec<PathBuf>);
impl Scratch {
    fn new()->Self{Self(Vec::new())}
    fn add(&mut self,p:PathBuf)->PathBuf{self.0.push(p.clone());p}
}
impl Drop for Scratch {
    fn drop(&mut self){for p in &self.0 {let _=fs::remove_file(p);}}
}
fn stamp()->u128{
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}
fn temp_file(folder:&Path,name:&str,scratch:&mut Scratch)->PathBuf{
    scratch.add(folder.join(format!(".cew-{}-{}-{name}",std::process::id(),stamp())))
}
fn check(cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Relaxed){Err("Cancelled by user".into())}else{Ok(())}
}
fn download_image<F:FnMut(String)>(
    url:&str,path:&Path,limit:u64,cancel:&AtomicBool,log:&mut F
)->Result<(),String>{
    check(cancel)?;
    log(format!("HTTP GET: {url}"));
    let started=Instant::now();
    let mut last=0u64;
    http::download_monitored(url,path,limit,cancel,|size|{
        if size!=last {
            last=size;
            log(format!("Received {:.2} MiB ({:.1}s)",size as f64/1048576.0,started.elapsed().as_secs_f32()));
        }
    })?;
    let len=fs::metadata(path).map_err(|e|e.to_string())?.len();
    log(format!("Download complete: {:.2} MiB / {:.1}s",len as f64/1048576.0,started.elapsed().as_secs_f32()));
    check(cancel)
}
/// A WMS server can reply with HTTP 200 and a ServiceException XML payload.
/// Do not let such a response replace a valid per-desktop wallpaper.
fn validate_wms_image(path:&Path)->Result<(),String>{
    let mut file=fs::File::open(path).map_err(|e|e.to_string())?;
    let mut header=[0u8;8];
    file.read_exact(&mut header).map_err(|_|"Meteosat WMS returned an empty or truncated image".to_string())?;
    let jpeg=header.starts_with(&[0xff,0xd8,0xff]);
    let png=header==[0x89,b'P',b'N',b'G',13,10,26,10];
    if !jpeg && !png {
        return Err("Meteosat WMS returned non-image data (possibly a ServiceException)".into());
    }
    if file.metadata().map_err(|e|e.to_string())?.len()<4096 {
        return Err("Meteosat WMS returned an implausibly small image".into());
    }
    imaging::load_scaled(path,48,None)
        .map_err(|e|format!("Meteosat WMS image decode failed: {e}"))?;
    Ok(())
}
pub fn run_once(cfg:&AppConfig,cancel:&AtomicBool,mut log:impl FnMut(String))->Result<PathBuf,String>{
    run_once_in_desktop(cfg,None,cancel,&mut log)
}
pub fn run_once_in_desktop(cfg:&AppConfig,desktop:Option<&str>,cancel:&AtomicBool,mut log:impl FnMut(String))->Result<PathBuf,String>{
    run_once_for_in_desktop(cfg,None,desktop,cancel,&mut log)
}
pub fn run_once_for(cfg:&AppConfig,display:Option<&Monitor>,cancel:&AtomicBool,mut log:impl FnMut(String))->Result<PathBuf,String>{
    run_once_for_in_desktop(cfg,display,None,cancel,&mut log)
}
/// Generate and store a stable BMP for one (virtual desktop, monitor) pair,
/// without changing Windows wallpaper. The switch handler exclusively decides
/// when images are shown on a physical monitor.
pub fn render_pair(cfg:&AppConfig,desktop:&str,display:&Monitor,cancel:&AtomicBool,log:impl FnMut(String))->Result<PathBuf,String>{
    run_once_impl(cfg,Some(display),Some(desktop),true,None,cancel,log)
}
pub fn render_pair_cached(cfg:&AppConfig,desktop:&str,display:&Monitor,cache:&mut CycleSourceCache,
    cancel:&AtomicBool,log:impl FnMut(String))->Result<PathBuf,String>{
    run_once_impl(cfg,Some(display),Some(desktop),true,Some(cache),cancel,log)
}
pub fn run_once_for_in_desktop(cfg:&AppConfig,display:Option<&Monitor>,desktop:Option<&str>,cancel:&AtomicBool,log:impl FnMut(String))->Result<PathBuf,String>{
    run_once_impl(cfg,display,desktop,false,None,cancel,log)
}
fn run_once_impl(cfg:&AppConfig,display:Option<&Monitor>,desktop:Option<&str>,render_only:bool,
    mut source_cache:Option<&mut CycleSourceCache>,cancel:&AtomicBool,mut log:impl FnMut(String))->Result<PathBuf,String>{
    let begin=Instant::now();
    let folder=if cfg.save_path.trim().is_empty(){app_dir().join("wallpapers")}
        else{PathBuf::from(&cfg.save_path)};
    fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
    let source=sources::by_name(&cfg.image_source);
    log(format!("Start update: {} | interval={} minutes",source.name,cfg.interval_minutes));
    log(format!("Output folder: {}",folder.display()));
    let mut scratch=Scratch::new();
    let cached_input=source_cache.as_deref()
        .and_then(|cache|cache.find(source.name)).cloned();
    let fresh_download=cached_input.is_none();
    // Preserve the exact provider observation time when the same raw image
    // is reused for another desktop / monitor during a scheduled cycle.
    let mut observed_utc=source_cache.as_deref()
        .and_then(|cache|cache.observed.get(source.name)).cloned();
    let input=if let Some(path)=cached_input {
        log(format!("复用本周期已下载的 {} 原图（不再次发起网络请求）。",source.name));
        path
    }else{match source.kind {
        SourceKind::Direct | SourceKind::Goes | SourceKind::Epic | SourceKind::Wms => {
            let image_path=temp_file(&folder,"download.jpg",&mut scratch);
            match source.kind {
                SourceKind::Direct=>{
                    download_image(source.home_url,&image_path,70*1048576,cancel,&mut log)?;
                },
                SourceKind::Wms=>{
                    check(cancel)?;
                    log("EUMETSAT: query latest published Meteosat-12 WMS observation time".into());
                    let cap=http::get_text(&sources::wms_capabilities_url(),24*1048576)
                        .map_err(|e|format!("Meteosat GetCapabilities failed: {e}"))?;
                    check(cancel)?;
                    let latest=sources::meteosat_latest_time(&cap)?;
                    log(format!("Meteosat-12 latest advertised observation (UTC): {latest}"));
                    let candidates=sources::wms_retry_times(&latest)?;
                    let mut last_error=String::new();
                    let mut downloaded=false;
                    for (index,time) in candidates.iter().enumerate(){
                        check(cancel)?;
                        if index>0 {
                            log(format!("Meteosat 最新时间片尚不可用；尝试较早时间片 {time}"));
                        }
                        let url=sources::wms_url_at(time)?;
                        let result=download_image(&url,&image_path,70*1048576,cancel,&mut log)
                            .and_then(|_|validate_wms_image(&image_path));
                        match result {
                            Ok(())=>{
                                if index>0 {
                                    log(format!("Meteosat 已回退到可用观测时间 (UTC): {time}"));
                                }
                                observed_utc=Some(time.clone());
                                downloaded=true;
                                break;
                            },
                            Err(error)=>{
                                check(cancel)?;
                                last_error=error;
                                log(format!("Meteosat WMS 时间片 {time} 失败：{last_error}"));
                            },
                        }
                    }
                    if !downloaded {
                        return Err(format!(
                            "Meteosat WMS 最近 {} 个时间片均失败；最后错误：{last_error}",
                            candidates.len()));
                    }
                },
                SourceKind::Goes=>{
                    let links=sources::goes_cdn_urls(source.name)?;
                    log("NOAA direct CDN GeoColor download (no HTML parsing)".into());
                    let mut success=false;
                    let mut error=String::new();
                    for (i,url) in links.iter().enumerate(){
                        check(cancel)?;
                        if i>0{log(format!("Trying lower resolution fallback: {url}"));}
                        match download_image(url,&image_path,70*1048576,cancel,&mut log){
                            Ok(())=>{success=true;break;}
                            Err(e)=>{error=e;log(format!("Download attempt {} failed: {}",i+1,error));}
                        }
                    }
                    if !success{return Err(format!("Both NOAA GeoColor sizes failed: {error}"));}
                    // The fixed-size NOAA CDN URL carries no acquisition time.
                    // Its HTTP Last-Modified is a publication time, NOT an Obs time.
                },
                SourceKind::Epic=>{
                    check(cancel)?;
                    log(format!("Fetch NASA EPIC metadata: {}",source.home_url));
                    let json=http::get_text(source.home_url,2*1048576)?;
                    log(format!("NASA metadata: {} bytes",json.len()));
                    let selected=sources::epic_latest_image(&json)?;
                    log(format!("NASA EPIC latest observation (UTC): {}; images are delayed by provider",
                        selected.observed_at));
                    download_image(&selected.url,&image_path,70*1048576,cancel,&mut log)?;
                    observed_utc=Some(selected.observed_at);
                },
                _=>unreachable!()
            }
            image_path
        },
        SourceKind::Himawari=>{
            check(cancel)?;
            let url=format!("{}/latest.json",source.home_url);
            log(format!("Fetch NICT Himawari timestamp: {url}"));
            let json=http::get_text(&url,512*1024)?;
            let links=sources::himawari_tiles(&json)?;
            // NICT latest.json date is the timestamp used for these 16 tiles.
            observed_utc=serde_json::from_str::<serde_json::Value>(&json).ok()
                .and_then(|metadata|metadata["date"].as_str().map(str::to_owned));
            let output=temp_file(&folder,"himawari.bmp",&mut scratch);
            let mut bgra=vec![0u8;2200*2200*4];
            for (i,url) in links.iter().enumerate(){
                check(cancel)?;
                log(format!("Himawari tile {}/{}",i+1,links.len()));
                let tile=temp_file(&folder,&format!("tile{i}.png"),&mut scratch);
                download_image(url,&tile,8*1048576,cancel,&mut log)?;
                let pix=imaging::load_scaled(&tile,550,None)
                    .map_err(|e|format!("Himawari tile {} decode: {e}",i+1))?;
                let x=(i%4)*550;let y=(i/4)*550;
                for row in 0..550{
                    let dst=((y+row)*2200+x)*4;
                    bgra[dst..dst+2200].copy_from_slice(&pix.bgra[row*2200..(row+1)*2200]);
                }
                let _=fs::remove_file(tile);
            }
            check(cancel)?;
            log("Compose 4x4 Himawari tiles: 2200x2200".into());
            wallpaper::write_bitmap(&output,2200,2200,&bgra)?;
            drop(bgra);
            output
        }
    }};
    check(cancel)?;
    let (w,h)=display.map(|d|(d.width,d.height)).map(Ok).unwrap_or_else(wallpaper::screen_size)?;
    let diameter=wallpaper::diameter(h,&cfg.scale_mode).min(w).max(1);
    log(format!("Screen: {w}x{h}; Earth diameter: {diameter}px"));
    let crop=match source.kind {
        SourceKind::Direct=>Some((65,80,10835,10965)),
        SourceKind::Goes=>Some((24,24,10800,10848)),
        _=>None,
    };
    log("WIC: decode, crop and scale satellite image".into());
    let started=Instant::now();
    let image=imaging::load_scaled(&input,diameter,crop)
        .map_err(|e|format!("WIC image decode failed: {e}"))?;
    log(format!("WIC decode complete ({:.1}s)",started.elapsed().as_secs_f32()));
    check(cancel)?;
    let fingerprint=if matches!(source.kind,SourceKind::Epic|SourceKind::Wms){
        Some(source_fingerprint(&input,source.name,&cfg.scale_mode,cfg.watermark_on,w,h)?)
    }else{None};
    // Cache only validated original satellite files and only sources used by
    // multiple profiles in this cycle. Per-pair rendering remains independent.
    if fresh_download {
        if let Some(cache)=source_cache.as_deref_mut(){
            cache.store(source.name,&input,&folder,observed_utc.as_deref())?;
        }
    }
    // Render to a throwaway scratch path. Publish a stable per-desktop+monitor
    // file only after the image is completely written.
    let path=temp_file(&folder,"render.bmp",&mut scratch);
    log(format!("Compose desktop BMP: {}x{}",w,h));
    if cfg.watermark_on {
        if observed_utc.is_none() {
            log(format!("{}: provider does not expose verified acquisition time; Obs --",source.name));
        }else {
            log(format!("{} observation UTC for watermark: {}",source.name,
                observed_utc.as_deref().unwrap_or("")));
        }
    }
    wallpaper::compose_observed(&path,w,h,&image,cfg.watermark_on,observed_utc.as_deref())?;
    check(cancel)?;
    // Background refresh can target inactive virtual desktops. Check that this
    // GUID still exists, not that it is the foreground desktop.
    if let Some(id)=desktop {
        let snapshot=crate::virtual_desktop::snapshot()?;
        if !snapshot.ids.iter().any(|known|known==id) {
            return Err(format!("Desktop {id} was removed during refresh"));
        }
    }
    check(cancel)?;
    if render_only {
        let id=desktop.ok_or("Missing virtual desktop in pair renderer")?;
        let m=display.ok_or("Missing physical monitor in pair renderer")?;
        let unchanged=fingerprint.as_deref().is_some_and(|fingerprint|
            crate::virtual_cache::unchanged_image(&folder,Some(id),Some(m),
                &cfg.image_source,fingerprint).is_some());
        let stable=crate::virtual_cache::save_identified(&folder,Some(id),Some(m),
            &cfg.image_source,&path,fingerprint.as_deref())?;
        if unchanged {
            log(format!("卫星图源 {} 返回相同照片：保留现有壁纸，不覆盖 BMP。",source.name));
        }
        log(format!("Saved pair image: desktop={} monitor={} source={} path={}",
            id,m.id,cfg.image_source,stable.display()));
        return Ok(stable);
    }
    if let Some(id)=desktop {
        if display.is_some(){
            return Err("Use render_pair for independent virtual desktop and monitor images".into());
        }
        // An attached single physical display produces an explicit desktop+monitor
        // image file. Multiple monitors with one virtual-desktop wallpaper use 'all'.
        let found=crate::monitor::connected().unwrap_or_default();
        let sole=if found.len()==1{found.first()}else{None};
        let unchanged=fingerprint.as_deref().is_some_and(|fingerprint|
            crate::virtual_cache::unchanged_image(&folder,Some(id),sole,
                &cfg.image_source,fingerprint).is_some());
        let stable=crate::virtual_cache::save_identified(&folder,Some(id),sole,
            &cfg.image_source,&path,fingerprint.as_deref())?;
        if unchanged{log(format!("卫星图源 {} 尚无新图：继续使用已有壁纸。",source.name));}
        check(cancel)?;
        if !crate::virtual_desktop::snapshot()?.ids.iter().any(|known|known==id){
            return Err(format!("Desktop {id} no longer exists; skip assignment"));
        }
        log(format!("Windows: SetDesktopWallpaper({id}) -> {}",stable.display()));
        crate::virtual_wallpaper::assign(id,&stable)?;
        log(format!("Assigned desktop {} in {:.1}s",id,begin.elapsed().as_secs_f32()));
        Ok(stable)
    }else{
        let unchanged=fingerprint.as_deref().is_some_and(|fingerprint|
            crate::virtual_cache::unchanged_image(&folder,None,display,
                &cfg.image_source,fingerprint).is_some());
        let stable=crate::virtual_cache::save_identified(&folder,None,display,
            &cfg.image_source,&path,fingerprint.as_deref())?;
        if unchanged{log(format!("卫星图源 {} 尚无新图：继续使用已有壁纸。",source.name));}
        check(cancel)?;
        log(format!("Windows: apply wallpaper from {}",stable.display()));
        match display{
            Some(d)=>crate::monitor::assign(&d.id,&stable)?,
            None=>wallpaper::set_wallpaper(&stable)?,
        }
        log(format!("Wallpaper updated in {:.1}s",begin.elapsed().as_secs_f32()));
        Ok(stable)
    }
}

#[cfg(test)]
mod cycle_cache_tests {
    use super::*;
    #[test]fn image_signature_detects_server_changes_and_respects_settings(){
        let p=std::env::temp_dir().join(format!("cew-fingerprint-{}.jpg",std::process::id()));
        fs::write(&p,b"IMAGE A").unwrap();
        let original=source_fingerprint(&p,"NASA EPIC","黄金比例",false,1600,900).unwrap();
        assert_eq!(original,source_fingerprint(&p,"NASA EPIC","黄金比例",false,1600,900).unwrap());
        assert_ne!(original,source_fingerprint(&p,"NASA EPIC","铺满屏幕",false,1600,900).unwrap());
        assert_ne!(original,source_fingerprint(&p,"NASA EPIC","黄金比例",true,1600,900).unwrap());
        assert_ne!(original,source_fingerprint(&p,"NASA EPIC","黄金比例",false,1920,1080).unwrap());
        fs::write(&p,b"IMAGE B").unwrap();
        assert_ne!(original,source_fingerprint(&p,"NASA EPIC","黄金比例",false,1600,900).unwrap());
        fs::remove_file(&p).unwrap();
    }

    #[test]fn cycle_cache_retains_provider_observation_time(){
        let dir=std::env::temp_dir().join(format!("cew-obstime-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let src=dir.join("raw.jpg");
        fs::write(&src,b"raw pixels").unwrap();
        {
            let mut cache=CycleSourceCache::for_sources(["NASA EPIC","NASA EPIC"]);
            cache.store("NASA EPIC",&src,&dir,Some("2026-10-10 10:50:00")).unwrap();
            assert_eq!(cache.observed.get("NASA EPIC").map(String::as_str),
                Some("2026-10-10 10:50:00"));
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]fn same_source_reused_and_temp_files_cleaned(){
        let dir=std::env::temp_dir().join(format!("cew-cycle-cache-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let src=dir.join("input.jpg");
        fs::write(&src,b"mock satellite bytes").unwrap();
        let saved;
        {
            let mut cache=CycleSourceCache::for_sources(["GOES-East","GOES-East","NASA EPIC"]);
            assert!(cache.repeated.contains("GOES-East"));
            assert!(!cache.repeated.contains("NASA EPIC"));
            cache.store("GOES-East",&src,&dir,None).unwrap();
            saved=cache.find("GOES-East").unwrap().clone();
            assert!(saved.exists());
            cache.store("GOES-East",&src,&dir,None).unwrap();
            assert_eq!(cache.images.len(),1);
            cache.store("NASA EPIC",&src,&dir,None).unwrap();
            assert_eq!(cache.images.len(),1);
        }
        assert!(!saved.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
