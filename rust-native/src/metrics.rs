//! Real OS working-set measurement, including peak since process creation.
use std::{fs, path::PathBuf};
use serde_json::json;
use windows_sys::Win32::System::{
  ProcessStatus::{GetProcessMemoryInfo,PROCESS_MEMORY_COUNTERS},
  Threading::GetCurrentProcess,
};
pub fn write_render_report()->Result<(),String>{
    unsafe{
        let mut pmc:PROCESS_MEMORY_COUNTERS=std::mem::zeroed();
        pmc.cb=std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(),&mut pmc,pmc.cb)==0 {
            return Err(format!("GetProcessMemoryInfo: {}",std::io::Error::last_os_error()));
        }
        let mib=1048576.0;
        let info=json!({
            "scenario":"Windows native WIC decode of 4096x4096 BMP to 1200x1200; composition of 3840x2160 desktop image",
            "peak_working_set_mib":(pmc.PeakWorkingSetSize as f64/mib*100.0).round()/100.0,
            "working_set_after_render_mib":(pmc.WorkingSetSize as f64/mib*100.0).round()/100.0,
            "peak_private_commit_mib":(pmc.PeakPagefileUsage as f64/mib*100.0).round()/100.0,
            "private_commit_after_render_mib":(pmc.PagefileUsage as f64/mib*100.0).round()/100.0,
        });
        let dir=std::env::current_exe().map_err(|e|e.to_string())?
            .parent().ok_or("No executable folder")?.to_path_buf();
        fs::write(dir.join("memory-render.json"),serde_json::to_vec_pretty(&info).map_err(|e|e.to_string())?)
            .map_err(|e|e.to_string())?;
        Ok(())
    }
}
