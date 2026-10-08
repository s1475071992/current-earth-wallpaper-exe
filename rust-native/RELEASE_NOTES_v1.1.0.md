# Current Earth Wallpaper v1.1.0

**Windows 11 native Rust release (Windows x64, portable EXE, no code signing)**

This release expands v1.0.0 with independent satellite wallpaper sources for
**each virtual desktop × physical display**. Original Python `main` branch is
unchanged. The Rust sources live on `rust-windows-native`.

### Highlights

- **Unified pair configuration:** Select a virtual desktop, a physical monitor
  and its own satellite source; no duplicate source selection controls.
- **Detect displays:** Re-enumerate the number, resolution and stable device
  identifiers of connected monitors.
- **All-desktop background refresh:** Each configured desktop/monitor pair
  maintains exactly one fixed-name BMP in the chosen image directory. On
  virtual-desktop switch, only preexisting BMPs are reapplied—**no network
  download or image rendering**.
- **Hot-unplug recovery:** If one physical monitor remains, the native
  Windows 11 virtual-desktop wallpaper helper binds each desktop's image by
  GUID. If multiple monitors are attached, the per-monitor wallpaper interface
  reapplies the active desktop's cached images.
- **Single instance:** Relaunching the EXE closes a previous hidden/tray-less
  GUI and starts a fresh instance. Stale GUI instances cannot both run jobs.
- **Power and caching:** Debounced wake catch-up, monitor-resolution cache
  validation, crash/timeout protection on undocumented COM, bounded cache
  cleanup, stable wallpaper paths, and optimized image memory usage.
- **Transport improvements:** HTTPS for all six satellite providers, including
  FY4B, and one satellite download per distinct source per refresh cycle
  even if multiple desktops/monitors choose it.
- Chinese, English, Japanese and Korean UI; tray, autostart, periodic refresh,
  image size choices, timestamp watermark and optional file logging.

### Files

- `CurrentEarthWallpaperNative-v1.1.0-windows-x64.exe` — unsigned portable EXE.
- `CurrentEarthWallpaper-v1.1.0-source.zip` — complete Rust source (src,
  Cargo.toml, generated Cargo.lock, LICENSE, README, build/test workflows).
- `SHA256SUMS.txt` — SHA256 hashes for the EXE and source ZIP.

### Supported OS and limitations

The best-tested system is **Windows 11 Pro 26H2 build 26300.9550**.
Single-display desktop wallpapers use an **undocumented Windows COM interface**
in a separately guarded process. Later Windows upgrades may break this
interface. The multi-monitor × virtual-desktop behavior is a best-effort
wallpaper reapplication technique because Windows does not guarantee that
combination through its public APIs.

The CI system tests Rust unit tests, Clippy, Windows Release compilation,
offline WIC rendering, live GOES/FY4B satellite decoding, helper behavior,
hidden three-process single-instance takeover, and an idle-memory sample.
CI does **not** reproduce actual sleep/hibernate, hot-unplug, or the Windows
11 virtual desktop shell with multiple physical displays. Real-hardware
behavior can still differ.

**Unsigned:** This release is intentionally **not signed with a certificate**
at the user's request. Windows SmartScreen may show a warning. Download from
this repository's GitHub Release and verify the provided SHA256 hash.

This release does not install a service, driver, or elevated component.
The v1.0.0 Release remains available for rollback.
