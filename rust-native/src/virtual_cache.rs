//! Last rendered wallpapers keyed by (virtual desktop, display, source).
//! Keep uniquely named BMPs because Windows Explorer may retain old image paths.
use std::{fs,path::{Path,PathBuf},sync::atomic::{AtomicBool,Ordering},time::{Duration,SystemTime,UNIX_EPOCH}};
use crate::{config,monitor::Monitor,virtual_desktop,virtual_wallpaper};
fn hash(s:&str)->u64{
    let mut x=0xcbf29ce484222325u64;
    for c in s.bytes(){x=(x^(c as u64)).wrapping_mul(0x100000001b3);}
    x
}
fn dir()->PathBuf{config::app_dir().join("virtual-cache")}
fn prefix(id:&str,monitor:Option<&Monitor>,source:&str)->String{
    let screen=monitor.map(|m|m.id.as_str()).unwrap_or("all");
    format!("vd-{:016x}-{:016x}-{:016x}-",hash(id),hash(screen),hash(source))
}
pub fn latest(id:&str,monitor:Option<&Monitor>,source:&str)->Option<PathBuf>{
    let prefix=prefix(id,monitor,source);
    fs::read_dir(dir()).ok()?
        .flatten()
        .map(|e|e.path())
        .filter(|p|p.file_name().and_then(|n|n.to_str())
            .is_some_and(|n|n.starts_with(&prefix)&&n.ends_with(".bmp")))
        .filter_map(|p|{
            let modified=fs::metadata(&p).ok()?.modified().ok()?;
            Some((modified,p))
        })
        .max_by_key(|x|x.0)
        .map(|(_,p)|p)
}
pub fn recent(id:&str,monitor:Option<&Monitor>,source:&str,minutes:u32)->Option<PathBuf>{
    let path=latest(id,monitor,source)?;
    let age=fs::metadata(&path).ok()?.modified().ok()?.elapsed().ok()?;
    (age<Duration::from_secs(minutes as u64*60)).then_some(path)
}
fn guard(id:&str,cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Relaxed){return Err("Desktop change cancelled".into())}
    // Cached wallpaper may be assigned to an inactive desktop without switching.
    let ids=virtual_desktop::snapshot()?.ids;
    if !ids.iter().any(|known|known==id) {
        return Err("Virtual desktop no longer exists".into());
    }
    Ok(())
}
pub fn restore(id:&str,monitor:Option<&Monitor>,source:&str,cancel:&AtomicBool)->Result<PathBuf,String>{
    guard(id,cancel)?;
    if monitor.is_some(){return Err("Per-monitor + per-virtual-desktop composite not implemented".into());}
    let path=latest(id,monitor,source).ok_or("No cached image")?;
    guard(id,cancel)?;
    virtual_wallpaper::assign(id,&path)?;
    Ok(path)
}
pub fn save(id:&str,monitor:Option<&Monitor>,source:&str,image:&Path)->Result<PathBuf,String>{
    fs::create_dir_all(dir()).map_err(|e|e.to_string())?;
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let dest=dir().join(format!("{}{}.bmp",prefix(id,monitor,source),stamp));
    let tmp=dest.with_extension(format!("{}.tmp",std::process::id()));
    let result=(||->Result<(),String>{
        fs::copy(image,&tmp).map_err(|e|e.to_string())?;
        fs::rename(&tmp,&dest).map_err(|e|e.to_string())?;
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_file(&tmp);return Err(result.err().unwrap());}
    trim(48);
    Ok(dest)
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
    #[test]fn desktop_monitor_source_keys_distinct(){
        let a=prefix("desktop-1",None,"GOES-East");
        let b=prefix("desktop-2",None,"GOES-East");
        let c=prefix("desktop-1",None,"GOES-West");
        let d=prefix("desktop-1",Some(&Monitor{id:"monitor-1".into(),width:1024,height:768}),"GOES-East");
        assert_ne!(a,b);assert_ne!(a,c);assert_ne!(a,d);
    }
}
