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

## Physical monitors

Enable **Different satellite for each monitor**, select a monitor and its source in two dropdowns. Configuration stores monitor device path → source. Every refresh enumerates attached monitors through IDesktopWallpaper, renders the selected satellite at native display resolution, and assigns the file through IDesktopWallpaper::SetWallpaper. Processing is sequential to limit memory. Monitors without mappings use the default source. Enumeration failures are reported rather than silently writing a global wallpaper.

## Windows virtual desktops

The JSON configuration reserves virtual_desktop_sources (virtual desktop GUID → satellite source), but automatic per-virtual-desktop wallpaper assignment is NOT yet available. Microsoft's public IVirtualDesktopManager does not expose a virtual desktop wallpaper setter. Private Explorer COM methods are Windows build-dependent; simultaneous per-monitor and per-virtual-desktop wallpapers can conflict even in Windows 11. The app does not expose an inert virtual desktop toggle. Target OS version/build (winver) is required to implement and verify a compatible adapter.


## Experimental Windows 11 26H2 virtual desktops (build 26300.9550)

This opt-in feature uses **read-only Explorer registry state** to discover ordered virtual
desktop GUIDs and the active desktop. PowerToys uses a similar registry strategy, but
this is not a Microsoft-supported contract. It can break on a Windows update. No
undocumented Explorer COM interfaces are invoked and no virtual desktop registry
values are written.

1. Create at least two desktops with Win+Tab and switch once.
2. In the app enable **Virtual desktop wallpapers (26H2 compatibility)**.
3. Select "Desktop 1"/"Desktop 2" and choose its default satellite.
4. To set *both* physical monitors independently for each desktop, additionally
   enable "Different satellite for each monitor": select virtual desktop, then physical
   monitor, then satellite in that monitor's dropdown.
5. Click Start updating. Every second the app detects the active desktop. On switching,
   it cancels work for the old desktop and restores a cached BMP if available, followed
   by a fresh satellite update. If the active GUID cannot be established, it pauses
   writes instead of applying the wrong wallpaper.

Selection precedence: (desktop,monitor) override > desktop default > monitor default >
global default. The per-desktop mapping is retained on restart. Cache is bounded to
48 file entries under LOCALAPPDATA/CurrentEarthWallpaper/virtual-cache, and controls
keep the Python main branch unchanged.

**Known limitations:** Windows 11 itself may revert per-monitor wallpapers when
virtual desktops switch, and the system's wallpaper preferences may conflict with
this app's combination of per-virtual-desktop and per-monitor assignments. This uses
best-effort restoration after the switch rather than modifying private Explorer
interfaces. GitHub Windows 2022 CI cannot reproduce the user's Windows 11 26H2
desktop compositor; Windows 11 26H2 dual-monitor real-world verification is still
required before calling the mode production-ready.

Diagnostic mode: run EXE with --virtual-desktop-probe. It produces read-only
"virtual-desktop-probe.json" next to the EXE with detected IDs/current status; no
registry modifications or wallpaper changes.
