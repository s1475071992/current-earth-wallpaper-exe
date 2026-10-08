# Current Earth Wallpaper — Rust + Windows native migration

**Branch**: `rust-windows-native` (independent of the Python `main` branch).

## Stage 1 — native foundation

- [x] Source registry with all six satellite providers and tested metadata/URL parsers.
- [x] JSON settings compatible with the Python version (`AppConfig` keys).
- [x] Plain Windows GUI: no Tk, Qt, Electron or WebView.
- [x] Four interface languages: 中文 / English / 日本語 / 한국어.
- [x] Tray visibility preference, `Ctrl+Alt+E` restore shortcut, and safe no-tray fallback.
- [x] GitHub Actions Windows x64 Rust release build and unit tests.
- [ ] WinHTTP download and provider-specific transport integration.
- [ ] 4×4 Himawari image tile composition.
- [ ] Native WIC/GDI+ rendering, circular Earth mask, watermark, screen scaling.
- [ ] Worker-thread refresh scheduler, bounded retries, automatic start.
- [ ] Memory benchmarks and Windows desktop integration tests.

**Important:** This first-stage EXE is an *interface/architecture preview*, not yet a working satellite wallpaper updater. The Start control deliberately reports that the image pipeline has not yet been migrated. Never replace your working version with this preview.

## Build

In a Windows x64 terminal with Rust stable:

```powershell
cd rust-native
cargo test
cargo build --release
```

The executable will be `rust-native/target/release/CurrentEarthWallpaperNative.exe`.

Settings are written to `%LOCALAPPDATA%\CurrentEarthWallpaper\wallpaper_config.json`. The previous JSON next to the EXE is used as a read-only fallback for migration.

## Resource use goals

Idle 15–35 MB / GUI open below 50 MB are **targets**, not measurements. No high-resolution frames should stay decoded between updates.

## Safety

The app does not delete OS temporary directories, does not execute untrusted shell commands, and does not require elevated rights. When hiding the tray icon, it first verifies that the global restore hotkey was successfully registered. The on-window Exit button always remains available.

## Phase 2

Build WinHTTP networking and bounded-memory wallpaper processing in separate Rust modules, retaining the Python source's six source semantics. Add on-Windows integration tests and measure working-set peaks on high-res GOES images.
