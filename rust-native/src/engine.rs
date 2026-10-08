//! End-to-end provider pipeline. Temporary downloads never persist after the cycle.
use std::{path::{Path,PathBuf},fs};
use crate::{config::{AppConfig,app_dir},http,imaging,sources::{self,SourceKind},wallpaper};
struct Scratch(Vec<PathBuf>);
impl Scratch { fn new()->Self{Self(Vec::new())} fn add(&mut self,path:PathBuf)->PathBuf{self.0.push(path.clone());path} }
impl Drop for Scratch {fn drop(&mut self){for p in &self.0{let _=fs::remove_file(p);}}}
fn stamp()->u128{std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()}
fn temp_file(folder:&Path,name:&str,scratch:&mut Scratch)->PathBuf {
    scratch.add(folder.join(format!(".cew-{}-{}-{name}",std::process::id(),stamp())))
}
pub fn run_once(cfg:&AppConfig)->Result<PathBuf,String>{
    let folder=if cfg.save_path.trim().is_empty(){app_dir().join("wallpapers")}
        else{PathBuf::from(&cfg.save_path)};
    fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
    let source=sources::by_name(&cfg.image_source);
    let mut scratch=Scratch::new();
    let input=match source.kind {
        SourceKind::Direct | SourceKind::Goes | SourceKind::Epic | SourceKind::Wms =>{
            let url=match source.kind {
                SourceKind::Direct=>source.home_url.to_string(),
                SourceKind::Wms=>sources::wms_url(),
                SourceKind::Goes=>{
                    let html=http::get_text(source.home_url,2*1024*1024)?;
                    sources::goes_image_url(&html)?
                },
                SourceKind::Epic=>{
                    let json=http::get_text(source.home_url,2*1024*1024)?;
                    sources::epic_image_url(&json)?
                },
                _=>unreachable!()
            };
            let path=temp_file(&folder,"download.jpg",&mut scratch);
            http::download(&url,&path,70*1024*1024)?;
            path
        }
        SourceKind::Himawari=>{
            let json=http::get_text(&format!("{}/latest.json",source.home_url),512*1024)?;
            let urls=sources::himawari_tiles(&json)?;
            let output=temp_file(&folder,"himawari.bmp",&mut scratch);
            // Tile memory is released after each copy; the final assembled disk is 18.5 MiB.
            let mut bgra=vec![0u8;2200*2200*4];
            for (i,url) in urls.iter().enumerate(){
                let tile=temp_file(&folder,&format!("tile{i}.png"),&mut scratch);
                http::download(url,&tile,8*1024*1024)?;
                let pix=imaging::load_scaled(&tile,550,None)?;
                let x=(i%4)*550;let y=(i/4)*550;
                for row in 0..550 {
                    let dst=((y+row)*2200+x)*4;
                    bgra[dst..dst+2200].copy_from_slice(&pix.bgra[row*2200..(row+1)*2200]);
                }
                let _=fs::remove_file(tile);
            }
            wallpaper::write_bitmap(&output,2200,2200,&bgra)?;
            output
        }
    };
    let (w,h)=wallpaper::screen_size()?;
    let diameter=wallpaper::diameter(h,&cfg.scale_mode).min(w).max(1);
    let crop=match source.kind {
        SourceKind::Direct=>Some((65,80,10835,10965)),
        SourceKind::Goes=>Some((24,24,10800,10848)),
        _=>None,
    };
    let image=imaging::load_scaled(&input,diameter,crop)?;
    drop(scratch);
    let name=format!("cew-{}-{}.bmp",std::process::id(),stamp());
    let wallpaper_path=folder.join(name);
    wallpaper::compose(&wallpaper_path,w,h,&image,cfg.watermark_on)?;
    wallpaper::set_wallpaper(&wallpaper_path)?;
    wallpaper::prune_own_wallpapers(&folder,&wallpaper_path,5);
    Ok(wallpaper_path)
}
