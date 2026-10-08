//! One GUI instance per Windows logon session. Starting a new GUI instance
//! requests the previous GUI to EXIT (including a hidden/tray-less window)
//! and waits for it to close before opening the replacement.
//!
//! IMPORTANT: helper CLI modes (--vd-native-set, etc.) bypass this module,
//! so they can work while the parent GUI is running.
use std::{ptr::{null,null_mut},time::{Duration,Instant},thread};
use windows_sys::Win32::{
    Foundation::{CloseHandle,HANDLE,WAIT_OBJECT_0,WAIT_ABANDONED,WAIT_TIMEOUT},
    System::Threading::{CreateMutexW,ReleaseMutex,WaitForSingleObject,OpenProcess,GetCurrentProcessId,
        PROCESS_QUERY_LIMITED_INFORMATION},
    UI::WindowsAndMessaging::{FindWindowW,GetWindowThreadProcessId,PostMessageW,WM_COMMAND},
};
const MUTEX_NAME:&str=r"Local\CurrentEarthWallpaperNative.GuiInstance.v1";
const GUI_CLASS:&str="CurrentEarthWallpaperNativeWindow";
const EXIT_COMMAND:usize=108; // existing GUI ID_EXIT: bypasses WM_CLOSE hide/exit dialog.
const PROCESS_SYNCHRONIZE:u32=0x00100000; // SYNCHRONIZE standard access right
const TAKEOVER_TIMEOUT:Duration=Duration::from_secs(12);
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}

/// Main thread must own this mutex until the GUI exits.
pub struct Guard{handle:HANDLE}
impl Drop for Guard{
    fn drop(&mut self){unsafe{
        ReleaseMutex(self.handle);
        CloseHandle(self.handle);
    }}
}

fn close_old_window(deadline:Instant)->Result<bool,String>{
    let hwnd=unsafe{FindWindowW(wide(GUI_CLASS).as_ptr(),null())};
    if hwnd.is_null(){return Ok(false);}
    let mut pid=0u32;
    unsafe{GetWindowThreadProcessId(hwnd,&mut pid);}
    if pid==0||pid==unsafe{GetCurrentProcessId()}{return Ok(false);}
    let handle=unsafe{OpenProcess(PROCESS_SYNCHRONIZE|PROCESS_QUERY_LIMITED_INFORMATION,0,pid)};
    if handle.is_null(){
        return Err(format!("Found a prior wallpaper GUI (PID {pid}), but cannot wait for it to exit: {}",
            std::io::Error::last_os_error()));
    }
    struct Process(HANDLE);
    impl Drop for Process{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
    let old=Process(handle);
    // The message's recipient is the app's unique native HWND class.
    // This works even when the window is hidden with tray-icon display disabled,
    // and existing v1.0/v1.1 builds also understand ID_EXIT 108.
    if unsafe{PostMessageW(hwnd,WM_COMMAND,EXIT_COMMAND,0)}==0 {
        return Err(format!("Could not request exit of prior wallpaper GUI (PID {pid}): {}",
            std::io::Error::last_os_error()));
    }
    let remaining=deadline.saturating_duration_since(Instant::now());
    let millis=remaining.as_millis().min(u32::MAX as u128) as u32;
    if unsafe{WaitForSingleObject(old.0,millis)}!=WAIT_OBJECT_0{
        return Err(format!("Prior wallpaper GUI (PID {pid}) did not exit within 12s; new instance cancelled to prevent duplicate background updates"));
    }
    Ok(true)
}

/// Try to acquire the GUI-only named mutex. If another GUI owns it, ask that
/// window to shut down, then WAIT for it to relinquish the mutex. No forcible
/// TerminateProcess and no process-name-wide kill are used.
pub fn replace_previous()->Result<(Guard,bool),String>{
    let handle=unsafe{CreateMutexW(null(),0,wide(MUTEX_NAME).as_ptr())};
    if handle.is_null(){return Err(format!("CreateMutexW failed: {}",std::io::Error::last_os_error()));}
    struct CloseOnFail(HANDLE,bool);
    impl Drop for CloseOnFail{fn drop(&mut self){if !self.1{unsafe{CloseHandle(self.0);}}}}
    let mut guard=CloseOnFail(handle,false);
    let deadline=Instant::now()+TAKEOVER_TIMEOUT;
    let mut replaced=false;
    loop {
        match unsafe{WaitForSingleObject(handle,0)} {
            WAIT_OBJECT_0|WAIT_ABANDONED=>{
                // Upgrade from older builds which didn't have a named mutex:
                // find and stop their hidden window too, before the new UI starts.
                match close_old_window(deadline){
                    Ok(exited)=>{replaced|=exited;guard.1=true;return Ok((Guard{handle},replaced));}
                    Err(e)=>{unsafe{ReleaseMutex(handle);}return Err(e);}
                }
            },
            WAIT_TIMEOUT=>{
                // Existing new-version GUI currently owns the mutex.
                // It may still be starting up. Retry until its window exists,
                // request normal app shutdown, and wait to own the same mutex.
                let hwnd=unsafe{FindWindowW(wide(GUI_CLASS).as_ptr(),null_mut())};
                if !hwnd.is_null(){
                    match close_old_window(deadline){
                        Ok(exited)=>replaced|=exited,
                        Err(e)=>return Err(e),
                    }
                }
                if Instant::now()>=deadline {
                    return Err("Another wallpaper GUI holds the instance lock but could not be stopped; refusing a second instance".into());
                }
                thread::sleep(Duration::from_millis(80));
            },
            other=>return Err(format!("WaitForSingleObject(instance mutex) failed: code {other}")),
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn gui_ipc_is_not_windows_close_dialog(){
        assert_eq!(EXIT_COMMAND,108);
        assert_ne!(MUTEX_NAME,GUI_CLASS);
    }
}
