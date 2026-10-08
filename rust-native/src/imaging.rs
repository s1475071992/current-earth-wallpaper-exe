//! Windows Imaging Component downsampling; bounded final pixel buffers.
//! Oversized GOES images are clipped and resized by WIC before CopyPixels.
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path};
use windows::{core::{Interface,PCWSTR}, Win32::{
    Foundation::GENERIC_READ,
    System::Com::{CoCreateInstance,CoInitializeEx,CoUninitialize,CLSCTX_INPROC_SERVER,COINIT_MULTITHREADED},
    Graphics::Imaging::*,
}};
pub struct Pixels { pub width:u32, pub height:u32, pub bgra:Vec<u8> }
struct Apartment;
impl Apartment {fn new()->Result<Self,String>{
    unsafe { CoInitializeEx(None,COINIT_MULTITHREADED).ok().map_err(|e|format!("COM init: {e}"))?; }
    Ok(Self)
}}
impl Drop for Apartment{fn drop(&mut self){unsafe{CoUninitialize();}}}
fn err<E:std::fmt::Display>(error:E)->String{error.to_string()}
/// crop is a reference-coordinate tuple (left, top, side, original reference width).
pub fn load_scaled(path:&Path,target:u32,crop:Option<(u32,u32,u32,u32)>)->Result<Pixels,String>{
    if target==0||target>8192{return Err("Invalid output diameter".into())}
    let _com=Apartment::new()?;
    unsafe {
        let factory:IWICImagingFactory=CoCreateInstance(&CLSID_WICImagingFactory,None,CLSCTX_INPROC_SERVER).map_err(err)?;
        let ws:Vec<u16>=OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let decoder=factory.CreateDecoderFromFilename(PCWSTR(ws.as_ptr()),None,
            GENERIC_READ,WICDecodeMetadataCacheOnDemand).map_err(err)?;
        let frame=decoder.GetFrame(0).map_err(err)?;
        let mut width=0u32;let mut height=0u32;
        frame.GetSize(&mut width,&mut height).map_err(err)?;
        if width==0||height==0||width>32768||height>32768{return Err("Invalid source image size".into());}
        let mut side=width.min(height);
        let (mut x,mut y)=((width-side)/2,(height-side)/2);
        if let Some((cx,cy,cs,base))=crop {
            if base>0 {
                let scale=width as f64/base as f64;
                let sc=((cs as f64*scale).round() as u32).min(side).max(1);
                side=sc;
                x=((cx as f64*scale).round() as u32).min(width-side);
                y=((cy as f64*scale).round() as u32).min(height-side);
            }
        }
        let clip=factory.CreateBitmapClipper().map_err(err)?;
        clip.Initialize(&frame,&WICRect{X:x as i32,Y:y as i32,Width:side as i32,Height:side as i32}).map_err(err)?;
        let scaler=factory.CreateBitmapScaler().map_err(err)?;
        scaler.Initialize(&clip,target,target,WICBitmapInterpolationModeFant).map_err(err)?;
        let conv=factory.CreateFormatConverter().map_err(err)?;
        conv.Initialize(&scaler,&GUID_WICPixelFormat32bppBGRA,WICBitmapDitherTypeNone,
            None,0.0,WICBitmapPaletteTypeCustom).map_err(err)?;
        let len=(target as usize).checked_mul(target as usize).and_then(|v|v.checked_mul(4)).ok_or("Output too large")?;
        let mut bgra=vec![0u8;len];
        conv.CopyPixels(std::ptr::null(),target*4,&mut bgra).map_err(err)?;
        Ok(Pixels{width:target,height:target,bgra})
    }
}
