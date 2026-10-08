//! Native wallpaper composition with scanline BMP output: O(screen width) canvas memory.
use std::{fs::{self,File},io::{Write,BufWriter},path::{Path,PathBuf},ffi::OsStr,os::windows::ffi::OsStrExt};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics,SM_CXSCREEN,SM_CYSCREEN,SystemParametersInfoW,SPI_SETDESKWALLPAPER,
    SPIF_UPDATEINIFILE,SPIF_SENDCHANGE,
};
use windows_sys::Win32::System::Time::{GetLocalTime,SYSTEMTIME};
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
fn glyph(c:char)->[u8;7]{
    match c {
        '0'=>[14,17,19,21,25,17,14], '1'=>[4,12,4,4,4,4,14],
        '2'=>[14,17,1,2,4,8,31], '3'=>[30,1,1,14,1,1,30],
        '4'=>[2,6,10,18,31,2,2], '5'=>[31,16,16,30,1,1,30],
        '6'=>[14,16,16,30,17,17,14], '7'=>[31,1,2,4,8,8,8],
        '8'=>[14,17,17,14,17,17,14], '9'=>[14,17,17,15,1,1,14],
        '/'=>[1,1,2,4,8,16,16], ':'=>[0,4,4,0,4,4,0],
        '-'=>[0,0,0,31,0,0,0], _=>[0;7]
    }
}
fn time_mark()->String {
    let mut t: SYSTEMTIME=unsafe{std::mem::zeroed()};
    unsafe{GetLocalTime(&mut t);}
    format!("{:04}/{:02}/{:02} {:02}:{:02}:{:02}",t.wYear,t.wMonth,t.wDay,t.wHour,t.wMinute,t.wSecond)
}
fn draw_watermark_row(row:&mut [u8],screen_y:u32,w:u32,h:u32,text:&str){
    let px_scale=(h/500).max(1);
    let char_w=6*px_scale;
    let text_w=text.chars().count() as u32*char_w;
    let start_x=w.saturating_sub(text_w+h/35);
    let start_y=h.saturating_sub(8*px_scale+h/28);
    if screen_y<start_y||screen_y>=start_y+7*px_scale{return}
    let glyph_y=((screen_y-start_y)/px_scale) as usize;
    for (i,c) in text.chars().enumerate(){
        let bits=glyph(c)[glyph_y];
        for bit in 0..5 {
            if bits&(1<<(4-bit))==0{continue}
            let x=start_x+i as u32*char_w+bit*px_scale;
            for dx in 0..px_scale{
                if x+dx>=w{continue}
                let p=(x+dx) as usize*4;
                row[p..p+4].copy_from_slice(&[255,255,255,255]);
            }
        }
    }
}
pub fn compose(dest:&Path,w:u32,h:u32,earth:&Pixels,watermark:bool)->Result<(),String>{
    let d=earth.width.min(earth.height);
    if d==0||d>w||d>h{return Err("Invalid Earth size".into())}
    let x0=(w-d)/2; let y0=(h-d)/2;
    let mut bmp=BufWriter::new(File::create(dest).map_err(|e|e.to_string())?);
    header(&mut bmp,w,h)?;
    let mut row=vec![0u8;w as usize*4];
    let rad=d as i64;
    let mark=if watermark{time_mark()}else{String::new()};
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
    let mut entries:Vec<PathBuf>=fs::read_dir(folder).into_iter().flatten().flatten()
        .map(|e|e.path())
        .filter(|p|p.file_name().and_then(|n|n.to_str())
            .is_some_and(|n|n.starts_with("cew-")&&n.ends_with(".bmp"))).collect();
    entries.sort();
    for p in entries.iter().take(entries.len().saturating_sub(max_count)){
        if p!=active { let _=fs::remove_file(p); }
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn size_modes(){assert_eq!(diameter(1000,"铺满屏幕"),1000);
        assert_eq!(diameter(1000,"黄金比例"),618);}
    #[test]fn bmp_header_and_row_orientation(){
        let p=std::env::temp_dir().join(format!("cew-test-{}.bmp",std::process::id()));
        let pixels=[1,2,3,255,4,5,6,255, 7,8,9,255,10,11,12,255];
        write_bitmap(&p,2,2,&pixels).unwrap();
        let b=fs::read(&p).unwrap();let _=fs::remove_file(p);
        assert_eq!(&b[0..2],b"BM");
        assert_eq!(&b[54..58],[7,8,9,255]);
    }
}
