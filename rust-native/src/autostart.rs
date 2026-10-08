//! Per-user Run key; the value invokes this EXE with --autostart.
use windows_sys::Win32::System::Registry::*;
fn w(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
const SUBKEY:&str="Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE:&str="CurrentEarthWallpaperNative";
pub fn enabled()->bool{
    unsafe {
        let mut key=std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER,w(SUBKEY).as_ptr(),0,KEY_READ,&mut key)!=0{return false;}
        let exists=RegQueryValueExW(key,w(VALUE).as_ptr(),std::ptr::null_mut(),
            std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut())==0;
        RegCloseKey(key);exists
    }
}
pub fn set(enabled:bool)->Result<(),String>{
    unsafe {
        let mut key=std::ptr::null_mut();
        let rc=RegCreateKeyExW(HKEY_CURRENT_USER,w(SUBKEY).as_ptr(),0,
            std::ptr::null_mut(),0,KEY_SET_VALUE,std::ptr::null(),&mut key,std::ptr::null_mut());
        if rc!=0{return Err(format!("Registry open error {rc}"));}
        let name=w(VALUE);
        let rc=if enabled{
            let exe=std::env::current_exe().map_err(|e|e.to_string())?;
            let value=w(&format!("\"{}\" --autostart",exe.display()));
            RegSetValueExW(key,name.as_ptr(),0,REG_SZ,
                value.as_ptr() as *const u8,(value.len()*2) as u32)
        }else{
            let rc=RegDeleteValueW(key,name.as_ptr());
            if rc==2 {0}else{rc}
        };
        RegCloseKey(key);
        if rc==0{Ok(())}else{Err(format!("Registry update error {rc}"))}
    }
}
