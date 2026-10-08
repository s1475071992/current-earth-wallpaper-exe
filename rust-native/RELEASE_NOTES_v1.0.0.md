# Current Earth Wallpaper v1.0.0

First Rust/Win32 native stable release for Windows x64, following end-user
verification of independent Windows 11 26H2 virtual-desktop satellite wallpapers.

### Highlights
- Scheduled refresh of **all** virtual desktops without any network request on switch.
- Six satellite providers, four languages, tray controls, autostart and native WIC rendering.
- Sleep/hibernate pause + deduplicated 20-second post-wake refresh.
- Automatic bounded BMP cache / old scratch-file cleanup with active wallpaper protection.
- Windows version/API capability check before virtual-desktop downloads.
- 15-second isolated COM helper watchdog and session-wide crash circuit breaker.

### Downloads
- `CurrentEarthWallpaperNative-v1.0.0-windows-x64.exe`: portable 64-bit Windows executable.
- `CurrentEarthWallpaper-v1.0.0-source.zip`: Rust source, build instructions and licenses.
- `SHA256SUMS.txt`: checksums for both files.

### Safety and compatibility
- The Python `main` branch is unchanged. Rust sources are on `rust-windows-native`.
- The virtual-desktop wallpaper COM API is undocumented, and major Windows updates
  may require an adapter update. Unknown/incompatible builds fail closed.
- Simultaneously combining independent physical monitors and independent virtual
  desktops is not yet supported.
- Unsigned EXE: Windows SmartScreen may ask for confirmation. This version has not
  undergone third-party code signing.
- GitHub CI cannot physically test sleep/resume or the Windows 11 virtual-desktop shell.
