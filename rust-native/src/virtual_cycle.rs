//! Immutable work plan for one scheduled refresh. Every virtual desktop receives
//! its own selected satellite irrespective of which desktop is currently visible.
use crate::config::AppConfig;

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct DesktopWork {
    pub id: String,
    pub source: String,
}
pub fn plan(cfg:&AppConfig,ids:&[String])->Result<Vec<DesktopWork>,String>{
    if ids.is_empty(){return Err("未发现虚拟桌面，已跳过本轮更新".into());}
    if ids.len()>128 {return Err("虚拟桌面数量超出安全限制".into());}
    let mut work=Vec::with_capacity(ids.len());
    for id in ids {
        if id.len()!=36 || !id.bytes().enumerate().all(|(i,b)|
            if [8,13,18,23].contains(&i){b==b'-'} else {b.is_ascii_hexdigit()})
        {return Err(format!("Invalid virtual desktop GUID: {id}"));}
        if work.iter().any(|x:&DesktopWork|x.id==*id){continue;}
        work.push(DesktopWork{
            id:id.clone(),
            source:cfg.virtual_desktop_sources.get(id)
                .cloned().unwrap_or_else(||cfg.image_source.clone()),
        });
    }
    Ok(work)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn every_desktop_is_updated_regardless_of_current(){
        let mut cfg=AppConfig::default();
        cfg.image_source="风云4B".into();
        let ids=[
            "11111111-1111-1111-1111-111111111111".to_string(),
            "22222222-2222-2222-2222-222222222222".to_string(),
            "33333333-3333-3333-3333-333333333333".to_string(),
        ];
        cfg.virtual_desktop_sources.insert(ids[0].clone(),"NASA EPIC".into());
        cfg.virtual_desktop_sources.insert(ids[2].clone(),"Himawari-9".into());
        let works=plan(&cfg,&ids).unwrap();
        assert_eq!(works.iter().map(|x|x.source.as_str()).collect::<Vec<_>>(),
            vec!["NASA EPIC","风云4B","Himawari-9"]);
        assert_eq!(works.len(),3);
    }
    #[test]fn rejects_non_guids_and_empty(){
        let cfg=AppConfig::default();
        assert!(plan(&cfg,&[]).is_err());
        assert!(plan(&cfg,&["Desktop 1".into()]).is_err());
    }
}