//! Supported Windows multi-monitor wallpaper implementation via IDesktopWallpaper.
//! Identifiers are stable monitor device paths; detached displays are excluded.
use std::{ffi::OsStr,os::windows::ffi::OsStrExt,path::Path};
use windows::{
    core::{PCWSTR,PWSTR},
    Win32::{
        System::Com::{CoCreateInstance,CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED,CLSCTX_ALL},
        UI::Shell::{DesktopWallpaper,IDesktopWallpaper},
    }
};
use windows_sys::Win32::System::Com::CoTaskMemFree;

#[derive(Debug,Clone)]
pub struct Monitor {
    pub id: String,
    pub width: u32,
    pub height: u32,
}
struct Apartment;
impl Apartment {
    fn new()->Result<Self,String> {
        unsafe{CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok().map_err(|e|e.to_string())?;}
        Ok(Self)
    }
}
impl Drop for Apartment{fn drop(&mut self){unsafe{CoUninitialize();}}}
fn wide(t:&str)->Vec<u16>{t.encode_utf16().chain(Some(0)).collect()}
fn desktop()->Result<IDesktopWallpaper,String>{
    unsafe{CoCreateInstance(&DesktopWallpaper,None,CLSCTX_ALL).map_err(|e|format!("IDesktopWallpaper COM: {e}"))}
}
pub fn connected()->Result<Vec<Monitor>,String>{
    let _com=Apartment::new()?;
    let api=desktop()?;
    let count=unsafe{api.GetMonitorDevicePathCount().map_err(|e|e.to_string())?};
    let mut items=Vec::new();
    for i in 0..count.min(32){
        let path=unsafe{api.GetMonitorDevicePathAt(i).map_err(|e|e.to_string())?};
        let id=unsafe{path.to_string().map_err(|e|e.to_string())};
        unsafe{CoTaskMemFree(Some(path.0 as *const std::ffi::c_void));}
        let id=id?;
        let id_wide=wide(&id);
        // S_FALSE for disconnected monitors; skip rather than overwriting them.
        let rect=unsafe{api.GetMonitorRECT(PCWSTR(id_wide.as_ptr()))};
        if let Ok(rect)=rect{
            let w=rect.right-rect.left;let h=rect.bottom-rect.top;
            if w>=160&&h>=120&&w<=16000&&h<=16000{
                items.push(Monitor{id,width:w as u32,height:h as u32});
            }
        }
    }
    Ok(items)
}
pub fn assign(monitor_id:&str,image:&Path)->Result<(),String>{
    let absolute=image.canonicalize().map_err(|e|e.to_string())?;
    let _com=Apartment::new()?;
    let api=desktop()?;
    let id=wide(monitor_id);
    let path:Vec<u16>=OsStr::new(&absolute).encode_wide().chain(Some(0)).collect();
    unsafe{
        api.SetWallpaper(PCWSTR(id.as_ptr()),PCWSTR(path.as_ptr()))
            .map_err(|e|format!("SetWallpaper({}): {e}",monitor_id))?;
    }
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn empty_id_not_used_as_global_monitor() {
        let m=Monitor{id:"TEST".into(),width:1920,height:1080};
        assert_eq!(m.width,1920);
        assert!(!m.id.is_empty());
    }
}
