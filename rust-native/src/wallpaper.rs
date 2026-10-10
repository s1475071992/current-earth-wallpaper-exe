//! Native wallpaper composition with scanline BMP output: O(screen width) canvas memory.
use std::{fs::{self,File},io::{Write,BufWriter},path::{Path,PathBuf},ffi::OsStr,os::windows::ffi::OsStrExt};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics,SM_CXSCREEN,SM_CYSCREEN,SystemParametersInfoW,SPI_SETDESKWALLPAPER,
    SPIF_UPDATEINIFILE,SPIF_SENDCHANGE,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTime;
use windows_sys::Win32::Foundation::SYSTEMTIME;
use crate::imaging::Pixels;

pub fn screen_size()->Result<(u32,u32),String> {
    let w=unsafe{GetSystemMetrics(SM_CXSCREEN)};
    let h=unsafe{GetSystemMetrics(SM_CYSCREEN)};
    if w<160||h<120||w>16000||h>16000{Err("Invalid screen dimensions".into())}
    else{Ok((w as u32,h as u32))}
}
pub fn diameter(height:u32,scale:&str)->u32 {
    let factor=match scale{
        "铺满屏幕"=>1.0, "原始大小"=>1.0, "更小尺寸"=>0.45, _=>0.618,
    };
    ((height as f64*factor).round() as u32).max(1)
}
fn header(writer:&mut impl Write,w:u32,h:u32)->Result<(),String>{
    let bytes=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or("Image too large")?;
    let mut hbuf=Vec::with_capacity(54);
    hbuf.extend_from_slice(b"BM");
    hbuf.extend_from_slice(&(bytes+54).to_le_bytes());
    hbuf.extend_from_slice(&[0u8;4]);
    hbuf.extend_from_slice(&54u32.to_le_bytes());
    hbuf.extend_from_slice(&40u32.to_le_bytes());
    hbuf.extend_from_slice(&(w as i32).to_le_bytes());
    hbuf.extend_from_slice(&(h as i32).to_le_bytes());
    hbuf.extend_from_slice(&1u16.to_le_bytes());
    hbuf.extend_from_slice(&32u16.to_le_bytes());
    hbuf.extend_from_slice(&0u32.to_le_bytes());
    hbuf.extend_from_slice(&bytes.to_le_bytes());
    hbuf.extend_from_slice(&[0u8;16]);
    writer.write_all(&hbuf).map_err(|e|e.to_string())
}
pub fn write_bitmap(dest:&Path,w:u32,h:u32,bgra:&[u8])->Result<(),String>{
    if bgra.len()!=(w as usize)*(h as usize)*4{return Err("Invalid bitmap buffer".into())}
    let mut file=BufWriter::new(File::create(dest).map_err(|e|e.to_string())?);
    header(&mut file,w,h)?;
    let row=w as usize*4;
    for y in (0..h as usize).rev(){
        file.write_all(&bgra[y*row..(y+1)*row]).map_err(|e|e.to_string())?;
    }
    file.flush().map_err(|e|e.to_string())
}

/// Generate a large, valid test BMP without ever keeping its full frame in memory.
pub fn write_test_pattern(dest:&Path,side:u32)->Result<(),String>{
    if !(256..=8192).contains(&side){return Err("Unsupported test size".into());}
    let mut out=BufWriter::new(File::create(dest).map_err(|e|e.to_string())?);
    header(&mut out,side,side)?;
    let mut row=vec![0u8;side as usize*4];
    for y in 0..side {
        for x in 0..side{
            let off=x as usize*4;
            row[off]=(x&255) as u8;
            row[off+1]=(y&255) as u8;
            row[off+2]=((x+y)&255) as u8;
            row[off+3]=255;
        }
        out.write_all(&row).map_err(|e|e.to_string())?;
    }
    out.flush().map_err(|e|e.to_string())
}

fn glyph(c:char)->[u8;7]{
    match c {
        '0'=>[14,17,19,21,25,17,14], '1'=>[4,12,4,4,4,4,14],
        '2'=>[14,17,1,2,4,8,31], '3'=>[30,1,1,14,1,1,30],
        '4'=>[2,6,10,18,31,2,2], '5'=>[31,16,16,30,1,1,30],
        '6'=>[14,16,16,30,17,17,14], '7'=>[31,1,2,4,8,8,8],
        '8'=>[14,17,17,14,17,17,14], '9'=>[14,17,17,15,1,1,14],
        ':'=>[0,4,4,0,4,4,0], '-'=>[0,0,0,31,0,0,0],
        'O'=>[14,17,17,17,17,17,14], 'b'=>[16,16,30,17,17,17,30],
        's'=>[0,0,15,16,14,1,30], 'U'=>[17,17,17,17,17,17,14],
        'p'=>[0,0,30,17,30,16,16], 'd'=>[1,1,15,17,17,17,15],
        'T'=>[31,4,4,4,4,4,4], 'C'=>[14,17,16,16,16,17,14],
        '+'=>[0,4,4,31,4,4,0],
        _=>[0;7],
    }
}
fn month_days(year:u32,month:u32)->u32 {
    match month {
        1|3|5|7|8|10|12=>31, 4|6|9|11=>30,
        2=>if year%4==0 && (year%100!=0 || year%400==0){29}else{28},
        _=>0,
    }
}
/// Provider observation timestamps are UTC (ISO-8601 or NASA/NICT's
/// "YYYY-MM-DD HH:MM:SS"). Display fixed UTC+8, never the PC's ambiguous
/// local timezone. Reject invalid calendar dates instead of inventing them.
pub fn observed_utc8(utc:&str)->Option<String>{
    let b=utc.as_bytes();
    if b.len()<19 || b[4]!=b'-' || b[7]!=b'-'
        || !matches!(b[10],b'T'|b' ') || b[13]!=b':' || b[16]!=b':'
        || !b.iter().take(19).enumerate().all(|(i,v)|
            [4,7,10,13,16].contains(&i)||v.is_ascii_digit()) {
        return None;
    }
    if b.len()>19 && !utc[19..].starts_with('Z') && !utc[19..].starts_with('.') {
        return None;
    }
    let part=|start,end| utc[start..end].parse::<u32>().ok();
    let (mut year,mut month,mut day)=(part(0,4)?,part(5,7)?,part(8,10)?);
    let (mut hour,minute,second)=(part(11,13)?,part(14,16)?,part(17,19)?);
    if year<1900 || month==0 || day==0 || day>month_days(year,month)
        || hour>23 || minute>59 || second>59{return None;}
    hour+=8;
    if hour>=24 {
        hour-=24;
        day+=1;
        if day>month_days(year,month){
            day=1;month+=1;
            if month>12{month=1;year+=1;}
        }
    }
    Some(format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}"))
}
fn update_time_utc8()->String{
    let mut t:SYSTEMTIME=unsafe{std::mem::zeroed()};
    unsafe{GetSystemTime(&mut t);}
    let utc=format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        t.wYear,t.wMonth,t.wDay,t.wHour,t.wMinute,t.wSecond);
    let date=observed_utc8(&utc).unwrap_or_else(||"---- -- -- --:--".into());
    format!("{date}:{:02} UTC+8",t.wSecond)
}
fn watermark_label(observed_utc:Option<&str>,updated:&str)->String {
    let observation=observed_utc.and_then(observed_utc8)
        .map(|t|format!("{t} UTC+8")).unwrap_or_else(||"--".into());
    format!("Obs {observation} Upd {updated}")
}
fn draw_watermark_row(row:&mut [u8],screen_y:u32,w:u32,h:u32,text:&str){
    // 5x7 bitmap font, 1-2x scale, no layout padding on the right or bottom.
    // This is ~75% white alpha over the original background (not opaque white).
    let mut px_scale=(h/800).clamp(1,2);
    let count=text.chars().count() as u32;
    while px_scale>1 && count.saturating_mul(6).saturating_mul(px_scale)>w {
        px_scale-=1;
    }
    let char_w=6*px_scale;
    let text_w=count.saturating_sub(1)*char_w+5*px_scale;
    let start_x=w.saturating_sub(text_w);
    let start_y=h.saturating_sub(7*px_scale);
    if screen_y<start_y||screen_y>=h{return}
    let glyph_y=((screen_y-start_y)/px_scale) as usize;
    for (i,c) in text.chars().enumerate(){
        let bits=glyph(c)[glyph_y];
        for bit in 0..5 {
            if bits&(1<<(4-bit))==0{continue}
            let x=start_x+i as u32*char_w+bit*px_scale;
            for dx in 0..px_scale{
                if x+dx>=w{continue}
                let p=(x+dx) as usize*4;
                for color in &mut row[p..p+3] {
                    *color=((*color as u16*64+255*191+127)/255) as u8;
                }
                row[p+3]=255;
            }
        }
    }
}
/// Keep the existing API for offline renderer tests and callers without
/// available observation metadata. Unknown observation is displayed as "--".
pub fn compose(dest:&Path,w:u32,h:u32,earth:&Pixels,watermark:bool)->Result<(),String>{
    compose_observed(dest,w,h,earth,watermark,None)
}
pub fn compose_observed(dest:&Path,w:u32,h:u32,earth:&Pixels,watermark:bool,
    observed_utc:Option<&str>)->Result<(),String>{
    let d=earth.width.min(earth.height);
    if d==0||d>w||d>h{return Err("Invalid Earth size".into())}
    let x0=(w-d)/2; let y0=(h-d)/2;
    let mut bmp=BufWriter::new(File::create(dest).map_err(|e|e.to_string())?);
    header(&mut bmp,w,h)?;
    let mut row=vec![0u8;w as usize*4];
    let rad=d as i64;
    let mark=if watermark {
        watermark_label(observed_utc,&update_time_utc8())
    }else{String::new()};
    for y in (0..h).rev(){
        row.fill(0);
        if y>=y0&&y<y0+d{
            let yy=(y-y0) as i64;
            for x in 0..d{
                let xx=x as i64;
                if (2*xx+1-rad).pow(2)+(2*yy+1-rad).pow(2)<=rad*rad{
                    let to=((x+x0)*4) as usize;
                    let from=((y-y0)*d*4+x*4) as usize;
                    row[to..to+4].copy_from_slice(&earth.bgra[from..from+4]);
                    row[to+3]=255;
                }
            }
        }
        if watermark{draw_watermark_row(&mut row,y,w,h,&mark);}
        bmp.write_all(&row).map_err(|e|e.to_string())?;
    }
    bmp.flush().map_err(|e|e.to_string())?;
    Ok(())
}
pub fn set_wallpaper(path:&Path)->Result<(),String>{
    let absolute=path.canonicalize().map_err(|e|e.to_string())?;
    let wide:Vec<u16>=OsStr::new(&absolute).encode_wide().chain(Some(0)).collect();
    let ok=unsafe{SystemParametersInfoW(SPI_SETDESKWALLPAPER,0,wide.as_ptr() as *mut _,
        SPIF_SENDCHANGE|SPIF_UPDATEINIFILE)};
    if ok==0{Err(format!("SystemParametersInfoW: {}",std::io::Error::last_os_error()))}
    else{Ok(())}
}
pub fn prune_own_wallpapers(folder:&Path,active:&Path,max_count:usize){
    // Screens using different wallpapers must retain their own file paths.
    // Preserve the two newest generated BMPs per display identity, plus active.
    let mut entries:Vec<(PathBuf,String,std::time::SystemTime)>=fs::read_dir(folder).into_iter().flatten().flatten()
        .filter_map(|entry|{
            let p=entry.path();
            let name=p.file_name()?.to_str()?;
            if !name.starts_with("cew-")||!name.ends_with(".bmp"){return None}
            let chunks=name.trim_end_matches(".bmp").split('-').collect::<Vec<_>>();
            // cew-PID-timestamp or cew-PID-monitorhash-timestamp
            if chunks.len()!=3 && chunks.len()!=4{return None}
            if !chunks[1].bytes().all(|v|v.is_ascii_digit()){return None}
            let identity=if chunks.len()==4 {chunks[2].to_string()}else{"global".to_string()};
            let m=entry.metadata().ok()?;
            if !m.is_file(){return None}
            Some((p,identity,m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)))
        }).collect();
    entries.sort_by(|a,b|b.2.cmp(&a.2));
    let mut counts=std::collections::BTreeMap::<String,usize>::new();
    let mut kept=0usize;
    for (path,identity,modified) in entries{
        let c=counts.entry(identity).or_default();
        *c+=1;
        let recent=std::time::SystemTime::now().duration_since(modified)
            .unwrap_or_default()<std::time::Duration::from_secs(14*86400);
        let protect=path==active || *c<=2;
        if protect || (kept<max_count.max(20) && recent){kept+=1;continue}
        let _=fs::remove_file(path);
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn size_modes(){assert_eq!(diameter(1000,"铺满屏幕"),1000);
        assert_eq!(diameter(1000,"黄金比例"),618);}

    #[test]fn timestamp_metadata_utc8_and_rollover(){
        assert_eq!(observed_utc8("2026-10-10T10:50:00.000Z").unwrap(),"2026-10-10 18:50");
        assert_eq!(observed_utc8("2026-10-07 21:49:33").unwrap(),"2026-10-08 05:49");
        assert_eq!(observed_utc8("2026-12-31T18:01:00Z").unwrap(),"2027-01-01 02:01");
        assert_eq!(observed_utc8("2024-02-29T23:40:00Z").unwrap(),"2024-03-01 07:40");
        assert!(observed_utc8("2026-02-30T18:20:00Z").is_none());
        assert_eq!(watermark_label(Some("2026-10-10T10:50:00Z"),"2026-10-10 19:03:18 UTC+8"),
            "Obs 2026-10-10 18:50 UTC+8 Upd 2026-10-10 19:03:18 UTC+8");
        assert!(watermark_label(None,"2026-10-10 19:03:18 UTC+8").starts_with("Obs -- Upd "));
    }
    #[test]fn watermark_is_quiet_and_has_zero_edge_padding(){
        let mut row=vec![0u8;1100*4];
        let t="Obs 2026-10-10 18:50 UTC+8 Upd 2026-10-10 19:03:18 UTC+8";
        draw_watermark_row(&mut row,1599,1100,1600,t);
        // The final "8" has a blank outer column in its bottom glyph row.
        assert_eq!(&row[(1100-4)*4..(1100-4)*4+3], &[191,191,191]);
        assert_eq!(&row[0..3], &[0,0,0]);
        row.fill(0);
        // Its center row has a lit rightmost column: no extra right padding.
        draw_watermark_row(&mut row,1595,1100,1600,t);
        assert_eq!(&row[(1100-1)*4..1100*4-1], &[191,191,191]);
    }
    #[test]fn bmp_header_and_row_orientation(){
        let p=std::env::temp_dir().join(format!("cew-test-{}.bmp",std::process::id()));
        let pixels=[1,2,3,255,4,5,6,255, 7,8,9,255,10,11,12,255];
        write_bitmap(&p,2,2,&pixels).unwrap();
        let b=fs::read(&p).unwrap();let _=fs::remove_file(p);
        assert_eq!(&b[0..2],b"BM");
        assert_eq!(&b[54..58],[7,8,9,255]);
    }
}
