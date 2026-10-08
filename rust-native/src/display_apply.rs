//! Best-effort virtual-desktop x physical-monitor wallpaper emulation.
//! There is no supported Windows API that directly assigns an independent monitor
//! wallpaper to an INACTIVE virtual desktop. We prepare files for all pairs,
//! then re-apply the active desktop's monitor files on a confirmed switch.
//! No download and no rendering ever occur in this module.
use std::{path::Path,sync::{Mutex,atomic::{AtomicU64,Ordering}}};
use crate::{config::AppConfig,monitor::{self,Monitor},virtual_cache,virtual_cycle,virtual_desktop};

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
/// Reapply available files to all displays without any network or image decode.
pub fn reapply(id:&str,cfg:&AppConfig,generation:u64)->Result<(usize,usize),String>{
    let folder=if cfg.save_path.trim().is_empty(){crate::config::app_dir().join("wallpapers")}
        else{std::path::PathBuf::from(&cfg.save_path)};
    // Enumerate in the dedicated worker thread to avoid blocking the GUI.
    let monitors=monitor::connected()?;
    if monitors.is_empty(){return Err("No connected physical displays".into())}
    let _guard=APPLY_LOCK.lock().map_err(|_|"Monitor wallpaper lock poisoned")?;
    check_current(id,generation)?;
    let mut applied=0usize;
    let mut missing=0usize;
    for monitor in &monitors{
        check_current(id,generation)?;
        let source=virtual_cycle::source_for_pair(cfg,id,&monitor.id);
        match virtual_cache::cached_pair(&folder,id,monitor,source){
            Some(path)=>{
                monitor::assign(&monitor.id,&path)?;
                applied+=1;
            },
            None=>missing+=1,
        }
    }
    Ok((applied,missing))
}
/// A scheduled background render may update one currently displayed profile.
/// Assign only if the foreground desktop still matches, and serialize with
/// switch requests. Inactive desktop profiles are never displayed here.
pub fn apply_if_current(id:&str,monitor:&Monitor,path:&Path)->Result<bool,String>{
    let _guard=APPLY_LOCK.lock().map_err(|_|"Monitor wallpaper lock poisoned")?;
    if virtual_desktop::snapshot()?.current.as_deref()!=Some(id){return Ok(false);}
    monitor::assign(&monitor.id,path)?;
    Ok(true)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn generation_monotonic(){
        let first=next_generation();
        assert_eq!(next_generation(),first+1);
    }
}
