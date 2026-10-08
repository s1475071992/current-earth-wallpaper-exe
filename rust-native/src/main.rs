#![cfg_attr(windows, windows_subsystem = "windows")]
//! First migration milestone: small, native Win32 configuration window.
//! Image refresh intentionally disabled until the native image engine is implemented.

mod config;
mod sources;

#[cfg(not(windows))]
fn main() {
    eprintln!("This GUI runs on Windows. Cargo unit tests can run on any platform.");
}

#[cfg(windows)]
mod winapp {
    use super::config::{self, AppConfig, LANGUAGES, SCALES, SOURCES};
    use std::sync::{Mutex, OnceLock};
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
    const ID_SHOW: u16 = 201;
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
        const DICT: [[&str; 13]; 4] = [
            ["实时地球壁纸 · Rust 原生预览", "卫星图源", "壁纸大小", "界面语言", "更新间隔（分钟）", "显示托盘图标", "显示时间水印", "开始更新", "退出程序", "配置已保存。壁纸下载与渲染引擎正在迁移。", "原生引擎尚未完成，请勿替代正式版。", "无法隐藏托盘：Ctrl+Alt+E 已被其他软件占用。", "显示主窗口"],
            ["Current Earth Wallpaper · Rust Native Preview", "Satellite source", "Wallpaper size", "Interface language", "Update interval (minutes)", "Show tray icon", "Time watermark", "Start updating", "Exit app", "Settings saved. Native image engine is being ported.", "Native image engine isn't ready yet. Keep using the stable build.", "Cannot hide tray: Ctrl+Alt+E is in use.", "Show window"],
            ["リアルタイム地球壁紙 · Rust ネイティブ", "衛星ソース", "壁紙の大きさ", "表示言語", "更新間隔（分）", "トレイアイコンを表示", "時刻の透かし", "更新開始", "終了", "設定を保存しました。画像エンジンは移植中です。", "画像エンジンはまだ未完成です。", "トレイを隠せません。Ctrl+Alt+E は使用中です。", "ウィンドウを表示"],
            ["실시간 지구 배경화면 · Rust 네이티브", "위성 소스", "배경화면 크기", "인터페이스 언어", "갱신 간격(분)", "트레이 아이콘 표시", "시간 워터마크", "업데이트 시작", "종료", "설정 저장됨. 이미지 엔진을 이식하는 중입니다.", "이미지 엔진이 아직 준비되지 않았습니다.", "트레이 숨기기 불가: Ctrl+Alt+E 사용 중.", "창 표시"],
        ];
        DICT[lang.min(3)][key.min(12)]
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
        icon: usize,
        tray_added: bool,
        restore_hotkey: bool,
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
            let hotkey = RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, b'E' as u32) != 0;
            if !hotkey { cfg.show_tray_icon = true; }
            let labels = [
                control(hwnd, "STATIC", "", 24, 28, 185, 24, 0, 0),
                control(hwnd, "STATIC", "", 24, 72, 185, 24, 0, 0),
                control(hwnd, "STATIC", "", 24, 116, 185, 24, 0, 0),
                control(hwnd, "STATIC", "", 24, 160, 185, 24, 0, 0),
            ];
            let combo_style = CBS_DROPDOWNLIST as u32 | WS_VSCROLL;
            let source = control(hwnd, "COMBOBOX", "", 210, 24, 230, 190, combo_style, ID_SOURCE);
            let scale = control(hwnd, "COMBOBOX", "", 210, 68, 230, 150, combo_style, ID_SCALE);
            let language = control(hwnd, "COMBOBOX", "", 210, 112, 230, 150, combo_style, ID_LANGUAGE);
            let interval = control(hwnd, "EDIT", &cfg.interval_minutes.to_string(), 210, 156, 230, 26,
                WS_BORDER | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32, ID_INTERVAL);
            choose(source, &SOURCES, SOURCES.iter().position(|x| *x == cfg.image_source).unwrap_or(0));
            choose(scale, &SCALES, SCALES.iter().position(|x| *x == cfg.scale_mode).unwrap_or(2));
            choose(language, &LANGUAGES, cfg.language_index());
            let tray_checkbox = control(hwnd, "BUTTON", "", 24, 207, 240, 26,
                BS_AUTOCHECKBOX as u32, ID_TRAY_CHECK);
            let watermark = control(hwnd, "BUTTON", "", 24, 243, 240, 26,
                BS_AUTOCHECKBOX as u32, ID_WATERMARK);
            check(tray_checkbox, cfg.show_tray_icon);
            check(watermark, cfg.watermark_on);
            let start = control(hwnd, "BUTTON", "", 24, 290, 201, 35,
                BS_PUSHBUTTON as u32, ID_START);
            let exit = control(hwnd, "BUTTON", "", 245, 290, 195, 35,
                BS_PUSHBUTTON as u32, ID_EXIT);
            let status = control(hwnd, "STATIC", "", 24, 341, 425, 55, 0, 0);
            let icon = LoadIconW(null_mut(), IDI_APPLICATION);
            let mut ui = Self {
                cfg, parent: hwnd as usize, labels: labels.map(|x| x as usize),
                source: source as usize, scale: scale as usize, language: language as usize,
                interval: interval as usize, tray_checkbox: tray_checkbox as usize,
                watermark: watermark as usize, start: start as usize, exit: exit as usize,
                status: status as usize, icon: icon as usize,
                tray_added: false, restore_hotkey: hotkey,
            };
            ui.localize();
            ui.update_tray();
            ui
        }
        unsafe fn localize(&self) {
            let l = self.cfg.language_index();
            set_text(h(self.parent), lang_text(l,0));
            for (i, hwnd) in self.labels.iter().enumerate() {
                set_text(h(*hwnd),lang_text(l,i+1));
            }
            set_text(h(self.tray_checkbox),lang_text(l,5));
            set_text(h(self.watermark),lang_text(l,6));
            set_text(h(self.start),lang_text(l,7));
            set_text(h(self.exit),lang_text(l,8));
            set_text(h(self.status),lang_text(l,9));
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
        unsafe fn destroy(&mut self) {
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
                0
            },
            WM_COMMAND => {
                let id = (wp & 0xffff) as u16;
                match id {
                    ID_EXIT | ID_TRAY_EXIT => { DestroyWindow(hwnd); },
                    ID_SHOW => { show_main(hwnd); },
                    ID_START => {
                        let lang = UI.get().map(|v| v.lock().unwrap().cfg.language_index()).unwrap_or(0);
                        MessageBoxW(hwnd, w(lang_text(lang,10)).as_ptr(), w("Rust Native Preview").as_ptr(), MB_OK | MB_ICONINFORMATION);
                    },
                    ID_SOURCE | ID_SCALE | ID_LANGUAGE | ID_INTERVAL | ID_TRAY_CHECK | ID_WATERMARK => {
                        if let Some(state) = UI.get() {
                            let mut state = state.lock().unwrap();
                            state.save_changes();
                        }
                    },
                    _ => {},
                }
                0
            },
            WM_CLOSE => { ShowWindow(hwnd, SW_HIDE); 0 },
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
                CW_USEDEFAULT, CW_USEDEFAULT, 480, 450,
                null_mut(), null_mut(), h_instance, null(),
            );
            if hwnd.is_null() { return; }
            ShowWindow(hwnd, SW_SHOW);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(windows)]
fn main() { winapp::run(); }
