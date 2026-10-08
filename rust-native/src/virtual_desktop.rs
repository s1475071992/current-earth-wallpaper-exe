//! Experimental **read-only** Windows 11 Explorer virtual desktop detection.
 //! Unlike the supported IDesktopWallpaper API, the registry layout is not a public
 //! API contract. Never edit desktop registry state or guess an unknown active GUID.
use windows_sys::Win32::System::{
    Registry::*,
    Threading::{GetCurrentProcessId,ProcessIdToSessionId},
};
use std::ptr::{null,null_mut};

const KEY:&str=r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";

#[derive(Clone,Debug)]
pub struct Snapshot {
    pub ids:Vec<String>,
    pub current:Option<String>,
}
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}

fn read_binary(key_path:&str,name:&str)->Result<Option<Vec<u8>>,String>{
    unsafe{
        let mut handle=null_mut();
        let rc=RegOpenKeyExW(HKEY_CURRENT_USER,wide(key_path).as_ptr(),0,KEY_READ,&mut handle);
        if rc!=0{return if rc==2 {Ok(None)}else{Err(format!("RegOpenKeyExW {name}: {rc}"))};}
        let mut kind=0u32;let mut needed=0u32;
        let value=wide(name);
        let rc=RegQueryValueExW(handle,value.as_ptr(),null_mut(),&mut kind,null_mut(),&mut needed);
        if rc!=0||kind!=REG_BINARY||needed==0||needed>65536 {
            RegCloseKey(handle);
            return if rc==2{Ok(None)}else{Err(format!("Registry value {name}: error={rc}, type={kind}, bytes={needed}"))};
        }
        let mut bytes=vec![0u8;needed as usize];
        let rc=RegQueryValueExW(handle,value.as_ptr(),null_mut(),&mut kind,bytes.as_mut_ptr(),&mut needed);
        RegCloseKey(handle);
        if rc!=0||kind!=REG_BINARY{return Err(format!("Registry read {name}: {rc}"))}
        bytes.truncate(needed as usize);
        Ok(Some(bytes))
    }
}

pub fn format_guid(data:&[u8])->Option<String>{
    if data.len()!=16{return None;}
    let a=u32::from_le_bytes(data[0..4].try_into().ok()?);
    let b=u16::from_le_bytes(data[4..6].try_into().ok()?);
    let c=u16::from_le_bytes(data[6..8].try_into().ok()?);
    Some(format!("{a:08x}-{b:04x}-{c:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        data[8],data[9],data[10],data[11],data[12],data[13],data[14],data[15]))
}
pub fn parse_desktops(bytes:&[u8])->Result<Vec<String>,String>{
    if bytes.len()%16!=0{return Err("Invalid VirtualDesktopIDs registry length".into())}
    let mut out=Vec::new();
    for chunk in bytes.chunks_exact(16){
        let id=format_guid(chunk).ok_or("Invalid desktop GUID")?;
        if !out.contains(&id){out.push(id);}
    }
    Ok(out)
}
pub fn snapshot()->Result<Snapshot,String>{
    let bytes=read_binary(KEY,"VirtualDesktopIDs")?
        .ok_or("Explorer VirtualDesktopIDs unavailable; create/switch virtual desktops first")?;
    let ids=parse_desktops(&bytes)?;
    if ids.is_empty(){return Err("No virtual desktop GUIDs found".into());}
    let mut active=read_binary(KEY,"CurrentVirtualDesktop")?;
    if active.is_none(){
        let mut session=0u32;
        unsafe{
            if ProcessIdToSessionId(GetCurrentProcessId(),&mut session)!=0{
                active=read_binary(&format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\SessionInfo\{session}\VirtualDesktops"),
                    "CurrentVirtualDesktop")?;
            }
        }
    }
    let current=active.and_then(|b|format_guid(&b))
        .or_else(||if ids.len()==1{Some(ids[0].clone())}else{None})
        .filter(|id|ids.contains(id));
    Ok(Snapshot{ids,current})
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn guid_endianness(){
        let raw=[0x78,0x56,0x34,0x12,0xbc,0x9a,0xf0,0xde,0x12,0x34,0x56,0x78,0x90,0xab,0xcd,0xef];
        assert_eq!(format_guid(&raw).unwrap(),"12345678-9abc-def0-1234-567890abcdef");
        assert!(format_guid(&raw[..15]).is_none());
    }
    #[test]fn desktop_ids_validate_length(){
        assert!(parse_desktops(&[0u8;17]).is_err());
        assert_eq!(parse_desktops(&[0u8;32]).unwrap().len(),1);
    }
}
