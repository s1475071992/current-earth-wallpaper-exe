//! Last rendered wallpaper cache for each virtual desktop + physical monitor + source.
//! Cache restores instantly on desktop change; the downloader refreshes afterward.
use std::{fs,path::{Path,PathBuf},sync::atomic::{AtomicBool,Ordering}};
use crate::{config,monitor::{self,Monitor},wallpaper,virtual_desktop};

fn hash(s:&str)->u64{
    let mut x=0xcbf29ce484222325u64;
    for c in s.bytes(){x=(x^(c as u64)).wrapping_mul(0x100000001b3);}
    x
}
fn dir()->PathBuf{config::app_dir().join("virtual-cache")}
fn cached_path(id:&str,monitor:Option<&Monitor>,source:&str)->PathBuf{
    let screen=monitor.map(|m|m.id.as_str()).unwrap_or("all");
    dir().join(format!("vd-{:016x}-{:016x}-{:016x}.bmp",hash(id),hash(screen),hash(source)))
}
fn guard(id:&str,cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Relaxed){return Err("Desktop change cancelled".into())}
    let current=virtual_desktop::snapshot()?.current;
    if current.as_deref()!=Some(id){return Err("Active virtual desktop has changed".into())}
    Ok(())
}
pub fn restore(id:&str,monitor:Option<&Monitor>,source:&str,cancel:&AtomicBool)->Result<(),String>{
    guard(id,cancel)?;
    let cached=cached_path(id,monitor,source);
    if !cached.is_file(){return Err("No cached image".into())}
    guard(id,cancel)?;
    match monitor {
        Some(m)=>monitor::assign(&m.id,&cached),
        None=>wallpaper::set_wallpaper(&cached)
    }
}
pub fn save(id:&str,monitor:Option<&Monitor>,source:&str,image:&Path)->Result<(),String>{
    fs::create_dir_all(dir()).map_err(|e|e.to_string())?;
    let dst=cached_path(id,monitor,source);
    let tmp=dst.with_extension(format!("{}.tmp",std::process::id()));
    let result=(||{
        fs::copy(image,&tmp).map_err(|e|e.to_string())?;
        if dst.exists(){fs::remove_file(&dst).map_err(|e|e.to_string())?;}
        fs::rename(&tmp,&dst).map_err(|e|e.to_string())?;
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_file(&tmp);}
    // Keep cache bounded; images for old desktops may be re-downloaded.
    trim(48);
    result
}
fn trim(keep:usize){
    let Ok(list)=fs::read_dir(dir())else{return;};
    let mut bmp=Vec::new();
    for entry in list.flatten(){
        let path=entry.path();
        if !path.file_name().and_then(|n|n.to_str()).is_some_and(|n|n.starts_with("vd-")&&n.ends_with(".bmp")){continue}
        let modified=entry.metadata().ok().and_then(|m|m.modified().ok());
        bmp.push((modified,path));
    }
    bmp.sort_by_key(|(time,_)|*time);
    let remove=bmp.len().saturating_sub(keep);
    for (_,path) in bmp.into_iter().take(remove){let _=fs::remove_file(path);}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn desktop_monitor_source_cache_key_is_separate(){
        let a=cached_path("desktop-1",None,"GOES-East");
        let b=cached_path("desktop-2",None,"GOES-East");
        let c=cached_path("desktop-1",None,"GOES-West");
        let d=cached_path("desktop-1",Some(&Monitor{id:"monitor-1".into(),width:1024,height:768}),"GOES-East");
        assert_ne!(a,b);assert_ne!(a,c);assert_ne!(a,d);
    }
}
