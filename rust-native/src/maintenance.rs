//! Bounded cleanup of **only** files created by Current Earth Wallpaper.
//! Never delete another application's files or the most recent (possibly active)
//! wallpaper for a desktop + satellite profile.
use std::{collections::{BTreeMap,HashSet},fs,path::{Path,PathBuf},time::{Duration,SystemTime}};

const MAX_CACHE_BYTES:u64=512*1024*1024;
const MAX_CACHE_FILES:usize=64;
const MAX_CACHE_AGE:Duration=Duration::from_secs(14*24*3600);
const MAX_TEMP_AGE:Duration=Duration::from_secs(24*3600);

#[derive(Default,Debug)]
pub struct Report {pub removed:usize,pub freed:u64,pub kept:usize}

#[derive(Clone)]
struct Candidate{
    path:PathBuf,key:String,size:u64,modified:SystemTime,
}
fn group(name:&str)->Option<String>{
    if !name.starts_with("vd-")||!name.ends_with(".bmp"){return None}
    let parts=name[3..name.len()-4].split('-').collect::<Vec<_>>();
    if parts.len()!=4||parts[..3].iter().any(|s|s.len()!=16||!s.bytes().all(|c|c.is_ascii_hexdigit()))
       ||parts[3].is_empty()||!parts[3].bytes().all(|c|c.is_ascii_digit()){
       return None
    }
    Some(format!("vd-{}-{}-{}",parts[0],parts[1],parts[2]))
}
fn collect(folder:&Path)->Vec<Candidate>{
    let mut out=Vec::new();
    let Ok(dir)=fs::read_dir(folder)else{return out};
    for entry in dir.flatten(){
        let path=entry.path();
        let Some(name)=path.file_name().and_then(|s|s.to_str())else{continue};
        let Some(key)=group(name)else{continue};
        let Ok(meta)=entry.metadata()else{continue};
        if !meta.is_file(){continue}
        let modified=meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        out.push(Candidate{path,key,size:meta.len(),modified});
    }
    out
}
/// Keep two most recent images per profile. Older copies are eligible for
/// removal at 14 days and when aggregate size/count exceeds bounds.
/// Latest copies remain even if they are older than 14 days: Explorer may still
/// reference their paths, including for inactive virtual desktops.
pub fn prune_virtual(folder:&Path)->Report{
    let now=SystemTime::now();
    let mut files=collect(folder);
    files.sort_by(|a,b|b.modified.cmp(&a.modified).then_with(||b.path.cmp(&a.path)));
    let mut protected=HashSet::new();
    let mut counts:BTreeMap<String,usize>=BTreeMap::new();
    for f in &files{
        let count=counts.entry(f.key.clone()).or_default();
        if *count<2{protected.insert(f.path.clone());}
        *count+=1;
    }
    let mut total=files.iter().map(|f|f.size).sum::<u64>();
    let mut count=files.len();
    let mut report=Report::default();
    // Delete oldest non-protected images first.
    for f in files.iter().rev(){
        if protected.contains(&f.path){continue;}
        let old=now.duration_since(f.modified).unwrap_or_default()>MAX_CACHE_AGE;
        if !old && count<=MAX_CACHE_FILES && total<=MAX_CACHE_BYTES{continue;}
        if fs::remove_file(&f.path).is_ok(){
            total=total.saturating_sub(f.size);
            count=count.saturating_sub(1);
            report.removed+=1;
            report.freed=report.freed.saturating_add(f.size);
        }
    }
    report.kept=count;
    report
}
/// Clean incomplete download/render scratch files only after 24 hours.
/// Never touch unrelated user files. Only used inside the app-managed directory.
pub fn prune_scratch(folder:&Path)->Report{
    let mut report=Report::default();
    let Ok(dir)=fs::read_dir(folder)else{return report};
    for e in dir.flatten(){
        let path=e.path();
        let Some(name)=path.file_name().and_then(|s|s.to_str())else{continue};
        if !name.starts_with(".cew-") {continue}
        let Ok(meta)=e.metadata()else{continue};
        if !meta.is_file(){continue}
        if meta.modified().ok().and_then(|t|t.elapsed().ok()).is_none_or(|d|d<MAX_TEMP_AGE){continue;}
        if fs::remove_file(&path).is_ok(){report.removed+=1;report.freed+=meta.len();}
    }
    report
}
#[cfg(test)]
mod tests{
    use super::*;
    fn make()->PathBuf{
        let p=std::env::temp_dir().join(format!("cew-prune-{}-{:?}",std::process::id(),std::thread::current().id()));
        fs::create_dir_all(&p).unwrap();p
    }
    fn sample(n:u64)->String{format!("vd-0123456789abcdef-0123456789abcdef-0123456789abcdef-{n}.bmp")}
    #[test]fn preserves_latest_two_and_ignores_other_files(){
        let p=make();
        for n in 0..4{fs::write(p.join(sample(n)),vec![1u8;10]).unwrap();}
        fs::write(p.join("wallpaper_config.json"),b"never touch").unwrap();
        fs::write(p.join("other.bmp"),b"never touch").unwrap();
        // Quota not reached, all recent images can stay.
        let report=prune_virtual(&p);assert_eq!(report.removed,0);
        assert!(p.join(sample(3)).exists());
        assert!(p.join("wallpaper_config.json").exists());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]fn malformed_virtual_cache_names_never_eligible(){
        assert!(group("vd-bad.bmp").is_none());
        assert!(group("vd-0123456789abcdef-0123456789abcdef-0123456789abcdef-3.bmp").is_some());
    }
}
