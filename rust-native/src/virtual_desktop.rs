//! Read-only Windows 11 virtual-desktop registry adapter.
//! Explorer's registry values are not a stable documented API. Detect unknown
//! states and do not silently map them to Desktop 1 (which causes wrong wallpapers).
use windows_sys::Win32::System::{
    Registry::*,
    Threading::GetCurrentProcessId,
    RemoteDesktop::ProcessIdToSessionId,
};
use std::ptr::null_mut;

const ROOT:&str=r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";
const SESSION_ROOT:&str=r"Software\Microsoft\Windows\CurrentVersion\Explorer\SessionInfo";

#[derive(Clone,Debug)]
pub struct Snapshot {
    pub ids:Vec<String>,
    pub current:Option<String>,
    pub source:&'static str,
    pub diagnostic:Vec<String>,
}
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
fn read_binary(key_path:&str,name:&str)->Result<Option<Vec<u8>>,String>{
    unsafe{
        let mut handle=std::ptr::null_mut();
        let rc=RegOpenKeyExW(HKEY_CURRENT_USER,wide(key_path).as_ptr(),0,KEY_READ,&mut handle);
        if rc!=0{return if rc==2{Ok(None)}else{Err(format!("{key_path}: RegOpenKeyExW={rc}"))};}
        let mut kind=0u32;let mut size=0u32;
        let value=wide(name);
        let rc=RegQueryValueExW(handle,value.as_ptr(),null_mut(),&mut kind,null_mut(),&mut size);
        if rc!=0||kind!=REG_BINARY||size==0||size>65536 {
            RegCloseKey(handle);
            return if rc==2{Ok(None)}
                else{Err(format!("{key_path} / {name}: query error={rc}, type={kind}, bytes={size}"))};
        }
        let mut buffer=vec![0u8;size as usize];
        let rc=RegQueryValueExW(handle,value.as_ptr(),null_mut(),&mut kind,buffer.as_mut_ptr(),&mut size);
        RegCloseKey(handle);
        if rc!=0||kind!=REG_BINARY{return Err(format!("{key_path} / {name}: read error={rc}"))}
        buffer.truncate(size as usize);
        Ok(Some(buffer))
    }
}
pub fn format_guid(data:&[u8])->Option<String>{
    if data.len()!=16{return None}
    let a=u32::from_le_bytes(data[0..4].try_into().ok()?);
    let b=u16::from_le_bytes(data[4..6].try_into().ok()?);
    let c=u16::from_le_bytes(data[6..8].try_into().ok()?);
    Some(format!("{a:08x}-{b:04x}-{c:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        data[8],data[9],data[10],data[11],data[12],data[13],data[14],data[15]))
}
pub fn parse_desktops(bytes:&[u8])->Result<Vec<String>,String>{
    if bytes.is_empty()||bytes.len()%16!=0||bytes.len()>16*128 {
        return Err(format!("Invalid VirtualDesktopIDs byte count: {}",bytes.len()))
    }
    let mut result=Vec::new();
    for entry in bytes.chunks_exact(16) {
        let id=format_guid(entry).ok_or("Invalid GUID bytes")?;
        if !result.contains(&id){result.push(id);}
    }
    Ok(result)
}
fn session_path()->Result<String,String>{
    let mut session=0u32;
    unsafe{
        if ProcessIdToSessionId(GetCurrentProcessId(),&mut session)==0{
            return Err(format!("ProcessIdToSessionId failed: {}",std::io::Error::last_os_error()));
        }
    }
    Ok(format!(r"{SESSION_ROOT}\{session}\VirtualDesktops"))
}
fn capture(path:&str,name:&str,diagnostics:&mut Vec<String>)->Option<Vec<u8>> {
    match read_binary(path,name) {
        Ok(Some(bytes))=>{
            diagnostics.push(format!("{name} [{}]: {} bytes",if path==ROOT{"root"}else{"session"},bytes.len()));
            Some(bytes)
        },
        Ok(None)=>{
            diagnostics.push(format!("{name} [{}]: not found",if path==ROOT{"root"}else{"session"}));
            None
        },
        Err(e)=>{
            diagnostics.push(format!("{name}: {e}"));
            None
        }
    }
}
pub fn snapshot()->Result<Snapshot,String>{
    let mut diagnostic=Vec::new();
    let session=match session_path(){
        Ok(s)=>Some(s),
        Err(e)=>{diagnostic.push(e);None}
    };
    let root_ids=capture(ROOT,"VirtualDesktopIDs",&mut diagnostic);
    let session_ids=session.as_ref().and_then(|p|capture(p,"VirtualDesktopIDs",&mut diagnostic));
    let ids=root_ids.as_deref()
        .and_then(|v|parse_desktops(v).ok())
        .or_else(||session_ids.as_deref().and_then(|v|parse_desktops(v).ok()))
        .ok_or_else(||format!("Cannot read desktop ID list: {}",diagnostic.join("; ")))?;

    let root_active=capture(ROOT,"CurrentVirtualDesktop",&mut diagnostic)
        .and_then(|v|format_guid(&v));
    let session_active=session.as_ref()
        .and_then(|p|capture(p,"CurrentVirtualDesktop",&mut diagnostic))
        .and_then(|v|format_guid(&v));
    let (current,source)=choose_current(&ids,root_active.as_deref(),session_active.as_deref());
    if current.is_none() {
        diagnostic.push("Current GUID unknown or inconsistent; no wallpaper assignment permitted".into());
    }
    diagnostic.push(format!("active source={source}, known desktops={}",ids.len()));
    Ok(Snapshot{ids,current,source,diagnostic})
}
fn choose_current(ids:&[String],root:Option<&str>,session:Option<&str>)->(Option<String>,&'static str){
    // PowerToys prefers the Windows 11 root location. A stale/invalid root
    // must not suppress a valid per-session value.
    if let Some(id)=root.filter(|id|ids.iter().any(|s|s==id)){
        return (Some(id.to_string()),"root")
    }
    if let Some(id)=session.filter(|id|ids.iter().any(|s|s==id)){
        return (Some(id.to_string()),"session")
    }
    if ids.len()==1{return (Some(ids[0].clone()),"single-desktop")}
    (None,"unknown")
}
pub fn diagnostic_report()->serde_json::Value{
    match snapshot(){
        Ok(s)=>serde_json::json!({
            "registry_read_only":true,
            "available":true,
            "desktop_count":s.ids.len(),
            "desktop_ids":s.ids,
            "current":s.current,
            "active_source":s.source,
            "details":s.diagnostic,
            "warning":"Explorer registry state is unofficial and can lag during desktop transitions",
        }),
        Err(e)=>serde_json::json!({
            "registry_read_only":true,
            "available":false,
            "error":e,
        })
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn guid_endianness(){
        let raw=[0x78,0x56,0x34,0x12,0xbc,0x9a,0xf0,0xde,0x12,0x34,0x56,0x78,0x90,0xab,0xcd,0xef];
        assert_eq!(format_guid(&raw).unwrap(),"12345678-9abc-def0-1234-567890abcdef");
        assert!(format_guid(&raw[..15]).is_none());
    }
    #[test] fn desktop_ids_validate_length(){
        assert!(parse_desktops(&[0u8;17]).is_err());
        assert_eq!(parse_desktops(&[0u8;32]).unwrap().len(),1);
        assert!(parse_desktops(&[]).is_err());
    }
    #[test] fn desktop_choice_never_falls_back_to_first_of_many(){
        let list=vec!["first".into(),"second".into()];
        assert_eq!(choose_current(&list,None,None).0,None);
        assert_eq!(choose_current(&list,Some("invalid"),Some("second")).0,Some("second".into()));
        assert_eq!(choose_current(&list,Some("first"),Some("second")).0,Some("first".into()));
    }
}
