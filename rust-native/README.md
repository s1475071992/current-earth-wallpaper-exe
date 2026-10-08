# Current Earth Wallpaper (Rust + Win32 native)

This is the **Rust migration branch**. The Python version remains in `main`.

## Native application
- Six satellite providers: FY4B, GOES-East (G19), GOES-West (G18), Himawari-9 (16 NICT tiles), NASA EPIC and Meteosat MTG WMS.
- WinHTTP networking with built-in Windows certificate trust and 70 MiB per-image transfer bound.
- Native Windows Imaging Component (WIC) clipping and downsampling. The 10848-pixel GOES original is **not** fully copied to a managed RGBA buffer.
- WIC 4x4 Himawari tile assembly. Earth disk rendered on black canvas and BMP output written scanline-by-scanline (only one BMP row allocated).
- Win32 SystemParametersInfoW desktop wallpaper setting; old files pruned only inside the app-managed filename namespace.
- Periodic worker thread with three one-minute retry opportunities and configurable refresh interval.
- Win32 UI, four interface languages, tray show/hide preference and Ctrl+Alt+E window restore.
- Per-user JSON settings in `%LOCALAPPDATA%\CurrentEarthWallpaper\wallpaper_config.json`; image folder defaults to `%LOCALAPPDATA%\CurrentEarthWallpaper\wallpapers`.

## Build and tests
```powershell
cd rust-native
cargo test
cargo build --release
.\target\release\CurrentEarthWallpaperNative.exe --self-test-render
```
GitHub Actions uses Windows 2022 with automated Rust unit tests, native offline image-rendering smoke test and ten-second real process idle working-set/private-memory sampling. The recorded `memory-idle.json` is attached to the workflow run.

## Limitations to verify
- A successful CI run does not prove every remote satellite provider is reachable. API availability and providers' conditions can change.
- Background *idle* memory is measured in CI; live-image decoding and remote download *peak* memory still need real device/provider tests.
- NASA EPIC and WMS images may update more slowly than weather satellites.
- NICT visualizations are not licensed for commercial use.
- This branch is under active testing; do not overwrite the Python version until tested on your own Windows desktop.

## 日志写入开关 / File log setting

The **Save logs to file** checkbox is stored in the JSON configuration as `log_to_file` (defaults to `true` for backward compatibility). When disabled, the scrolling in-memory execution log remains visible, but no new file writes or log rotation take place. Existing log files are left untouched. The setting persists after restart, and all four UI languages have a translation.
