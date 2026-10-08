use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf, collections::BTreeMap};

pub const SOURCES: [&str; 6] = [
    "风云4B", "GOES-East", "GOES-West", "Himawari-9", "NASA EPIC", "Meteosat (MTG)",
];
pub const SCALES: [&str; 4] = ["铺满屏幕", "原始大小", "黄金比例", "更小尺寸"];
pub const LANGUAGES: [&str; 4] = ["中文", "English", "日本語", "한국어"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub image_source: String,
    pub scale_mode: String,
    pub save_path: String,
    pub interval_minutes: u32,
    pub watermark_on: bool,
    pub show_tray_icon: bool,
    pub log_to_file: bool,
    pub per_monitor_enabled: bool,
    pub monitor_sources: BTreeMap<String,String>,
    pub virtual_desktop_sources: BTreeMap<String,String>,
    pub language: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            image_source: SOURCES[0].into(),
            scale_mode: SCALES[2].into(),
            save_path: String::new(),
            interval_minutes: 30,
            watermark_on: false,
            show_tray_icon: true,
            log_to_file: true,
            per_monitor_enabled: false,
            monitor_sources: BTreeMap::new(),
            virtual_desktop_sources: BTreeMap::new(),
            language: LANGUAGES[0].into(),
        }
    }
}

impl AppConfig {
    pub fn sanitize(&mut self) {
        if !SOURCES.contains(&self.image_source.as_str()) {
            self.image_source = SOURCES[0].into();
        }
        if !SCALES.contains(&self.scale_mode.as_str()) {
            self.scale_mode = SCALES[2].into();
        }
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language = LANGUAGES[0].into();
        }
        self.interval_minutes = self.interval_minutes.clamp(1, 1440);
        self.monitor_sources.retain(|id,source| !id.is_empty() && id.len()<=1024 && SOURCES.contains(&source.as_str()));
        self.virtual_desktop_sources.retain(|id,source| !id.is_empty() && id.len()<=128 && SOURCES.contains(&source.as_str()));
    }
    pub fn language_index(&self) -> usize {
        LANGUAGES.iter().position(|x| *x == self.language).unwrap_or(0)
    }
}

pub fn app_dir() -> PathBuf {
    let root = env::var_os("LOCALAPPDATA")
        .or_else(|| env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    root.join("CurrentEarthWallpaper")
}
pub fn path() -> PathBuf {
    app_dir().join("wallpaper_config.json")
}
pub fn load() -> AppConfig {
    let current = path();
    let legacy = env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("wallpaper_config.json")));
    let bytes = fs::read(&current).ok()
        .or_else(|| legacy.and_then(|p| fs::read(p).ok()));
    let mut cfg = bytes
        .as_deref()
        .and_then(|s| serde_json::from_slice::<AppConfig>(s).ok())
        .unwrap_or_default();
    cfg.sanitize();
    cfg
}
pub fn save(cfg: &AppConfig) -> std::io::Result<()> {
    let dest = path();
    if let Some(dir) = dest.parent() { fs::create_dir_all(dir)?; }
    let tmp = dest.with_extension("json.tmp");
    let contents = serde_json::to_vec_pretty(cfg)?;
    fs::write(&tmp, contents)?;
    fs::rename(tmp, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn python_config_compatibility() {
        let raw = r#"{"image_source":"NASA EPIC","scale_mode":"黄金比例","interval_minutes":15,"show_tray_icon":false,"language":"日本語","watermark_on":true}"#;
        let mut config: AppConfig = serde_json::from_str(raw).unwrap();
        config.sanitize();
        assert_eq!(config.image_source, "NASA EPIC");
        assert!(!config.show_tray_icon);
        assert_eq!(config.language_index(), 2);
    }
    #[test]
    fn display_profiles_roundtrip_and_validation() {
        let mut config=AppConfig::default();
        config.per_monitor_enabled=true;
        config.monitor_sources.insert("MONITOR_A".into(), "GOES-East".into());
        config.monitor_sources.insert("MONITOR_B".into(), "Himawari-9".into());
        config.virtual_desktop_sources.insert("virtual-desktop-guid-1".into(),"NASA EPIC".into());
        let mut loaded:AppConfig=serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        loaded.sanitize();
        assert!(loaded.per_monitor_enabled);
        assert_eq!(loaded.monitor_sources["MONITOR_A"],"GOES-East");
        assert_eq!(loaded.monitor_sources["MONITOR_B"],"Himawari-9");
        assert_eq!(loaded.virtual_desktop_sources["virtual-desktop-guid-1"],"NASA EPIC");
        loaded.monitor_sources.insert("broken".into(),"not a satellite".into());
        loaded.sanitize();
        assert!(!loaded.monitor_sources.contains_key("broken"));
    }
    #[test]
    fn file_logging_backward_compatibility_and_persistence() {
        let old = r#"{"image_source":"GOES-East","show_tray_icon":false}"#;
        let config:AppConfig=serde_json::from_str(old).unwrap();
        assert!(config.log_to_file, "previous settings must continue logging by default");
        let mut disabled = config.clone();
        disabled.log_to_file = false;
        let json=serde_json::to_string(&disabled).unwrap();
        let restored:AppConfig=serde_json::from_str(&json).unwrap();
        assert!(!restored.log_to_file, "off choice must persist across restarts");
    }
    #[test]
    fn bad_values_do_not_crash() {
        let mut config = AppConfig::default();
        config.image_source = "unknown".into();
        config.interval_minutes = 0;
        config.language = "???".into();
        config.sanitize();
        assert_eq!(config.image_source, "风云4B");
        assert_eq!(config.interval_minutes, 1);
        assert_eq!(config.language, "中文");
    }
}
