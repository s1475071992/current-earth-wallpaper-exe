//! Windows 11 private COM virtual-desktop wallpaper API (builds 26100..=26399).
//! Isolated in a subprocess: an ABI mismatch must not crash the wallpaper GUI.
//! Unofficial API; no Windows desktop mutations if COM/desktop GUID verification fails.
use std::{ffi::c_void, fs, path::{Path,PathBuf}, process::Command, ptr::{null, null_mut}, time::{Duration,Instant}};
use windows_sys::Win32::System::Registry::*;

#[repr(C)]
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct Guid{ data1:u32,data2:u16,data3:u16,data4:[u8;8] }
fn guid(input:&str)->Result<Guid,String>{
    let groups:Vec<_>=input.split('-').collect();
    if groups.len()!=5 || groups.iter().map(|s|s.len()).collect::<Vec<_>>()!=[8,4,4,4,12]{
        return Err("Invalid desktop GUID".into());
    }
    let a=u32::from_str_radix(groups[0],16).map_err(|_|"Invalid GUID")?;
    let b=u16::from_str_radix(groups[1],16).map_err(|_|"Invalid GUID")?;
    let c=u16::from_str_radix(groups[2],16).map_err(|_|"Invalid GUID")?;
    let mut tail=[0u8;8];
    let digits=format!("{}{}",groups[3],groups[4]);
    for (i,v) in tail.iter_mut().enumerate(){
        *v=u8::from_str_radix(&digits[i*2..i*2+2],16).map_err(|_|"Invalid GUID")?;
    }
    Ok(Guid{data1:a,data2:b,data3:c,data4:tail})
}
fn guid_str(g:&Guid)->String{
    format!("{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1,g.data2,g.data3,g.data4[0],g.data4[1],g.data4[2],g.data4[3],g.data4[4],
        g.data4[5],g.data4[6],g.data4[7])
}
const CLSID_IMMERSIVE:Guid=Guid{data1:0xC2F03A33,data2:0x21F5,data3:0x47FA,data4:[0xB4,0xBB,0x15,0x63,0x62,0xA2,0xF2,0x39]};
const IID_SERVICE_PROVIDER:Guid=Guid{data1:0x6D5140C1,data2:0x7436,data3:0x11CE,data4:[0x80,0x34,0x00,0xAA,0x00,0x60,0x09,0xFA]};
const SERVICE_VDM:Guid=Guid{data1:0xC5E0CDCA,data2:0x7B6E,data3:0x41B2,data4:[0x9F,0xC4,0xD9,0x39,0x75,0xCC,0x46,0x7B]};
const IID_VDM:Guid=Guid{data1:0x53F5CA0B,data2:0x158F,data3:0x4124,data4:[0x90,0x0C,0x05,0x71,0x58,0x06,0x0B,0x27]};
type Raw=*mut c_void;
#[link(name="ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved:Raw,flags:u32)->i32;
    fn CoUninitialize();
    fn CoCreateInstance(cls:*const Guid,outer:Raw,ctx:u32,iid:*const Guid,result:*mut Raw)->i32;
}
#[link(name="combase")]
unsafe extern "system" {
    fn WindowsCreateString(chars:*const u16,len:u32,out:*mut Raw)->i32;
    fn WindowsDeleteString(raw:Raw)->i32;
}
struct Apartment;
impl Apartment {
    fn new()->Result<Self,String>{
        let hr=unsafe{CoInitializeEx(null_mut(),0)}; // STA (helper is a new process)
        if hr<0{Err(format!("CoInitializeEx failed 0x{:08X}",hr as u32))}else{Ok(Self)}
    }
}
impl Drop for Apartment{fn drop(&mut self){unsafe{CoUninitialize();}}}
struct Com(Raw);
impl Drop for Com{
    fn drop(&mut self){if !self.0.is_null(){unsafe{
        let release:unsafe extern "system" fn(Raw)->u32=std::mem::transmute(vtable(self.0,2));
        release(self.0);
    }}}
}
unsafe fn vtable(this:Raw,slot:usize)->*const c_void{
    let table=*(this as *const *const *const c_void);
    *table.add(slot)
}
fn error(operation:&str,hr:i32)->String{
    format!("{operation}: HRESULT 0x{:08X}",hr as u32)
}
fn supported_build()->Result<u32,String>{
    const K:&str=r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
    unsafe{
        let mut key=null_mut();
        let rc=RegOpenKeyExW(HKEY_LOCAL_MACHINE,wide(K).as_ptr(),0,KEY_READ,&mut key);
        if rc!=0{return Err(format!("Cannot read Windows build: registry error {rc}"));}
        let mut bytes=[0u8;32];let mut len=bytes.len() as u32;let mut ty=0u32;
        let rc=RegQueryValueExW(key,wide("CurrentBuildNumber").as_ptr(),null_mut(),
            &mut ty,bytes.as_mut_ptr(),&mut len);
        RegCloseKey(key);
        if rc!=0||ty!=REG_SZ{return Err(format!("CurrentBuildNumber unavailable: {rc}"));}
        let utf16=bytes[..(len as usize).min(bytes.len())].chunks_exact(2)
            .map(|b|u16::from_le_bytes([b[0],b[1]])).take_while(|c|*c!=0).collect::<Vec<_>>();
        let build=String::from_utf16(&utf16).map_err(|e|e.to_string())?
            .parse::<u32>().map_err(|e|e.to_string())?;
        if !(26100..=26399).contains(&build){
            return Err(format!("Private wallpaper interface disabled for unverified Windows build {build}"));
        }
        Ok(build)
    }
}
fn access() ->Result<(Apartment,Com),String>{
    let apt=Apartment::new()?;
    let mut provider:Raw = null_mut();
    unsafe{
        let hr=CoCreateInstance(&CLSID_IMMERSIVE,null_mut(),23,&IID_SERVICE_PROVIDER,&mut provider);
        if hr<0{return Err(error("CoCreateInstance(ImmersiveShell)",hr));}
        if provider.is_null(){return Err("ImmersiveShell returned null".into());}
        let provider=Com(provider);
        let query:unsafe extern "system" fn(Raw,*const Guid,*const Guid,*mut Raw)->i32=
            std::mem::transmute(vtable(provider.0,3));
        let mut manager:Raw=null_mut();
        let hr=query(provider.0,&SERVICE_VDM,&IID_VDM,&mut manager);
        if hr<0{return Err(error("QueryService(IVirtualDesktopManagerInternal)",hr));}
        if manager.is_null(){return Err("VirtualDesktopManagerInternal returned null".into());}
        Ok((apt,Com(manager)))
    }
}
fn find(manager:&Com,target:&Guid)->Result<Com,String>{
    unsafe{
        // Windows 11 build 26100..26399: vtable 13 == FindDesktop(GUID*,IVirtualDesktop**).
        let find:unsafe extern "system" fn(Raw,*const Guid,*mut Raw)->i32=
            std::mem::transmute(vtable(manager.0,13));
        let mut desktop:Raw=null_mut();
        let hr=find(manager.0,target,&mut desktop);
        if hr<0{return Err(error("FindDesktop",hr));}
        if desktop.is_null(){return Err("FindDesktop returned null".into());}
        let desktop=Com(desktop);
        // Verify target identity before any mutation. IVirtualDesktop::GetId is slot 4.
        let get_id:unsafe extern "system" fn(Raw,*mut Guid)->i32=
            std::mem::transmute(vtable(desktop.0,4));
        let mut actual=Guid{data1:0,data2:0,data3:0,data4:[0;8]};
        let hr=get_id(desktop.0,&mut actual);
        if hr<0{return Err(error("IVirtualDesktop::GetId",hr));}
        if actual!=*target{return Err(format!("GUID mismatch: expected {}, got {}",guid_str(target),guid_str(&actual)));}
        Ok(desktop)
    }
}
fn check_interface(desktop_id:&str)->Result<u32,String>{
    let build=supported_build()?;
    let wanted=guid(desktop_id)?;
    let (_apt,manager)=access()?;
    let _desktop=find(&manager,&wanted)?;
    Ok(build)
}
/// Child-process entry: ONLY this path calls the private COM vtable.
pub fn apply_child(desktop_id:&str,image:&Path)->Result<(),String>{
    let _build=supported_build()?;
    let target=guid(desktop_id)?;
    if !image.is_file(){return Err("Wallpaper image missing".into());}
    if image.extension().and_then(|s|s.to_str()).is_none_or(|s|!s.eq_ignore_ascii_case("bmp")){
        return Err("Expected BMP wallpaper".into());
    }
    let absolute=image.canonicalize().map_err(|e|e.to_string())?;
    let wide:Vec<u16>=absolute.as_os_str().to_string_lossy().encode_utf16().collect();
    let (_apt,manager)=access()?;
    let desktop=find(&manager,&target)?;
    unsafe{
        let mut text:Raw=null_mut();
        let hr=WindowsCreateString(wide.as_ptr(),wide.len() as u32,&mut text);
        if hr<0{return Err(error("WindowsCreateString",hr));}
        struct Text(Raw);
        impl Drop for Text {fn drop(&mut self){unsafe{let _=WindowsDeleteString(self.0);}}}
        let text=Text(text);
        // Windows 11 build 26100..26399: manager vtable 16 == SetDesktopWallpaper.
        let set:unsafe extern "system" fn(Raw,Raw,Raw)->i32=
            std::mem::transmute(vtable(manager.0,16));
        let hr=set(manager.0,desktop.0,text.0);
        if hr<0{return Err(error("SetDesktopWallpaper",hr));}
    }
    Ok(())
}
pub fn probe_child()->serde_json::Value{
    let detected=crate::virtual_desktop::snapshot();
    match detected{
        Ok(s)=>{
            let target=s.current.clone().or_else(||s.ids.first().cloned());
            let result=target.as_deref().ok_or("No virtual desktop found".to_string())
                .and_then(check_interface);
            serde_json::json!({
                "private_interface":"SetDesktopWallpaper (undocumented)",
                "available":result.is_ok(),
                "windows_build":result.as_ref().ok(),
                "error":result.as_ref().err(),
                "current_desktop":s.current,
                "desktop_count":s.ids.len(),
                "writes_wallpaper":false,
            })
        },
        Err(e)=>serde_json::json!({"available":false,"error":e,"writes_wallpaper":false})
    }
}
fn run_helper(action:&str,id:&str,path:Option<&Path>)->Result<String,String>{
    let mut proc=Command::new(std::env::current_exe().map_err(|e|e.to_string())?);
    proc.arg(action).arg(id);
    if let Some(p)=path{proc.arg(p);}
    // Child process is isolated from the GUI. COM ABI faults return an error rather than crash it.
    let child=proc.output().map_err(|e|e.to_string())?;
    let stderr=String::from_utf8_lossy(&child.stderr).trim().to_string();
    if !child.status.success(){
        return Err(if stderr.is_empty(){format!("Virtual desktop COM helper exit {:?}",child.status.code())}else{stderr});
    }
    Ok(String::from_utf8_lossy(&child.stdout).trim().into())
}
pub fn assign(id:&str,image:&Path)->Result<(),String>{
    guid(id)?;
    let absolute=image.canonicalize().map_err(|e|e.to_string())?;
    run_helper("--vd-native-set",id,Some(&absolute)).map(|_|())
}
pub fn read_only_probe()->Result<String,String>{
    run_helper("--vd-native-probe","",None)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn guid_roundtrip(){
        for value in ["a57665ec-8e2e-4fb0-b32f-ad19837a473d","ca5472db-01ac-43e1-9528-3d31cf12a504"]{
            let g=guid(value).unwrap();
            assert_eq!(guid_str(&g),value);
        }
        assert!(guid("../../../etc/passwd").is_err());
    }
    #[test]fn version_gate_description(){
        // Never apply to unknown Win10/Win11 versions.
        assert!(guid("00000000-0000-0000-0000-000000000000").is_ok());
    }
}
