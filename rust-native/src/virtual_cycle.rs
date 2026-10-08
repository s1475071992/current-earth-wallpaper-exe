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

/// One render-only job for a particular virtual desktop AND physical monitor.
/// Pair-level overrides take precedence over desktop and monitor defaults.
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct PairWork {
    pub desktop_id:String,
    pub monitor_id:String,
    pub source:String,
}
pub fn source_for_pair<'a>(cfg:&'a AppConfig,desktop_id:&str,monitor_id:&str)->&'a str{
    cfg.virtual_monitor_sources.get(desktop_id)
        .and_then(|row|row.get(monitor_id))
        .or_else(||cfg.virtual_desktop_sources.get(desktop_id))
        .or_else(||cfg.monitor_sources.get(monitor_id))
        .unwrap_or(&cfg.image_source)
}
pub fn pair_plan(cfg:&AppConfig,desktops:&[String],monitors:&[String])->Result<Vec<PairWork>,String>{
    // The existing GUID validation and deduplication of desktop identifiers
    // is shared with the supported virtual-desktop-only path.
    let ids=plan(cfg,desktops)?;
    if monitors.is_empty(){return Err("No connected physical displays".into());}
    if monitors.len()>32{return Err("Too many physical displays".into());}
    let mut jobs=Vec::new();
    for desktop in ids {
        let mut seen=std::collections::HashSet::new();
        for monitor_id in monitors {
            if monitor_id.trim().is_empty(){return Err("Missing monitor ID".into());}
            if !seen.insert(monitor_id){continue;}
            jobs.push(PairWork{
                desktop_id:desktop.id.clone(),
                monitor_id:monitor_id.clone(),
                source:source_for_pair(cfg,&desktop.id,monitor_id).to_string(),
            });
        }
    }
    Ok(jobs)
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
    #[test]fn three_desktops_two_monitors_six_independent_jobs(){
        let a="11111111-1111-1111-1111-111111111111".to_string();
        let b="22222222-2222-2222-2222-222222222222".to_string();
        let c="33333333-3333-3333-3333-333333333333".to_string();
        let monitors=["A".to_string(),"B".to_string()];
        let mut cfg=AppConfig::default();
        cfg.monitor_sources.insert("A".into(),"GOES-East".into());
        cfg.virtual_desktop_sources.insert(b.clone(),"NASA EPIC".into());
        cfg.virtual_monitor_sources.entry(b.clone()).or_default().insert("B".into(),"Himawari-9".into());
        cfg.virtual_monitor_sources.entry(c.clone()).or_default().insert("A".into(),"GOES-West".into());
        let jobs=pair_plan(&cfg,&[a,b,c],&monitors).unwrap();
        assert_eq!(jobs.len(),6);
        assert_eq!(jobs.iter().map(|p|p.source.as_str()).collect::<Vec<_>>(),
            vec!["GOES-East","风云4B","NASA EPIC","Himawari-9","GOES-West","风云4B"]);
        assert_ne!(jobs[0].desktop_id,jobs[2].desktop_id);
        assert_ne!(jobs[0].monitor_id,jobs[1].monitor_id);
    }
    #[test]fn rejects_non_guids_and_empty(){
        let cfg=AppConfig::default();
        assert!(plan(&cfg,&[]).is_err());
        assert!(plan(&cfg,&["Desktop 1".into()]).is_err());
    }
}