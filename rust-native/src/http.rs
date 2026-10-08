//! Bounded-size synchronous WinHTTP downloader. Windows OS certificate store provides TLS roots.
use std::{ffi::c_void, fs::File, io::Write, path::Path};
use windows_sys::Win32::Networking::WinHttp::*;

struct Handle(*mut c_void);
impl Drop for Handle { fn drop(&mut self) { if !self.0.is_null() { unsafe { WinHttpCloseHandle(self.0); } } } }
fn wide(s:&str)->Vec<u16>{ s.encode_utf16().chain(Some(0)).collect() }
fn err(what:&str)->String { format!("{what}: {}",std::io::Error::last_os_error()) }

pub fn split_url(url:&str)->Result<(String,String,u16,String,bool),String>{
    let (secure, rest) = if let Some(s)=url.strip_prefix("https://"){(true,s)}
        else if let Some(s)=url.strip_prefix("http://"){(false,s)}
        else {return Err("HTTP(S) URL required".into())};
    let pos=rest.find(&['/','?','#'][..]).unwrap_or(rest.len());
    let authority=&rest[..pos];
    if authority.is_empty()||authority.contains('@')||authority.contains('[')||authority.contains(']') {return Err("Invalid host".into())}
    let (host,port)=match authority.rsplit_once(':') {
        Some((h,p)) => (h,p.parse::<u16>().map_err(|_|"Invalid port")?),
        None => (authority,if secure{443}else{80})
    };
    if host.is_empty()||!host.bytes().all(|c|c.is_ascii_alphanumeric()||b".-".contains(&c)) {
        return Err("Unsafe hostname".into());
    }
    let tail=rest[pos..].split('#').next().unwrap_or("");
    let path=if tail.is_empty(){"/".into()}else if tail.starts_with('?'){format!("/{tail}")}else{tail.to_string()};
    Ok((host.to_string(),path.clone(),port,path,secure))
}

/// Downloads to caller-owned path; size limit is enforced while streaming.
pub fn download(url:&str,dest:&Path,max_bytes:u64)->Result<(),String>{
    let (host,_,port,path,secure)=split_url(url)?;
    let agent=wide("CurrentEarthWallpaperNative/0.2 (+Windows WinHTTP)");
    unsafe {
        let session=Handle(WinHttpOpen(agent.as_ptr(),WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            std::ptr::null(),std::ptr::null(),0));
        if session.0.is_null(){return Err(err("WinHttpOpen"));}
        WinHttpSetTimeouts(session.0,20000,20000,45000,45000);
        let hostw=wide(&host);
        let connect=Handle(WinHttpConnect(session.0,hostw.as_ptr(),port,0));
        if connect.0.is_null(){return Err(err("WinHttpConnect"));}
        let pathw=wide(&path);
        let request=Handle(WinHttpOpenRequest(connect.0,std::ptr::null(),pathw.as_ptr(),
            std::ptr::null(),std::ptr::null(),std::ptr::null(),
            if secure{WINHTTP_FLAG_SECURE}else{0}));
        if request.0.is_null(){return Err(err("WinHttpOpenRequest"));}
        if WinHttpSendRequest(request.0,std::ptr::null(),0,std::ptr::null_mut(),0,0,0)==0 {
            return Err(err("WinHttpSendRequest"));
        }
        if WinHttpReceiveResponse(request.0,std::ptr::null_mut())==0 {
            return Err(err("WinHttpReceiveResponse"));
        }
        let mut status:u32=0;
        let mut size=std::mem::size_of::<u32>() as u32;
        if WinHttpQueryHeaders(request.0,WINHTTP_QUERY_STATUS_CODE|WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),&mut status as *mut _ as *mut c_void,&mut size,std::ptr::null_mut())==0 {
            return Err(err("WinHttpQueryHeaders"));
        }
        if !(200..300).contains(&status){return Err(format!("HTTP {status} from {host}"));}
        let tmp=dest.with_extension("download");
        let result=(||->Result<(),String>{
            let mut file=File::create(&tmp).map_err(|e|e.to_string())?;
            let mut total=0u64;
            let mut buf=[0u8;16384];
            loop {
                let mut n:u32=0;
                if WinHttpReadData(request.0,buf.as_mut_ptr() as *mut c_void,
                   buf.len() as u32,&mut n)==0{return Err(err("WinHttpReadData"));}
                if n==0{break;}
                total+=n as u64;
                if total>max_bytes{return Err(format!("Download exceeds {} MB",max_bytes/1048576));}
                file.write_all(&buf[..n as usize]).map_err(|e|e.to_string())?;
            }
            file.sync_all().map_err(|e|e.to_string())?;
            drop(file);
            std::fs::rename(&tmp,dest).map_err(|e|e.to_string())?;
            Ok(())
        })();
        if result.is_err(){let _=std::fs::remove_file(&tmp);}
        result
    }
}
pub fn get_text(url:&str,max_bytes:u64)->Result<String,String>{
    let tmp=std::env::temp_dir().join(format!("cew_meta_{}_{}.tmp",std::process::id(),
        std::thread::current().name().unwrap_or("worker")));
    let result=(||{
        download(url,&tmp,max_bytes)?;
        let text=std::fs::read_to_string(&tmp).map_err(|e|e.to_string())?;
        Ok(text)
    })();
    let _=std::fs::remove_file(tmp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_public_urls(){
        let p=split_url("https://epic.gsfc.nasa.gov/api/natural?x=1").unwrap();
        assert_eq!(p.0,"epic.gsfc.nasa.gov");
        assert_eq!(p.3,"/api/natural?x=1");
        assert!(p.4);
        assert!(split_url("file:///etc/passwd").is_err());
        assert!(split_url("https://evil.com@epic.gsfc.nasa.gov/x").is_err());
    }
}
