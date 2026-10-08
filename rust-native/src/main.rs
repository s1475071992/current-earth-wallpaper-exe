#![cfg_attr(windows, windows_subsystem = "windows")]
//! Stable Rust v1.0: native Win32 configuration, per-virtual-desktop wallpapers,
//! controlled background refresh and sleep/resume safety.

mod config;
mod sources;
#[cfg(windows)] mod http;
#[cfg(windows)] mod imaging;
#[cfg(windows)] mod wallpaper;
#[cfg(windows)] mod engine;
#[cfg(windows)] mod metrics;
#[cfg(windows)] mod autostart;
#[cfg(windows)] mod monitor;
#[cfg(windows)] mod virtual_desktop;
#[cfg(windows)] mod virtual_cache;
mod virtual_cycle;
#[cfg(windows)] mod display_apply;
mod scheduler;
mod maintenance;
#[cfg(windows)] mod virtual_wallpaper;

#[cfg(not(windows))]
fn main() {
    eprintln!("Current Earth Wallpaper v1.0.0 supports Windows; unit tests are portable.");
}

#[cfg(windows)]
mod winapp {
    use super::config::{self, AppConfig, LANGUAGES, SCALES, SOURCES};
    use std::sync::{Mutex, OnceLock,Arc,atomic::{AtomicBool,Ordering}};
    use std::collections::VecDeque;
    use std::{fs::{self,OpenOptions},io::Write};
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use std::time::{Duration,Instant};
    use std::{ffi::c_void, ptr::{null, null_mut}};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT},
            Shell::{Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE},
            WindowsAndMessaging::*,
        },
    };

    const ID_SOURCE: u16 = 101;
    const ID_SCALE: u16 = 102;
    const ID_LANGUAGE: u16 = 103;
    const ID_INTERVAL: u16 = 104;
    const ID_TRAY_CHECK: u16 = 105;
    const ID_WATERMARK: u16 = 106;
    const ID_START: u16 = 107;
    const ID_EXIT: u16 = 108;
    const ID_PATH: u16 = 109;
    const ID_AUTOSTART: u16 = 110;
    const ID_CLEAR_LOG: u16 = 111;
    const ID_FILE_LOG: u16 = 112;
    const ID_MULTI_MONITOR: u16 = 113;
    const ID_MONITOR_PICKER: u16 = 114;
    const ID_VDESK_PICKER:u16=117;
    const ID_VDESK_SOURCE:u16=118;
    const ID_VDESK_PROBE:u16=119;
    const ID_MONITOR_PROBE:u16=120;
    const EM_SETSEL:u32=0x00B1; // Edit control selection
    const EM_SCROLLCARET:u32=0x00B7; // Scroll to caret
    const ID_SHOW: u16 = 201;
    const TIMER_ID:usize=1;
    const REFRESH_DONE:u32=WM_APP+2;
    const REFRESH_PROGRESS:u32=WM_APP+3;
    const ID_TRAY_EXIT: u16 = 202;
    const HOTKEY_ID: i32 = 0xEA47;
    const TRAY_ID: u32 = 17;
    const TRAY_MESSAGE: u32 = WM_APP + 1;

    fn w(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }
    fn h(handle: usize) -> HWND { handle as HWND }
    fn set_text(hwnd: HWND, text: &str) {
        unsafe { SetWindowTextW(hwnd, w(text).as_ptr()); }
    }
    fn lang_text(lang: usize, key: usize) -> &'static str {
        const DICT: [[&str; 30]; 4] = [
            ["实时地球壁纸 · v1.1 测试版", "卫星图源", "壁纸大小", "界面语言", "更新间隔（分钟）", "显示托盘图标", "显示时间水印", "开始更新", "退出程序", "配置已保存。壁纸下载与渲染引擎正在迁移。", "原生引擎尚未完成，请勿替代正式版。", "无法隐藏托盘：Ctrl+Alt+E 已被其他软件占用。", "显示主窗口", "图像保存目录", "开机自动更新", "停止更新", "正在更新壁纸…", "执行日志", "清空显示", "关闭窗口：点击“是”隐藏并继续后台更新；点击“否”彻底退出并停止更新；“取消”留在界面。", "关闭窗口", "将运行日志保存到文件", "为每个虚拟桌面及显示器设置卫星源", "显示器", "该显示器的卫星源", "虚拟桌面独立壁纸（26H2 兼容模式）", "虚拟桌面", "所选桌面＋显示器的卫星源", "先选桌面、再选显示器，最后选择卫星", "检测虚拟桌面"],
            ["Current Earth Wallpaper · v1.1 beta", "Satellite source", "Wallpaper size", "Interface language", "Update interval (minutes)", "Show tray icon", "Time watermark", "Start updating", "Exit app", "Settings saved. Native image engine is being ported.", "Native image engine isn't ready yet. Keep using the stable build.", "Cannot hide tray: Ctrl+Alt+E is in use.", "Show window", "Image folder", "Start with Windows", "Stop updating", "Updating wallpaper...", "Execution log", "Clear view", "Close window: Yes hides and keeps updating; No quits and stops; Cancel stays.", "Close window", "Save logs to file", "Satellite for each desktop + monitor", "Monitor", "Satellite for this monitor", "Virtual desktop wallpapers (26H2 compatibility)", "Virtual desktop", "Satellite for selected desktop + monitor", "Choose desktop, monitor and satellite", "Check desktops"],
            ["リアルタイム地球壁紙 · v1.1 ベータ", "衛星ソース", "壁紙の大きさ", "表示言語", "更新間隔（分）", "トレイアイコンを表示", "時刻の透かし", "更新開始", "終了", "設定を保存しました。画像エンジンは移植中です。", "画像エンジンはまだ未完成です。", "トレイを隠せません。Ctrl+Alt+E は使用中です。", "ウィンドウを表示", "画像の保存先", "Windows起動時に自動更新", "更新停止", "壁紙を更新中…", "実行ログ", "表示を消去", "はい：非表示で更新継続。いいえ：終了して更新停止。キャンセル：戻る。", "ウィンドウを閉じる", "ログをファイルに保存", "仮想デスクトップとモニター別に衛星を指定", "モニター", "このモニターの衛星", "仮想デスクトップ別壁紙（26H2対応）", "仮想デスクトップ", "選択デスクトップ＋モニターの衛星", "デスクトップとモニターから衛星を選択", "デスクトップ検出"],
            ["실시간 지구 배경화면 · v1.1 베타", "위성 소스", "배경화면 크기", "인터페이스 언어", "갱신 간격(분)", "트레이 아이콘 표시", "시간 워터마크", "업데이트 시작", "종료", "설정 저장됨. 이미지 엔진을 이식하는 중입니다.", "이미지 엔진이 아직 준비되지 않았습니다.", "트레이 숨기기 불가: Ctrl+Alt+E 사용 중.", "창 표시", "이미지 저장 폴더", "Windows 시작 시 자동 업데이트", "업데이트 중지", "배경화면 갱신 중…", "실행 로그", "보기 지우기", "예: 숨기고 계속 업데이트. 아니요: 종료 및 중지. 취소: 돌아가기.", "창 닫기", "실행 로그 파일에 저장", "데스크톱과 모니터별로 위성 선택", "모니터", "이 모니터의 위성", "가상 데스크톱별 배경화면 (26H2 호환)", "가상 데스크톱", "선택 데스크톱＋모니터의 위성", "데스크톱과 모니터를 선택한 후 위성 지정", "바탕 화면 감지"],
        ];
        DICT[lang.min(3)][key.min(29)]
    }

    struct Ui {
        cfg: AppConfig,
        parent: usize,
        labels: [usize; 4],
        source: usize,
        scale: usize,
        language: usize,
        interval: usize,
        tray_checkbox: usize,
        watermark: usize,
        start: usize,
        exit: usize,
        status: usize,
        log_area:usize,
        log_label:usize,
        clear_log:usize,
        file_log_checkbox:usize,
        monitor_enabled:usize,
        monitor_picker:usize,
        monitor_picker_label:usize,
        monitor_probe:usize,
        monitors:Vec<crate::monitor::Monitor>,
        vdesk_picker:usize,
        vdesk_source:usize,
        vdesk_label:usize,
        vdesk_source_label:usize,
        vdesk_hint:usize,
        vdesk_probe:usize,
        vdesk_candidate:Option<String>,
        vdesk_candidate_count:u8,
        vdesk_heartbeat:Instant,
        vdesks:Vec<String>,
        active_vdesk:Option<String>,
        vdesk_error_reported:bool,
        log_lines:VecDeque<String>,
        cancel:Arc<AtomicBool>,
        icon: usize,
        tray_added: bool,
        restore_hotkey: bool,
        path_label:usize,
        path_edit:usize,
        autostart_check:usize,
        running:bool,
        busy:bool,
        failures:u32,
        next_due:Instant,
        wake:crate::scheduler::WakeGate,
        last_maintenance:Instant,
    }
    static UI: OnceLock<Mutex<Ui>> = OnceLock::new();

    unsafe fn control(parent: HWND, class: &str, title: &str, x:i32, y:i32, width:i32, height:i32, style:u32, id:u16) -> HWND {
        let hwnd = CreateWindowExW(
            0, w(class).as_ptr(), w(title).as_ptr(), WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
            x, y, width, height, parent, id as usize as *mut c_void,
            GetModuleHandleW(null()), null(),
        );
        if !hwnd.is_null() {
            SendMessageW(hwnd, WM_SETFONT, GetStockObject(DEFAULT_GUI_FONT) as WPARAM, 1);
        }
        hwnd
    }
    unsafe fn choose(combo: HWND, list: &[&str], index: usize) {
        for value in list {
            SendMessageW(combo, CB_ADDSTRING, 0, w(value).as_ptr() as LPARAM);
        }
        SendMessageW(combo, CB_SETCURSEL, index, 0);
    }
    unsafe fn check(hwnd: HWND, value: bool) {
        SendMessageW(hwnd, BM_SETCHECK, if value { 1usize } else { 0usize }, 0);
    }
    unsafe fn selected(hwnd: HWND) -> usize {
        let result = SendMessageW(hwnd, CB_GETCURSEL, 0, 0);
        if result < 0 { 0 } else { result as usize }
    }
    unsafe fn checked(hwnd: HWND) -> bool {
        SendMessageW(hwnd, BM_GETCHECK, 0, 0) == 1isize
    }

    impl Ui {
        unsafe fn new(hwnd: HWND) -> Self {
            let mut cfg = config::load();
            // Upgrade either legacy dimension into the unified pair mode.
            if cfg.per_monitor_enabled || cfg.virtual_desktops_enabled {
                cfg.per_monitor_enabled=true;
                cfg.virtual_desktops_enabled=true;
            }
            let hotkey = RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, b'E' as u32) != 0;
            if !hotkey { cfg.show_tray_icon = true; }
            if cfg.save_path.is_empty(){cfg.save_path=config::app_dir().join("wallpapers").to_string_lossy().into_owned();}
            let labels = [
                control(hwnd, "STATIC", "", 24, 28, 185, 24, 0, 0),
                control(hwnd, "STATIC", "", 24, 72, 185, 24, 0, 0),
                control(hwnd, "STATIC", "", 24, 116, 185, 24, 0, 0),
                control(hwnd, "STATIC", "", 24, 160, 185, 24, 0, 0),
            ];
            let path_label=control(hwnd,"STATIC","",24,198,140,24,0,0);
            let path_edit=control(hwnd,"EDIT",&cfg.save_path,164,195,276,27,
                WS_BORDER|ES_AUTOHSCROLL as u32,ID_PATH);
            let combo_style = CBS_DROPDOWNLIST as u32 | WS_VSCROLL;
            let source = control(hwnd, "COMBOBOX", "", 210, 24, 230, 190, combo_style, ID_SOURCE);
            let scale = control(hwnd, "COMBOBOX", "", 210, 68, 230, 150, combo_style, ID_SCALE);
            let language = control(hwnd, "COMBOBOX", "", 210, 112, 230, 150, combo_style, ID_LANGUAGE);
            let interval = control(hwnd, "EDIT", &cfg.interval_minutes.to_string(), 210, 156, 230, 26,
                WS_BORDER | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32, ID_INTERVAL);
            choose(source, &SOURCES, SOURCES.iter().position(|x| *x == cfg.image_source).unwrap_or(0));
            choose(scale, &SCALES, SCALES.iter().position(|x| *x == cfg.scale_mode).unwrap_or(2));
            choose(language, &LANGUAGES, cfg.language_index());
            let tray_checkbox = control(hwnd, "BUTTON", "", 24, 238, 260, 26,
                BS_AUTOCHECKBOX as u32, ID_TRAY_CHECK);
            let watermark = control(hwnd, "BUTTON", "", 24, 273, 260, 26,
                BS_AUTOCHECKBOX as u32, ID_WATERMARK);
            let autostart_check=control(hwnd,"BUTTON","",24,308,280,26,
                BS_AUTOCHECKBOX as u32,ID_AUTOSTART);
            check(autostart_check,crate::autostart::enabled());
            check(tray_checkbox, cfg.show_tray_icon);
            check(watermark, cfg.watermark_on);
            let start = control(hwnd, "BUTTON", "", 24, 348, 201, 35,
                BS_PUSHBUTTON as u32, ID_START);
            let exit = control(hwnd, "BUTTON", "", 245, 348, 195, 35,
                BS_PUSHBUTTON as u32, ID_EXIT);
            let (monitors,monitor_warning)=match crate::monitor::connected() {
                Ok(v)=>(v,None),
                Err(e)=>(Vec::new(),Some(e)),
            };
            let monitor_enabled=control(hwnd,"BUTTON","",24,389,416,26,BS_AUTOCHECKBOX as u32,ID_MULTI_MONITOR);
            check(monitor_enabled,cfg.per_monitor_enabled && cfg.virtual_desktops_enabled);
            let (vdesks,active_vdesk,vdesk_warning)=match crate::virtual_desktop::snapshot(){
                Ok(snapshot)=>(snapshot.ids,snapshot.current,None),
                Err(e)=>(Vec::new(),None,Some(e)),
            };
            let vdesk_label=control(hwnd,"STATIC","",24,427,185,24,0,0);
            let vdesk_picker=control(hwnd,"COMBOBOX","",210,425,230,160,combo_style,ID_VDESK_PICKER);
            for (i,_) in vdesks.iter().enumerate(){
                SendMessageW(vdesk_picker,CB_ADDSTRING,0,w(&format!("Desktop {}",i+1)).as_ptr() as LPARAM);
            }
            let active_idx=active_vdesk.as_ref().and_then(|v|vdesks.iter().position(|x|x==v)).unwrap_or(0);
            if !vdesks.is_empty(){SendMessageW(vdesk_picker,CB_SETCURSEL,active_idx,0);}
            let monitor_picker_label=control(hwnd,"STATIC","",24,470,185,24,0,0);
            let monitor_picker=control(hwnd,"COMBOBOX","",210,467,230,160,combo_style,ID_MONITOR_PICKER);
            for (i,m) in monitors.iter().enumerate(){
                SendMessageW(monitor_picker,CB_ADDSTRING,0,w(&format!("{} — {}×{}",i+1,m.width,m.height)).as_ptr() as LPARAM);
            }
            if !monitors.is_empty(){SendMessageW(monitor_picker,CB_SETCURSEL,0,0);}
            let vdesk_source_label=control(hwnd,"STATIC","",24,511,185,24,0,0);
            let vdesk_source=control(hwnd,"COMBOBOX","",210,507,230,180,combo_style,ID_VDESK_SOURCE);
            let initial_source=match (vdesks.get(active_idx),monitors.first()){
                (Some(id),Some(m))=>crate::virtual_cycle::source_for_pair(&cfg,id,&m.id),
                _=>cfg.image_source.as_str(),
            };
            choose(vdesk_source,&SOURCES,SOURCES.iter().position(|x|*x==initial_source).unwrap_or(0));
            let vdesk_probe=control(hwnd,"BUTTON","",24,551,194,30,BS_PUSHBUTTON as u32,ID_VDESK_PROBE);
            let monitor_probe=control(hwnd,"BUTTON","",230,551,210,30,BS_PUSHBUTTON as u32,ID_MONITOR_PROBE);
            let vdesk_hint=control(hwnd,"STATIC","",24,592,416,26,0,0);
            let status = control(hwnd, "STATIC", "", 24,659, 416, 28, 0, 0);
            let clear_log = control(hwnd,"BUTTON","",730,24,90,29,BS_PUSHBUTTON as u32,ID_CLEAR_LOG);
            let file_log_checkbox=control(hwnd,"BUTTON","",24,694,410,26,
                BS_AUTOCHECKBOX as u32,ID_FILE_LOG);
            check(file_log_checkbox,cfg.log_to_file);
            let log_label=control(hwnd,"STATIC","",465,25,190,24,0,0);
            let log_area=control(hwnd,"EDIT","",465,55,355,663,
                WS_BORDER|WS_VSCROLL|ES_MULTILINE as u32|ES_AUTOVSCROLL as u32|ES_READONLY as u32,0);
            let icon = LoadIconW(null_mut(), IDI_APPLICATION);
            let mut ui = Self {
                cfg, parent: hwnd as usize, labels: labels.map(|x| x as usize),
                source: source as usize, scale: scale as usize, language: language as usize,
                interval: interval as usize, tray_checkbox: tray_checkbox as usize,
                watermark: watermark as usize, start: start as usize, exit: exit as usize,
                status: status as usize, icon: icon as usize,
                log_area: log_area as usize, log_label:log_label as usize,
                clear_log:clear_log as usize, file_log_checkbox:file_log_checkbox as usize,
                monitor_enabled:monitor_enabled as usize,monitor_picker:monitor_picker as usize,
                monitor_picker_label:monitor_picker_label as usize,monitor_probe:monitor_probe as usize,
                monitors,vdesk_picker:vdesk_picker as usize,
                vdesk_source:vdesk_source as usize,vdesk_label:vdesk_label as usize,
                vdesk_source_label:vdesk_source_label as usize,vdesk_hint:vdesk_hint as usize,
                vdesk_probe:vdesk_probe as usize,vdesk_candidate:None,vdesk_candidate_count:0,
                vdesk_heartbeat:Instant::now(),
                vdesks,active_vdesk,vdesk_error_reported:false,
                log_lines:VecDeque::new(),
                cancel:Arc::new(AtomicBool::new(false)),
                tray_added: false, restore_hotkey: hotkey,
                path_label:path_label as usize,path_edit:path_edit as usize,
                autostart_check:autostart_check as usize,
                running:std::env::args().any(|a|a=="--autostart"),busy:false,failures:0,
                next_due:Instant::now(),
                wake:crate::scheduler::WakeGate::default(),
                last_maintenance:Instant::now(),
            };
            ui.localize();
            ui.sync_virtual_source();
            ui.update_tray();
            ui.append_event("Current Earth Wallpaper v1.1 beta · 双模式测试版".into());
            ui.append_event("应用已启动；右上角 × 可以选择后台运行或彻底退出。".into());
            ui.append_event(format!("当前连接的物理显示器：{} 台",ui.monitors.len()));
            if let Some(warning)=monitor_warning{ui.append_event(format!("显示器枚举失败：{warning}"));}
            if let Some(warning)=vdesk_warning{ui.append_event(format!("虚拟桌面：{warning}"));}
            ui.append_event(format!("已检测虚拟桌面 {} 个；当前桌面：{}",ui.vdesks.len(),ui.active_vdesk.as_deref().unwrap_or("未知")));
            if ui.cfg.log_to_file {
                ui.append_event(format!("日志文件：{}",config::app_dir().join("logs").join("current.log").display()));
            } else {
                ui.append_event("文件日志已关闭；窗口中仍显示运行记录。".into());
            }
            ui
        }
        unsafe fn localize(&self) {
            let l = self.cfg.language_index();
            set_text(h(self.parent), lang_text(l,0));
            for (i, hwnd) in self.labels.iter().enumerate() {
                set_text(h(*hwnd),lang_text(l,i+1));
            }
            set_text(h(self.path_label),lang_text(l,13));
            set_text(h(self.autostart_check),lang_text(l,14));
            set_text(h(self.tray_checkbox),lang_text(l,5));
            set_text(h(self.watermark),lang_text(l,6));
            set_text(h(self.start),lang_text(l,if self.running{15}else{7}));
            set_text(h(self.exit),lang_text(l,8));
            set_text(h(self.log_label),lang_text(l,17));
            set_text(h(self.clear_log),lang_text(l,18));
            set_text(h(self.file_log_checkbox),lang_text(l,21));
            set_text(h(self.monitor_enabled),lang_text(l,22));
            set_text(h(self.monitor_picker_label),lang_text(l,23));
            set_text(h(self.vdesk_label),lang_text(l,26));
            set_text(h(self.vdesk_source_label),lang_text(l,27));
            set_text(h(self.vdesk_hint),lang_text(l,28));
            set_text(h(self.monitor_probe),match l{
                0=>"检测显示器",1=>"Detect displays",2=>"モニターを検出",_=>"모니터 감지",
            });
        }
        unsafe fn tray_data(&self) -> NOTIFYICONDATAW {
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = h(self.parent);
            data.uID = TRAY_ID;
            data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            data.uCallbackMessage = TRAY_MESSAGE;
            data.hIcon = h(self.icon) as _;
            let tip = w("Current Earth Wallpaper - Rust");
            data.szTip[..tip.len()].copy_from_slice(&tip);
            data
        }
        unsafe fn update_tray(&mut self) {
            if self.cfg.show_tray_icon && !self.tray_added {
                self.tray_added = Shell_NotifyIconW(NIM_ADD, &self.tray_data()) != 0;
            } else if !self.cfg.show_tray_icon && self.tray_added {
                Shell_NotifyIconW(NIM_DELETE, &self.tray_data());
                self.tray_added = false;
            }
        }
        unsafe fn save_changes(&mut self) {
            self.cfg.image_source = SOURCES[selected(h(self.source)).min(SOURCES.len()-1)].into();
            self.cfg.scale_mode = SCALES[selected(h(self.scale)).min(SCALES.len()-1)].into();
            self.cfg.language = LANGUAGES[selected(h(self.language)).min(LANGUAGES.len()-1)].into();
            self.cfg.watermark_on = checked(h(self.watermark));
            self.cfg.log_to_file = checked(h(self.file_log_checkbox));
            let combined=checked(h(self.monitor_enabled));
            self.cfg.per_monitor_enabled=combined;
            let was_vdesk_enabled=self.cfg.virtual_desktops_enabled;
            self.cfg.virtual_desktops_enabled=combined;
            if was_vdesk_enabled!=self.cfg.virtual_desktops_enabled{
                self.append_event(format!("虚拟桌面模式已{}；已识别 {} 个桌面；当前：{}",if self.cfg.virtual_desktops_enabled{"开启"}else{"关闭"},self.vdesks.len(),self.active_vdesk.as_deref().unwrap_or("未知")));
                self.vdesk_candidate=None;
                self.vdesk_candidate_count=0;
                self.append_event("虚拟桌面设置已保存；下一轮后台定时更新生效。".into());
            }
            let mut folder=[0u16;2048];
            GetWindowTextW(h(self.path_edit),folder.as_mut_ptr(),folder.len() as i32);
            self.cfg.save_path=String::from_utf16_lossy(&folder).trim_matches('\0').trim().to_string();
            if self.cfg.save_path.is_empty(){
                self.cfg.save_path=config::app_dir().join("wallpapers").to_string_lossy().into_owned();
            }
            let want_autostart=checked(h(self.autostart_check));
            if want_autostart!=crate::autostart::enabled(){
                if crate::autostart::set(want_autostart).is_err(){
                    check(h(self.autostart_check),crate::autostart::enabled());
                }
            }
            let mut digits = [0u16; 32];
            GetWindowTextW(h(self.interval), digits.as_mut_ptr(), digits.len() as i32);
            let input = String::from_utf16_lossy(&digits).trim_matches('\0').trim().to_string();
            self.cfg.interval_minutes = input.parse().unwrap_or(30);
            self.cfg.sanitize();
            if checked(h(self.tray_checkbox)) != self.cfg.show_tray_icon {
                if !checked(h(self.tray_checkbox)) && !self.restore_hotkey {
                    check(h(self.tray_checkbox), true);
                    MessageBoxW(h(self.parent), w(lang_text(self.cfg.language_index(),11)).as_ptr(), w("Hotkey").as_ptr(), MB_OK | MB_ICONWARNING);
                } else {
                    self.cfg.show_tray_icon = checked(h(self.tray_checkbox));
                }
            }
            self.update_tray();
            self.localize();
            let _ = config::save(&self.cfg);
        }

        unsafe fn append_event(&mut self,detail:String){
            let mut t:SYSTEMTIME=std::mem::zeroed();
            GetLocalTime(&mut t);
            let line=format!("[{:02}:{:02}:{:02}] {}",t.wHour,t.wMinute,t.wSecond,detail);
            self.log_lines.push_back(line.clone());
            while self.log_lines.len()>160 { self.log_lines.pop_front(); }
            let display=self.log_lines.iter().cloned().collect::<Vec<_>>().join("\r\n");
            set_text(h(self.log_area),&display);
            SendMessageW(h(self.log_area),EM_SETSEL,display.encode_utf16().count(),-1);
            SendMessageW(h(self.log_area),EM_SCROLLCARET,0,0);
            if !self.cfg.log_to_file { return; }
            let file=config::app_dir().join("logs").join("current.log");
            if let Some(parent)=file.parent(){
                if fs::create_dir_all(parent).is_ok(){
                    if fs::metadata(&file).is_ok_and(|m|m.len()>2*1024*1024){
                        let _=fs::rename(&file,file.with_extension("old.log"));
                    }
                    if let Ok(mut handle)=OpenOptions::new().create(true).append(true).open(&file){
                        let _=writeln!(handle,"{line}");
                    }
                }
            }
        }

        unsafe fn selected_vdesk(&self)->Option<String>{
            self.vdesks.get(selected(h(self.vdesk_picker))).cloned()
        }
        unsafe fn set_virtual_source(&mut self){
            if let Some(id)=self.selected_vdesk(){
                let source=SOURCES[selected(h(self.vdesk_source)).min(SOURCES.len()-1)];
                if self.cfg.virtual_desktop_sources.get(&id).is_some_and(|old|old==source) {return;}
                self.cfg.virtual_desktop_sources.insert(id.clone(),source.into());
                self.append_event(format!("虚拟桌面 {} 的默认卫星：{}",id,source));
                self.sync_monitor_source();
                let _=config::save(&self.cfg);
                self.append_event("该桌面的卫星源将在下一轮定时更新时应用。".into());
            }
        }
        unsafe fn sync_virtual_source(&mut self){
            if let Some(id)=self.selected_vdesk(){
                let source=self.cfg.virtual_desktop_sources.get(&id).unwrap_or(&self.cfg.image_source);
                let position=SOURCES.iter().position(|x|*x==source).unwrap_or(0);
                SendMessageW(h(self.vdesk_source),CB_SETCURSEL,position,0);
                self.sync_monitor_source();
            }
        }
        unsafe fn report_virtual_desktops(&mut self){
            self.append_event(format!("设置：虚拟桌面模式={}；物理多显示器模式={}；后台更新={}",
                self.cfg.virtual_desktops_enabled,self.cfg.per_monitor_enabled,self.running));
            self.append_event("正在独立进程中检测 Windows 11 专用壁纸接口…".into());
            let hwnd=self.parent;
            std::thread::spawn(move||{
                let detail=match crate::virtual_wallpaper::read_only_probe(){
                    Ok(json) if !json.is_empty()=>format!("虚拟桌面专用 COM 检测：{json}"),
                    Ok(_)=>"虚拟桌面专用 COM 检测：未获得输出，请在 Windows 11 上实测。".into(),
                    Err(e)=>format!("虚拟桌面专用 COM 检测失败：{e}"),
                };
                let ptr=Box::into_raw(Box::new(detail));
                unsafe {
                    if PostMessageW(h(hwnd),REFRESH_PROGRESS,0,ptr as isize)==0 {
                        drop(Box::from_raw(ptr));
                    }
                }
            });
            match crate::virtual_desktop::snapshot(){
                Ok(snap)=>{
                    self.append_event(format!("检测结果：{} 个虚拟桌面；当前 GUID={}；来源={}",
                        snap.ids.len(),snap.current.as_deref().unwrap_or("未知"),snap.source));
                    for (i,id) in snap.ids.iter().enumerate(){
                        self.append_event(format!("Desktop {}: {}",i+1,id));
                    }
                    for line in snap.diagnostic {self.append_event(format!("注册表诊断：{line}"));}
                    if snap.current.is_none(){
                        self.append_event("无法识别当前桌面：请切换一次桌面，再点“检测虚拟桌面”。".into());
                    }
                },
                Err(e)=>self.append_event(format!("虚拟桌面识别错误：{e}")),
            }
        }
        unsafe fn schedule_offline_apply(&mut self){
            if !(self.cfg.virtual_desktops_enabled && self.cfg.per_monitor_enabled) || !self.running {
                return;
            }
            let Some(id)=self.active_vdesk.clone() else{return;};
            let cfg=self.cfg.clone();
            let generation=crate::display_apply::next_generation();
            let hwnd=self.parent;
            self.append_event(format!("虚拟桌面切换/启动：{}；只重新应用已有显示器图片，不下载。",id));
            std::thread::spawn(move||{
                let info=match crate::display_apply::reapply(&id,&cfg,generation){
                    Ok((count,missing))=>format!("桌面 {}：已重新应用 {} 台显示器的壁纸，{} 台尚无缓存（等待后台定时生成）。",
                        id,count,missing),
                    Err(e) if e.contains("Superseded")||e.contains("Another virtual desktop")=>
                        format!("跳过已经过期的桌面切换任务：{id}"),
                    Err(e)=>format!("桌面 {id}：显示器壁纸重新应用失败：{e}"),
                };
                let ptr=Box::into_raw(Box::new(info));
                unsafe{
                    if PostMessageW(h(hwnd),REFRESH_PROGRESS,0,ptr as isize)==0{
                        drop(Box::from_raw(ptr));
                    }
                }
            });
        }
        unsafe fn poll_virtual_desktops(&mut self){
            if !self.cfg.virtual_desktops_enabled{return;}
            self.vdesk_heartbeat=Instant::now();
            match crate::virtual_desktop::snapshot(){
                Ok(snap)=>{
                    if self.vdesks!=snap.ids{
                        self.vdesks=snap.ids;
                        SendMessageW(h(self.vdesk_picker),CB_RESETCONTENT,0,0);
                        for (i,_) in self.vdesks.iter().enumerate(){
                            SendMessageW(h(self.vdesk_picker),CB_ADDSTRING,0,
                                w(&format!("Desktop {}",i+1)).as_ptr() as LPARAM);
                        }
                        self.append_event(format!("虚拟桌面列表已更新：{} 个（下次定时周期自动覆盖全部）。",self.vdesks.len()));
                    }
                    if self.active_vdesk!=snap.current{
                        // Explorer's registry desktop GUID can be briefly stale.
                        // Confirm twice before changing a physical monitor's wallpaper.
                        let combined=self.cfg.per_monitor_enabled;
                        if combined{
                            if self.vdesk_candidate==snap.current{
                                self.vdesk_candidate_count=self.vdesk_candidate_count.saturating_add(1);
                            }else{
                                self.vdesk_candidate=snap.current.clone();
                                self.vdesk_candidate_count=1;
                            }
                        }
                        if !combined || self.vdesk_candidate_count>=2{
                            self.active_vdesk=snap.current;
                            self.vdesk_candidate=None;
                            self.vdesk_candidate_count=0;
                            self.append_event(format!("当前前台桌面：{}；{}。",
                                self.active_vdesk.as_deref().unwrap_or("未知"),
                                if combined{"仅重新应用对应显示器的已缓存图片"}else{"切换不触发壁纸刷新"}));
                            if let Some(id)=self.active_vdesk.as_ref(){
                                if let Some(index)=self.vdesks.iter().position(|v|v==id){
                                    SendMessageW(h(self.vdesk_picker),CB_SETCURSEL,index,0);
                                    self.sync_virtual_source();
                                }
                                if combined{self.schedule_offline_apply();}
                            }else if combined{crate::display_apply::invalidate();}
                        }
                    }else{
                        self.vdesk_candidate=None;
                        self.vdesk_candidate_count=0;
                    }
                    self.vdesk_error_reported=false;
                }
                Err(e)=>{
                    if !self.vdesk_error_reported{
                        self.append_event(format!("桌面状态诊断：{e}；后台周期仍会重新检查所有桌面。"));
                        self.vdesk_error_reported=true;
                    }
                }
            }
        }
        unsafe fn save_monitor_source(&mut self){
            let index=selected(h(self.monitor_picker));
            if let Some(monitor)=self.monitors.get(index){
                let source=SOURCES[selected(h(self.monitor_source)).min(SOURCES.len()-1)];
                if self.cfg.virtual_desktops_enabled {
                    if let Some(id)=self.selected_vdesk(){
                        self.cfg.virtual_monitor_sources.entry(id).or_default()
                            .insert(monitor.id.clone(),source.to_string());
                    }
                }else{
                    self.cfg.monitor_sources.insert(monitor.id.clone(),source.to_string());
                }
                self.append_event(format!("显示器 {}（{}×{}）：{}",index+1,monitor.width,monitor.height,source));
                self.append_event("配置已保存；切换桌面不会立即下载。".into());
                let _=config::save(&self.cfg);
            }
        }
        unsafe fn sync_monitor_source(&mut self){
            let index=selected(h(self.monitor_picker));
            if let Some(monitor)=self.monitors.get(index){
                let id=self.selected_vdesk();
                let src=if self.cfg.virtual_desktops_enabled {
                    id.as_ref().and_then(|v|self.cfg.virtual_monitor_sources.get(v))
                        .and_then(|map|map.get(&monitor.id))
                        .or_else(||id.as_ref().and_then(|v|self.cfg.virtual_desktop_sources.get(v)))
                        .or_else(||self.cfg.monitor_sources.get(&monitor.id))
                        .unwrap_or(&self.cfg.image_source)
                }else{
                    self.cfg.monitor_sources.get(&monitor.id).unwrap_or(&self.cfg.image_source)
                };
                let pos=SOURCES.iter().position(|x|*x==src.as_str()).unwrap_or(0);
                SendMessageW(h(self.monitor_source),CB_SETCURSEL,pos,0);
            }
        }
        unsafe fn spawn_job(&mut self){
            if !self.running || self.busy{return;}

            self.busy=true;
            if self.wake.pending(){self.wake.start_catchup();}
            self.cancel=Arc::new(AtomicBool::new(false));
            if self.cfg.virtual_desktops_enabled{
                self.append_event("后台定时周期开始：依次处理全部虚拟桌面，不受桌面切换影响。".into());
            } else {
                self.append_event(format!("开始更新默认卫星：{}",self.cfg.image_source));
            }
            set_text(h(self.status),lang_text(self.cfg.language_index(),16));
            let cfg=self.cfg.clone();
            let cancel=self.cancel.clone();
            let hwnd=self.parent;
            std::thread::spawn(move||{
                let outcome=std::panic::catch_unwind(||->Result<std::path::PathBuf,String>{
                    let mut report=|line:String|{
                        let ptr=Box::into_raw(Box::new(line));
                        if PostMessageW(h(hwnd),REFRESH_PROGRESS,0,ptr as isize)==0{
                            drop(Box::from_raw(ptr));
                        }
                    };
                    if cfg.virtual_desktops_enabled && cfg.per_monitor_enabled {
                        // Every pair produces exactly one fixed BMP. We do NOT call
                        // SetWallpaper for inactive virtual desktops.
                        let desktops=crate::virtual_desktop::snapshot()?.ids;
                        let monitors=crate::monitor::connected()?;
                        let monitor_ids=monitors.iter().map(|m|m.id.clone()).collect::<Vec<_>>();
                        let plan=crate::virtual_cycle::pair_plan(&cfg,&desktops,&monitor_ids)?;
                        let folder=if cfg.save_path.trim().is_empty(){
                            config::app_dir().join("wallpapers")
                        }else{std::path::PathBuf::from(&cfg.save_path)};
                        report(format!("双模式：{} 个虚拟桌面 × {} 台显示器，共 {} 张独立壁纸文件；后台按周期更新。",
                            desktops.len(),monitors.len(),plan.len()));
                        let mut success=0usize;
                        let mut failed=Vec::new();
                        let mut last=std::path::PathBuf::new();
                        for (i,pair) in plan.iter().enumerate(){
                            if cancel.load(Ordering::Relaxed){return Err("Cancelled".into());}
                            let m=monitors.iter().find(|m|m.id==pair.monitor_id)
                                .ok_or("Physical monitor disconnected")?;
                            report(format!("组合 {}/{}：桌面={} / 显示器={}×{} / 图源={}",
                                i+1,plan.len(),pair.desktop_id,m.width,m.height,pair.source));
                            let path=if let Some(fresh)=crate::virtual_cache::recent(
                                    &folder,&pair.desktop_id,Some(m),&pair.source,cfg.interval_minutes){
                                report(format!("沿用现有组合文件，无须下载：{}",fresh.display()));
                                Ok(fresh)
                            }else{
                                let mut one=cfg.clone();
                                one.image_source=pair.source.clone();
                                crate::engine::render_pair(&one,&pair.desktop_id,m,&cancel,&mut report)
                            };
                            match path{
                                Ok(path)=>{
                                    success+=1;
                                    last=path.clone();
                                    // Only the presently active virtual desktop may touch
                                    // physical wallpapers. Switching also independently
                                    // reapplies its pair files without downloading.
                                    match crate::display_apply::apply_if_current(&pair.desktop_id,m,&path){
                                        Ok(true)=>report(format!("当前桌面：已将图片应用到显示器 {}。",i+1)),
                                        Ok(false)=>{},
                                        Err(e)=>report(format!("组合图已保存，但显示器应用失败：{e}")),
                                    }
                                },
                                Err(e)=>{
                                    if cancel.load(Ordering::Relaxed){return Err("Cancelled".into());}
                                    report(format!("组合 {} 生成失败：{e}；继续下一个。",i+1));
                                    failed.push(format!("组合{}: {e}",i+1));
                                }
                            }
                        }
                        if failed.is_empty(){
                            report(format!("双模式周期完成：{} / {} 张独立壁纸。",success,plan.len()));
                            Ok(last)
                        }else{
                            Err(format!("本轮完成 {success}/{} 张；失败：{}",plan.len(),failed.join("; ")))
                        }
                    }else if cfg.virtual_desktops_enabled{
                        // Probe the private ABI before downloading possibly huge satellite data.
                        let capability=crate::virtual_wallpaper::read_only_probe()
                            .map_err(|e|format!("VIRTUAL_DESKTOP_UNSUPPORTED: {e}"))?;
                        let probe:serde_json::Value=serde_json::from_str(&capability)
                            .map_err(|e|format!("VIRTUAL_DESKTOP_UNSUPPORTED: COM capability probe returned invalid JSON: {e}"))?;
                        if probe.get("available")!=Some(&serde_json::Value::Bool(true)){
                            return Err(format!("VIRTUAL_DESKTOP_UNSUPPORTED: Windows virtual desktop COM unavailable after system update: {}",
                                probe.get("error").unwrap_or(&serde_json::Value::Null)));
                        }
                        // Do not depend on CurrentVirtualDesktop. Snapshot the GUID list
                        // and target every virtual desktop, including inactive ones.
                        let ids=crate::virtual_desktop::snapshot()?.ids;
                        let plan=crate::virtual_cycle::plan(&cfg,&ids)?;
                        // One stable wallpaper file per virtual desktop + attached monitor
                        // in the image folder selected by the user.
                        let attached=crate::monitor::connected().unwrap_or_default();
                        let only_display=if attached.len()==1{attached.first()}else{None};
                        let folder=if cfg.save_path.trim().is_empty(){
                            config::app_dir().join("wallpapers")
                        }else{std::path::PathBuf::from(&cfg.save_path)};
                        report(format!("本轮后台更新：{} 个虚拟桌面；固定壁纸文件夹：{}。",plan.len(),folder.display()));
                        let mut completed=0usize;
                        let mut last=std::path::PathBuf::new();
                        let mut failures=Vec::new();
                        for (index,work) in plan.iter().enumerate(){
                            if cancel.load(Ordering::Relaxed){return Err("Cancelled".into());}
                            report(format!("正在处理虚拟桌面 {}/{}：{} / 卫星 {}",
                                index+1,plan.len(),work.id,work.source));
                            let mut one=cfg.clone();
                            one.image_source=work.source.clone();
                            let action=if crate::virtual_cache::recent(&folder,&work.id,only_display,&work.source,cfg.interval_minutes).is_some(){
                                match crate::virtual_cache::restore(&folder,&work.id,only_display,&work.source,&cancel){
                                    Ok(path)=>{
                                        report(format!("桌面 {} 已通过 COM 恢复新鲜缓存，无需下载。",index+1));
                                        Ok(path)
                                    },
                                    Err(e)=>{
                                        report(format!("桌面 {} 缓存恢复失败：{e}；重新获取图像。",index+1));
                                        crate::engine::run_once_in_desktop(&one,Some(&work.id),&cancel,&mut report)
                                    }
                                }
                            }else{
                                crate::engine::run_once_in_desktop(&one,Some(&work.id),&cancel,&mut report)
                            };
                            match action{
                                Ok(path)=>{completed+=1;last=path;report(format!("桌面 {} 更新成功。",index+1));},
                                Err(e)=>{
                                    if cancel.load(Ordering::Relaxed){return Err("Cancelled".into());}
                                    if e.contains("VIRTUAL_DESKTOP_UNSUPPORTED"){
                                        return Err(e);
                                    }
                                    report(format!("桌面 {} 更新失败：{e}；继续其他桌面。",index+1));
                                    failures.push(format!("桌面 {}: {e}",index+1));
                                }
                            }
                        }
                        if failures.is_empty(){
                            report(format!("本轮后台更新完成：{completed}/{} 个桌面。",plan.len()));
                            Ok(last)
                        }else{
                            Err(format!("本轮完成 {completed}/{} 个桌面；失败：{}",plan.len(),failures.join("; ")))
                        }
                    }else if cfg.per_monitor_enabled{
                        let monitors=crate::monitor::connected()?;
                        if monitors.is_empty(){return Err("No connected displays available".into());}
                        let mut last=std::path::PathBuf::new();
                        for (i,m) in monitors.iter().enumerate(){
                            if cancel.load(Ordering::Relaxed){return Err("Cancelled".into());}
                            let mut one=cfg.clone();
                            one.image_source=cfg.monitor_sources.get(&m.id)
                                .cloned().unwrap_or_else(||cfg.image_source.clone());
                            report(format!("物理显示器 {} [{}x{}]: {}",i+1,m.width,m.height,one.image_source));
                            last=crate::engine::run_once_for(&one,Some(m),&cancel,&mut report)?;
                        }
                        Ok(last)
                    }else{
                        crate::engine::run_once(&cfg,&cancel,&mut report)
                    }
                });
                let result=outcome.unwrap_or_else(|_|Err("Worker unexpectedly panicked".into()));
                let success=result.is_ok();
                let report=Box::new(match result {
                    Ok(path)=>format!("Updated wallpaper: {}",path.display()),
                    Err(e)=>format!("Wallpaper update failed: {e}"),
                });
                let ptr=Box::into_raw(report);
                if PostMessageW(h(hwnd),REFRESH_DONE,success as usize,ptr as isize)==0{
                    drop(Box::from_raw(ptr));
                }
            });
        }
        unsafe fn completed(&mut self,success:bool,notice:String){
            self.busy=false;
            let cancelled=notice.contains("Cancelled")||notice.contains("cancelled");
            if success{self.failures=0;}else if !cancelled{self.failures=self.failures.saturating_add(1);}
            let incompatible=notice.contains("VIRTUAL_DESKTOP_UNSUPPORTED");
            let seconds=if !success&&!incompatible&&self.failures<=3{60}else{self.cfg.interval_minutes as u64*60};
            self.next_due=if self.wake.pending() && self.running {
                // The in-flight pre-sleep job finished: run one deferred cycle, not N missed cycles.
                Instant::now()+crate::scheduler::WAKE_SETTLE
            }else{Instant::now()+Duration::from_secs(seconds)};
            self.append_event(if success{format!("成功：{notice}")}else{format!("失败：{notice}")});
            if self.running{
                self.append_event(format!("下次自动更新：约 {} 分钟后（失败时可能提前重试）",seconds/60));
            }
            set_text(h(self.status), &notice);
        }
        unsafe fn destroy(&mut self) {
            self.running=false;
            self.cancel.store(true,Ordering::Relaxed);
            self.append_event("退出程序；停止后续自动更新。".into());
            if self.tray_added {
                Shell_NotifyIconW(NIM_DELETE, &self.tray_data());
                self.tray_added = false;
            }
            if self.restore_hotkey { UnregisterHotKey(h(self.parent), HOTKEY_ID); }
            let _ = config::save(&self.cfg);
        }
    }

    unsafe fn show_main(hwnd: HWND) {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
    unsafe fn popup(hwnd: HWND) {
        let menu = CreatePopupMenu();
        if menu.is_null() { return; }
        let lang = UI.get().map(|v| v.lock().unwrap().cfg.language_index()).unwrap_or(0);
        AppendMenuW(menu, MF_STRING, ID_SHOW as usize, w(lang_text(lang,12)).as_ptr());
        AppendMenuW(menu, MF_STRING, ID_TRAY_EXIT as usize, w(lang_text(lang,8)).as_ptr());
        let mut point: POINT = std::mem::zeroed();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, point.x, point.y, 0, hwnd, null());
        DestroyMenu(menu);
    }
    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_CREATE => {
                let ui = Ui::new(hwnd);
                let _ = UI.set(Mutex::new(ui));
                SetTimer(hwnd,TIMER_ID,1000,None);
                if let Some(state)=UI.get(){state.lock().unwrap().spawn_job();}
                0
            },
            WM_COMMAND => {
                let id = (wp & 0xffff) as u16;
                let notification=((wp>>16)&0xffff) as u16;
                match id {
                    ID_EXIT | ID_TRAY_EXIT => {
                        if let Some(state)=UI.get(){state.lock().unwrap().cancel.store(true,Ordering::Relaxed);}
                        DestroyWindow(hwnd);
                    },
                    ID_VDESK_PROBE=>{
                        if let Some(state)=UI.get(){state.lock().unwrap().report_virtual_desktops();}
                    },
                    ID_VDESK_PICKER if notification==1=>{
                        if let Some(state)=UI.get(){state.lock().unwrap().sync_virtual_source();}
                    },
                    ID_VDESK_SOURCE if notification==1=>{
                        if let Some(state)=UI.get(){state.lock().unwrap().set_virtual_source();}
                    },
                    ID_MONITOR_PICKER if notification==1=>{
                        if let Some(state)=UI.get(){state.lock().unwrap().sync_monitor_source();}
                    },
                    ID_MONITOR_SOURCE if notification==1=>{
                        if let Some(state)=UI.get(){state.lock().unwrap().save_monitor_source();}
                    },
                    ID_CLEAR_LOG=>{
                        if let Some(state)=UI.get(){let mut ui=state.lock().unwrap();ui.log_lines.clear();set_text(h(ui.log_area),"");}
                    },
                    ID_SHOW => { show_main(hwnd); },
                    ID_START => {
                        if let Some(state)=UI.get(){
                            let mut ui=state.lock().unwrap();
                            ui.save_changes();
                            let status=format!("更新前确认：虚拟桌面模式={}，多显示器模式={}，虚拟桌面数={}，当前={}。",
                                ui.cfg.virtual_desktops_enabled,ui.cfg.per_monitor_enabled,
                                ui.vdesks.len(),ui.active_vdesk.as_deref().unwrap_or("未知"));
                            ui.append_event(status);
                            if !ui.cfg.virtual_desktops_enabled && ui.vdesks.len()>1 {
                                ui.append_event("提示：虚拟桌面模式当前关闭，壁纸只能按全局/物理显示器更新。请勾选“虚拟桌面独立壁纸”。".into());
                            }
                            ui.running=!ui.running;
                            set_text(h(ui.start), lang_text(ui.cfg.language_index(),if ui.running{15}else{7}));
                            if ui.running{
                                ui.next_due=Instant::now();
                                ui.append_event("用户启动自动更新。".into());
                                ui.schedule_offline_apply();
                                ui.spawn_job();
                            }
                            else{
                                ui.cancel.store(true,Ordering::Relaxed);
                                ui.append_event("用户已停止自动更新；当前任务将尽快取消。".into());
                                set_text(h(ui.status),"已暂停自动更新");
                            }
                        }
                    },
                    ID_SOURCE | ID_SCALE | ID_LANGUAGE | ID_INTERVAL | ID_PATH | ID_AUTOSTART | ID_TRAY_CHECK | ID_WATERMARK | ID_FILE_LOG | ID_MULTI_MONITOR | ID_VDESK_ENABLED => {
                        if let Some(state) = UI.get() {
                            let mut state = state.lock().unwrap();
                            state.save_changes();
                            state.sync_monitor_source();
                            state.poll_virtual_desktops();
                        }
                    },
                    _ => {},
                }
                0
            },
            WM_POWERBROADCAST => {
                // WinUser.h PBT_APMSUSPEND=0x4, PBT_APMRESUMEAUTOMATIC=0x12,
                // PBT_APMRESUMESUSPEND=0x7. The two resume messages may both arrive.
                if let Some(state)=UI.get(){
                    let mut ui=state.lock().unwrap();
                    match wp {
                        0x4=>{
                            if ui.wake.suspend(){
                                ui.cancel.store(true,Ordering::Relaxed);
                                ui.append_event("系统准备休眠：已请求取消当前下载，暂停定时调度。".into());
                            }
                        },
                        0x12|0x7=>{
                            if ui.wake.resume(Instant::now()){
                                ui.next_due=Instant::now()+crate::scheduler::WAKE_SETTLE;
                                ui.append_event("电脑已从休眠/睡眠恢复：等待 20 秒后最多执行一次后台更新。".into());
                            }
                        },
                        _=>{}
                    }
                }
                1
            },
            WM_TIMER if wp==TIMER_ID => {
                if let Some(state)=UI.get(){
                    let mut ui=state.lock().unwrap();
                    // Desktop switching does not schedule or cancel refreshes.
                    if ui.running && ui.cfg.virtual_desktops_enabled &&
                        ui.vdesk_heartbeat.elapsed()>=if ui.cfg.per_monitor_enabled{
                            Duration::from_secs(1)
                        }else{Duration::from_secs(30)}{
                        ui.poll_virtual_desktops();
                    }
                    if ui.running&&!ui.busy&&!ui.wake.sleeping()&&Instant::now()>=ui.next_due{ui.spawn_job();}
                    if ui.last_maintenance.elapsed()>Duration::from_secs(3600){
                        ui.last_maintenance=Instant::now();
                        let cache=crate::maintenance::prune_virtual(&config::app_dir().join("virtual-cache"));
                        if cache.removed>0{
                            ui.append_event(format!("自动清理缓存：移除 {} 个旧文件，释放 {:.1} MiB；保留 {} 个。",
                                cache.removed,cache.freed as f64/1048576.0,cache.kept));
                        }
                        // Clean only our own incomplete image downloads, never arbitrary user files.
                        let folder=if ui.cfg.save_path.trim().is_empty(){config::app_dir().join("wallpapers")}
                            else{std::path::PathBuf::from(&ui.cfg.save_path)};
                        let scratch=crate::maintenance::prune_scratch(&folder);
                        if scratch.removed>0{
                            ui.append_event(format!("已清理 {} 个过期的下载临时文件。",scratch.removed));
                        }
                        if let Ok(desktops)=crate::virtual_desktop::snapshot(){
                            let orphans=crate::maintenance::prune_orphan_profiles(&folder,&desktops.ids);
                            if orphans.removed>0 {
                                ui.append_event(format!("已清理 {} 张超过14天的已删除虚拟桌面壁纸，释放 {:.1} MiB。",
                                    orphans.removed,orphans.freed as f64/1048576.0));
                            }
                        }
                    }
                }
                0
            },
            REFRESH_PROGRESS => {
                let ptr=lp as *mut String;
                if !ptr.is_null(){
                    let message=*Box::from_raw(ptr);
                    if let Some(state)=UI.get(){state.lock().unwrap().append_event(message);}
                }
                0
            },
            REFRESH_DONE => {
                let ptr=lp as *mut String;
                if !ptr.is_null(){
                    let report=*Box::from_raw(ptr);
                    if let Some(state)=UI.get(){state.lock().unwrap().completed(wp!=0,report);}
                }
                0
            },
            WM_CLOSE => {
                let lang=UI.get().map(|v|v.lock().unwrap().cfg.language_index()).unwrap_or(0);
                let choice=MessageBoxW(hwnd,w(lang_text(lang,19)).as_ptr(),
                    w(lang_text(lang,20)).as_ptr(),MB_YESNOCANCEL|MB_ICONQUESTION);
                if choice==IDYES {
                    if let Some(state)=UI.get(){
                        state.lock().unwrap().append_event("已隐藏窗口；后台定时任务保持原状态。".into());
                    }
                    ShowWindow(hwnd,SW_HIDE);
                }else if choice==IDNO{
                    if let Some(state)=UI.get(){state.lock().unwrap().cancel.store(true,Ordering::Relaxed);}
                    DestroyWindow(hwnd);
                }
                0
            },
            WM_HOTKEY if wp as i32 == HOTKEY_ID => { show_main(hwnd); 0 },
            TRAY_MESSAGE => {
                match lp as u32 {
                    WM_LBUTTONDBLCLK => show_main(hwnd),
                    WM_RBUTTONUP | WM_CONTEXTMENU => popup(hwnd),
                    _ => {},
                }
                0
            },
            WM_DESTROY => {
                KillTimer(hwnd,TIMER_ID);
                if let Some(state) = UI.get() { state.lock().unwrap().destroy(); }
                PostQuitMessage(0);
                0
            },
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
    pub fn run() {
        unsafe {
            SetProcessDPIAware();
            let cls = w("CurrentEarthWallpaperNativeWindow");
            let h_instance = GetModuleHandleW(null());
            let wnd = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_instance,
                hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: (5usize + 1) as _,
                lpszMenuName: null(),
                lpszClassName: cls.as_ptr(),
            };
            if RegisterClassW(&wnd) == 0 { return; }
            let hwnd = CreateWindowExW(
                0, cls.as_ptr(), w("Current Earth Wallpaper").as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
                CW_USEDEFAULT, CW_USEDEFAULT, 850, 775,
                null_mut(), null_mut(), h_instance, null(),
            );
            if hwnd.is_null() { return; }
            ShowWindow(hwnd, if std::env::args().any(|a|a=="--autostart"){SW_HIDE}else{SW_SHOW});
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(windows)]
fn live_goes_probe(){
    let mut results=Vec::new();
    let mut failures=0u32;
    for sat in ["GOES-East","GOES-West"] {
        let url=sources::goes_cdn_urls(sat).unwrap()[1].clone();
        let tmp=std::env::temp_dir().join(format!("cew-probe-{}-{}.jpg",std::process::id(),sat));
        let start=std::time::Instant::now();
        let result=http::download(&url,&tmp,12*1048576).and_then(|_|{
            let decoded=imaging::load_scaled(&tmp,300,None)?;
            Ok(format!("decoded {}x{}",decoded.width,decoded.height))
        });
        let _=std::fs::remove_file(tmp);
        if result.is_err(){failures+=1;}
        results.push(serde_json::json!({
            "source":sat, "url":url, "ok":result.is_ok(),
            "result":result.unwrap_or_else(|e|e),
            "elapsed_seconds":start.elapsed().as_secs_f32(),
        }));
    }
    let path=std::env::current_exe().unwrap().parent().unwrap().join("live-goes.json");
    let _=std::fs::write(path,serde_json::to_vec_pretty(&results).unwrap());
    if failures!=0 {std::process::exit(2);}
}

#[cfg(windows)]
fn virtual_desktop_probe(){
    let report=crate::virtual_desktop::diagnostic_report();
    /* prior report builder removed below */
    let _legacy_report=match virtual_desktop::snapshot(){
        Ok(v)=>serde_json::json!({
            "available":true,
            "count":v.ids.len(),
            "current":v.current,
            "desktop_ids":v.ids,
            "method":"read-only Explorer VirtualDesktops registry",
            "note":"On CI without multiple desktops current may be absent; no state is changed."
        }),
        Err(e)=>serde_json::json!({
            "available":false,
            "error":e,
            "note":"No registry changes made, safe fallback."
        })
    };
    if let Ok(path)=std::env::current_exe(){
        if let Some(dir)=path.parent(){
            let _=std::fs::write(dir.join("virtual-desktop-probe.json"),
                serde_json::to_vec_pretty(&report).unwrap());
        }
    }
}

#[cfg(windows)]
fn main() {
    let args:Vec<String>=std::env::args().collect();
    if args.get(1).is_some_and(|s|s=="--vd-native-set"){
        let result=match (args.get(2),args.get(3)){
            (Some(id),Some(path))=>virtual_wallpaper::apply_child(id,std::path::Path::new(path)),
            _=>Err("Helper requires desktop GUID and BMP path".into()),
        };
        if let Err(e)=result{eprintln!("{e}");std::process::exit(5);}
        return;
    }
    if args.get(1).is_some_and(|s|s=="--vd-native-probe"){
        println!("{}",virtual_wallpaper::probe_child());
        return;
    }
    if std::env::args().any(|arg|arg=="--virtual-desktop-probe") {virtual_desktop_probe();return;}
    if std::env::args().any(|arg|arg=="--self-test-goes") {live_goes_probe();return;}
    if std::env::args().any(|arg|arg=="--self-test-render"){
        let p=std::env::temp_dir().join(format!("cew_test_{}.bmp",std::process::id()));
        let out=std::env::temp_dir().join(format!("cew_render_{}.bmp",std::process::id()));
        wallpaper::write_test_pattern(&p,4096).expect("write 4K BMP image");
        let image=imaging::load_scaled(&p,1200,None).expect("WIC 4K downsample");
        wallpaper::compose(&out,3840,2160,&image,true).expect("compose UHD wallpaper");
        assert!(std::fs::metadata(&out).unwrap().len()>3840*2160*4);
        metrics::write_render_report().expect("save memory report");
        let _=std::fs::remove_file(p);
        let _=std::fs::remove_file(out);
        return;
    }
    winapp::run();
}
