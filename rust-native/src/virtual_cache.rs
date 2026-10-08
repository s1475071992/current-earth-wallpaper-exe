//! Exactly one stable BMP per virtual desktop + physical display profile.
//! All files live in the folder chosen in the UI, not in an invisible secondary cache.
use std::{fs,path::{Path,PathBuf},sync::atomic::{AtomicBool,Ordering},time::{Duration,SystemTime,UNIX_EPOCH}};
use serde::{Serialize,Deserialize};
use crate::{monitor::Monitor,virtual_desktop,virtual_wallpaper};

fn hash(text:&str)->u64 {
    let mut h=0xcbf29ce484222325u64;
    for b in text.bytes(){h=(h^(b as u64)).wrapping_mul(0x100000001b3);}
    h
}
fn key(id:Option<&str>,monitor:Option<&Monitor>)->String{
    let desk=id.unwrap_or("global");
    let screen=monitor.map(|m|format!("{:016x}",hash(&m.id)))
        .unwrap_or_else(||"all".into());
    format!("cew-desktop-{desk}_monitor-{screen}")
}
pub fn target(folder:&Path,id:Option<&str>,monitor:Option<&Monitor>)->PathBuf{
    folder.join(format!("{}.bmp",key(id,monitor)))
}
fn metadata_path(bmp:&Path)->PathBuf{bmp.with_extension("meta.json")}
#[derive(Serialize,Deserialize)]
struct Meta {source:String, modified_unix:u64}
fn meta_valid(path:&Path,source:&str)->bool {
    fs::read(metadata_path(path)).ok()
        .and_then(|b|serde_json::from_slice::<Meta>(&b).ok())
        .is_some_and(|m|m.source==source)
}
/// Cached stable BMP for one pair, regardless of age. Used only for fast
/// reapplication after switching virtual desktops: never fetch network data.
pub fn cached_pair(folder:&Path,id:&str,monitor:&Monitor,source:&str)->Option<PathBuf>{
    let path=target(folder,Some(id),Some(monitor));
    if path.is_file() && meta_valid(&path,source){Some(path)}else{None}
}
pub fn recent(folder:&Path,id:&str,monitor:Option<&Monitor>,source:&str,minutes:u32)->Option<PathBuf>{
    let bmp=target(folder,Some(id),monitor);
    if !bmp.is_file()||!meta_valid(&bmp,source){return None;}
    let age=fs::metadata(&bmp).ok()?.modified().ok()?.elapsed().ok()?;
    (age<Duration::from_secs(minutes as u64*60)).then_some(bmp)
}
pub fn check(id:&str,cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Relaxed){return Err("Cancelled".into())}
    let available=virtual_desktop::snapshot()?.ids;
    if !available.iter().any(|x|x==id){return Err("Virtual desktop no longer exists".into());}
    Ok(())
}
pub fn restore(folder:&Path,id:&str,monitor:Option<&Monitor>,source:&str,cancel:&AtomicBool)->Result<PathBuf,String>{
    check(id,cancel)?;
    // In the virtual-desktop-only mode a single physical display may have an
    // explicit fixed BMP path. The private COM method still targets the desktop.
    let bmp=target(folder,Some(id),monitor);
    if !bmp.is_file() || !meta_valid(&bmp,source){return Err("No matching saved wallpaper".into())}
    virtual_wallpaper::assign(id,&bmp)?;
    Ok(bmp)
}
#[link(name="kernel32")]
unsafe extern "system" {
    fn ReplaceFileW(replaced:*const u16,replacement:*const u16,backup:*const u16,flags:u32,
        exclude:*mut std::ffi::c_void,reserved:*mut std::ffi::c_void)->i32;
}
fn replace_file(src:&Path,dest:&Path)->Result<(),String>{
    use std::os::windows::ffi::OsStrExt;
    if !dest.exists(){return fs::rename(src,dest).map_err(|e|e.to_string());}
    let to:Vec<u16>=dest.as_os_str().encode_wide().chain(Some(0)).collect();
    let from:Vec<u16>=src.as_os_str().encode_wide().chain(Some(0)).collect();
    // ReplaceFileW preserves an existing path without leaving a partially written BMP.
    // If Explorer denies replacement, preserve old image rather than deleting it.
    for attempt in 0..3 {
        let done=unsafe{ReplaceFileW(to.as_ptr(),from.as_ptr(),std::ptr::null(),0,
            std::ptr::null_mut(),std::ptr::null_mut())};
        if done!=0{return Ok(())}
        if attempt<2{std::thread::sleep(Duration::from_millis(180));}
    }
    Err(format!("Windows blocked atomic replacement of existing wallpaper {:?}: {}",
        dest,std::io::Error::last_os_error()))
}
/// Render has already completed into a scratch BMP. Publish one named file per
/// profile atomically, then let the caller explicitly re-apply its pathname.
pub fn save(folder:&Path,id:Option<&str>,monitor:Option<&Monitor>,source:&str,rendered:&Path)->Result<PathBuf,String>{
    fs::create_dir_all(folder).map_err(|e|e.to_string())?;
    let dest=target(folder,id,monitor);
    let tmp=folder.join(format!(".cew-{}-publish-{}.tmp",
        std::process::id(),hash(&dest.to_string_lossy())));
    fs::copy(rendered,&tmp).map_err(|e|e.to_string())?;
    if let Err(e)=replace_file(&tmp,&dest){
        let _=fs::remove_file(&tmp);
        return Err(e);
    }
    let meta=Meta{source:source.into(),
        modified_unix:SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()};
    let metapath=metadata_path(&dest);
    // Metadata is advisory; failure must NOT destroy the updated wallpaper.
    fs::write(&metapath,serde_json::to_vec(&meta).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    Ok(dest)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn replacing_a_profile_reuses_one_bitmap_without_leaking_files(){
        let folder=std::env::temp_dir().join(format!(
            "cew-v1-stable-profile-{}-{:?}",std::process::id(),std::thread::current().id()));
        fs::create_dir_all(&folder).unwrap();
        let source1=folder.join("before.bmp");
        let source2=folder.join("after.bmp");
        fs::write(&source1,b"BMP_VERSION_ONE").unwrap();
        fs::write(&source2,b"BMP_VERSION_TWO").unwrap();
        let desktop="12345678-1234-1234-1234-123456789abc";
        let display=Monitor{id:"MONITOR_ONE".into(),width:1920,height:1080};
        let first=save(&folder,Some(desktop),Some(&display),"GOES-East",&source1).unwrap();
        assert_eq!(fs::read(&first).unwrap(),b"BMP_VERSION_ONE");
        assert!(meta_valid(&first,"GOES-East"));
        let second=save(&folder,Some(desktop),Some(&display),"NASA EPIC",&source2).unwrap();
        assert_eq!(first,second);
        assert_eq!(fs::read(&second).unwrap(),b"BMP_VERSION_TWO");
        assert!(meta_valid(&second,"NASA EPIC"));
        assert!(!meta_valid(&second,"GOES-East"));
        let count=fs::read_dir(&folder).unwrap().flatten().filter(|e|
            e.file_name().to_string_lossy().starts_with("cew-desktop-") &&
            e.path().extension().is_some_and(|x|x=="bmp")
        ).count();
        assert_eq!(count,1,"Only one wallpaper BMP per pair");
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]fn one_path_per_pair_even_as_source_changes(){
        let root=Path::new(r"C:\Wallpapers");
        let id="12345678-1234-1234-1234-123456789abc";
        let a=Monitor{id:"MONITOR_A".into(),width:1920,height:1080};
        let b=Monitor{id:"MONITOR_B".into(),width:2560,height:1440};
        assert_eq!(target(root,Some(id),Some(&a)),target(root,Some(id),Some(&a)));
        assert_ne!(target(root,Some(id),Some(&a)),target(root,Some(id),Some(&b)));
        assert_ne!(target(root,Some(id),Some(&a)),target(root,Some("00000000-0000-0000-0000-000000000000"),Some(&a)));
        assert!(target(root,Some(id),Some(&a)).to_string_lossy().contains(id));
    }
}
