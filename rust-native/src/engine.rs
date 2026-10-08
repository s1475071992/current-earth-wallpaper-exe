//! End-to-end provider pipeline. Per-stage events are sent to the GUI and a local log.
use std::{
    path::{Path, PathBuf}, fs,
    sync::atomic::{AtomicBool,Ordering},
    time::{Instant,SystemTime,UNIX_EPOCH},
};
use crate::{config::{AppConfig,app_dir},http,imaging,sources::{self,SourceKind},wallpaper};

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
pub fn run_once(cfg:&AppConfig,cancel:&AtomicBool,mut log:impl FnMut(String))->Result<PathBuf,String>{
    let begin=Instant::now();
    let folder=if cfg.save_path.trim().is_empty(){app_dir().join("wallpapers")}
        else{PathBuf::from(&cfg.save_path)};
    fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
    let source=sources::by_name(&cfg.image_source);
    log(format!("Start update: {} | interval={} minutes",source.name,cfg.interval_minutes));
    log(format!("Output folder: {}",folder.display()));
    let mut scratch=Scratch::new();
    let input=match source.kind {
        SourceKind::Direct | SourceKind::Goes | SourceKind::Epic | SourceKind::Wms => {
            let image_path=temp_file(&folder,"download.jpg",&mut scratch);
            match source.kind {
                SourceKind::Direct|SourceKind::Wms=>{
                    let url=if source.kind==SourceKind::Wms {sources::wms_url()}else{source.home_url.to_string()};
                    download_image(&url,&image_path,70*1048576,cancel,&mut log)?;
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
                },
                SourceKind::Epic=>{
                    check(cancel)?;
                    log(format!("Fetch NASA EPIC metadata: {}",source.home_url));
                    let json=http::get_text(source.home_url,2*1048576)?;
                    log(format!("NASA metadata: {} bytes",json.len()));
                    let url=sources::epic_image_url(&json)?;
                    download_image(&url,&image_path,70*1048576,cancel,&mut log)?;
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
    };
    check(cancel)?;
    let (w,h)=wallpaper::screen_size()?;
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
    drop(scratch);
    let path=folder.join(format!("cew-{}-{}.bmp",std::process::id(),stamp()));
    log(format!("Compose desktop BMP: {}x{}",w,h));
    wallpaper::compose(&path,w,h,&image,cfg.watermark_on)?;
    check(cancel)?;
    log("Windows: SystemParametersInfoW(SPI_SETDESKWALLPAPER)".into());
    wallpaper::set_wallpaper(&path)?;
    wallpaper::prune_own_wallpapers(&folder,&path,5);
    log(format!("Wallpaper updated successfully in {:.1}s",begin.elapsed().as_secs_f32()));
    Ok(path)
}
