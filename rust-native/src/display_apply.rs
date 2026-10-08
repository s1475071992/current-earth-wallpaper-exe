//! Best-effort virtual-desktop x physical-monitor wallpaper emulation.
//! There is no supported Windows API that directly assigns an independent monitor
//! wallpaper to an INACTIVE virtual desktop. We prepare files for all pairs,
//! then re-apply the active desktop's monitor files on a confirmed switch.
//! No download and no rendering ever occur in this module.
use std::{path::Path,sync::{Mutex,atomic::{AtomicU64,Ordering}}};
use crate::{config::AppConfig,monitor::{self,Monitor},virtual_cache,virtual_cycle,virtual_desktop,virtual_wallpaper};

/// Windows can natively keep separate virtual-desktop wallpapers when only
/// one physical monitor is connected. Multi-monitor virtual desktops have to
/// use per-display reapplication as an emulation instead.
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
enum WallpaperMode { NoDisplay, SingleMonitorNative, MultipleMonitorsEmulated }
fn select_mode(n:usize)->WallpaperMode {
    match n { 0=>WallpaperMode::NoDisplay, 1=>WallpaperMode::SingleMonitorNative,
        _=>WallpaperMode::MultipleMonitorsEmulated }
}
static APPLY_LOCK:Mutex<()> = Mutex::new(());
static GENERATION:AtomicU64=AtomicU64::new(0);

/// Every switch invalidates prior re-apply requests (including ones waiting for
/// the COM assignment mutex).
pub fn next_generation()->u64{
    GENERATION.fetch_add(1,Ordering::SeqCst)+1
}
pub fn invalidate(){
    GENERATION.fetch_add(1,Ordering::SeqCst);
}
fn check_current(id:&str,generation:u64)->Result<(),String>{
    if GENERATION.load(Ordering::SeqCst)!=generation {
        return Err("Superseded by newer desktop switch".into());
    }
    if virtual_desktop::snapshot()?.current.as_deref()!=Some(id){
        return Err("Another virtual desktop is now active".into());
    }
    Ok(())
}
/// Reapply *only local images* after a switch or display-topology change.
/// With exactly one connected display, populate Windows' actual per-virtual-
/// desktop wallpaper storage via the verified GUID COM setter for ALL desktops.
/// This survives ordinary virtual-desktop switches even if Explorer resets the
/// global/monitor wallpaper during a hot unplug. With two or more monitors we
/// keep the per-monitor emulation and assign only the active desktop's images.
pub fn reapply(id:&str,cfg:&AppConfig,generation:u64)->Result<(usize,usize),String>{
    let folder=if cfg.save_path.trim().is_empty(){crate::config::app_dir().join("wallpapers")}
        else{std::path::PathBuf::from(&cfg.save_path)};
    let monitors=monitor::connected()?;
    let mode=select_mode(monitors.len());
    if mode==WallpaperMode::NoDisplay{return Err("No connected physical displays".into());}
    let _guard=APPLY_LOCK.lock().map_err(|_|"Monitor wallpaper lock poisoned")?;
    check_current(id,generation)?;
    let mut applied=0usize;
    let mut missing=0usize;
    match mode{
        WallpaperMode::SingleMonitorNative=>{
            let monitor=&monitors[0];
            let snapshot=virtual_desktop::snapshot()?;
            // The user requested no re-download on switch. Native COM only
            // assigns already-rendered stable BMPs; missing profiles wait for
            // the normal scheduled background refresh.
            for desktop_id in &snapshot.ids{
                check_current(id,generation)?;
                let source=virtual_cycle::source_for_pair(cfg,desktop_id,&monitor.id);
                if let Some(path)=virtual_cache::cached_pair(&folder,desktop_id,monitor,source){
                    virtual_wallpaper::assign(desktop_id,&path)?;
                    applied+=1;
                }else{missing+=1;}
            }
        },
        WallpaperMode::MultipleMonitorsEmulated=>{
            for monitor in &monitors{
                check_current(id,generation)?;
                let source=virtual_cycle::source_for_pair(cfg,id,&monitor.id);
                if let Some(path)=virtual_cache::cached_pair(&folder,id,monitor,source){
                    monitor::assign(&monitor.id,&path)?;
                    applied+=1;
                }else{missing+=1;}
            }
        },
        WallpaperMode::NoDisplay=>unreachable!(),
    }
    Ok((applied,missing))
}
/// A background render may update an inactive virtual desktop. On a SINGLE
/// monitor, bind its new image to that native virtual-desktop GUID even when
/// inactive. On MULTIPLE monitors, apply only when this is the current virtual
/// desktop; other pairs are prepared for the next switch.
pub fn apply_if_current(id:&str,monitor:&Monitor,path:&Path)->Result<bool,String>{
    let _guard=APPLY_LOCK.lock().map_err(|_|"Monitor wallpaper lock poisoned")?;
    let connected=monitor::connected()?;
    if !connected.iter().any(|m|m.id==monitor.id){return Ok(false);}
    match select_mode(connected.len()){
        WallpaperMode::SingleMonitorNative=>{
            if !virtual_desktop::snapshot()?.ids.iter().any(|known|known==id){return Ok(false);}
            virtual_wallpaper::assign(id,path)?;
            Ok(true)
        },
        WallpaperMode::MultipleMonitorsEmulated=>{
            if virtual_desktop::snapshot()?.current.as_deref()!=Some(id){return Ok(false);}
            monitor::assign(&monitor.id,path)?;
            Ok(true)
        },
        WallpaperMode::NoDisplay=>Ok(false),
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn hot_unplug_switches_from_monitor_emulation_to_native_per_desktop(){
        assert_eq!(select_mode(2),WallpaperMode::MultipleMonitorsEmulated);
        assert_eq!(select_mode(1),WallpaperMode::SingleMonitorNative);
        assert_eq!(select_mode(0),WallpaperMode::NoDisplay);
        assert_eq!(select_mode(3),WallpaperMode::MultipleMonitorsEmulated);
    }
    #[test]fn generation_monotonic(){
        let first=next_generation();
        assert_eq!(next_generation(),first+1);
    }
}
