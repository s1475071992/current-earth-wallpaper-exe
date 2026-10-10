# Current Earth Wallpaper v1.1.1-rc.1 (Rust Native · Windows)

Standalone Windows x64 application with native Win32 UI, WinHTTP + Windows Imaging
Component (WIC). The **Python main branch is untouched**. Rust source and releases
are maintained in the `rust-windows-native` branch.

## Features

- Six satellite sources: FY4B, GOES-East, GOES-West, Himawari-9, NASA EPIC, Meteosat.
- Four UI languages, native tray icon, adjustable refresh interval, autostart and image scale.
- Optional file logging; window displays recent execution events without saving to disk.
- Per-physical-monitor satellite selection using public `IDesktopWallpaper` API.
- Experimental desktop × monitor combination mode, retaining one image per pair.
- Windows 11 virtual desktops, each with its own satellite source. Updates are scheduled
  **for every virtual desktop** in the background regardless of which is active; switching
  desktops never downloads images or resets the refresh schedule.
- A private Windows 11 COM `SetDesktopWallpaper` helper is isolated in a child process.
  It checks the exact target GUID before applying. Windows 11 26H2 build 26300.9550
  was tested successfully by the application user.
- Windows sleep/hibernate broadcasts pause downloads. Resume events (which Windows may
  send twice) schedule **at most one** catch-up refresh after a 20-second delay.
  Missed intervals are not replayed.
- **One stable BMP per virtual-desktop and monitor identity**, in the image directory
  selected in the settings window. Re-rendered images replace that same file atomically,
  then the Windows wallpaper API is called again. Sources can change without creating
  another BMP for the pair. A small .meta.json sidecar tracks the source for cache freshness.
- Automatic cleanup of legacy timestamped images and incomplete downloads; files
  from deleted virtual desktops are eligible for cleanup after 14 days.
- Undocumented Windows COM interface safety: detect unsupported builds, probe before
  network downloads, abort/disable the helper after crashes or 15-second timeouts.
  Never silently fall back to applying the virtual wallpaper globally.

## Unified wallpaper source settings

The settings window now contains **one checkbox** for desktop + monitor pair mode,
followed by exactly three selection controls:

1. **Virtual desktop** — choose the desktop to configure.
2. **Physical monitor** — choose the connected display to configure.
3. **Satellite source for this pair** — one source selector, not two separate controls.

Click **Detect displays** to re-enumerate attached monitors, see the current count
and pixel dimensions in the log, and retain the selected device when possible.
**Check desktops** separately refreshes the virtual-desktop diagnostic. Existing
per-display and per-desktop source overrides remain readable; an explicit pair
choice takes priority. Existing saved single-mode selections are migrated into
the unified checkbox mode without erasing their mappings.

With pair mode disabled the global default source still works. With pair mode
enabled, background scheduled refresh maintains a fixed BMP for every
(virtual desktop GUID, physical monitor device ID) combination. Switching
desktops only reapplies existing monitor BMPs, never downloading new images.
This Windows 11 emulation remains experimental until verified on a real
multi-monitor, multi-virtual-desktop system.

## One-process restart and replacement

The program allows **one GUI process per Windows logon session**. If you
launch the EXE again while its earlier copy is running, including when the
earlier window is hidden and the tray icon is disabled, the new instance
sends a graceful Exit command to the old window, waits for the old process
to finish, then takes ownership of a session-local Windows named mutex
and opens the new GUI. Settings are retained; no second background update
loop is allowed. If the old process is stuck or cannot be stopped within
12 seconds, the new launch reports an error instead of running two copies.

The hidden Windows COM helper subprocesses (wallpaper assignment, probing,
self-tests) **do not** acquire the GUI mutex; only normal interactive GUI
processes participate. GitHub Windows CI exercises a hidden 1→2→3 instance
replacement to check that only the newest GUI remains.

## Hot-unplug a monitor: automatic native virtual-desktop restoration

The v1.1.0 wallpaper switching backend adapts to the **number of connected
physical monitors** on every switch or scheduled background refresh:

- **Exactly one display:** assign each cached desktop+monitor BMP to its
  actual Windows 11 virtual-desktop GUID through the previously verified
  private `SetDesktopWallpaper` COM helper. This preserves the different
  desktop wallpapers after unplugging an external monitor; desktop switching
  does not need to fetch images.
- **Two or more displays:** reapply each connected physical display's BMP
  for the currently active virtual desktop via public `IDesktopWallpaper`.
  Different desktop/monitor combinations retain their separate fixed files.
- **Zero displays:** no wallpaper writes.

`WM_DISPLAYCHANGE` events are debounced for two seconds to allow the Windows
display topology to stabilize. The app refreshes the monitor list and reapplies
**existing cached images without network access**. The Detect Displays button
can also trigger the same cache reapplication manually. When a cached profile
for a newly recognized display is missing, the next normal scheduled cycle
will render it.

This is a Windows 11 compatibility mechanism: the internal per-desktop COM
API is undocumented, and multi-display virtual-desktop emulation still needs
testing with the user's actual display hardware.

## Usage

The v1.1.1-rc.1 bugfix preview is available as a Windows CI artifact. The existing v1.1.0 GitHub Release remains unchanged. No Python environment is needed.
The v1.1.0 EXE normally shuts down a previous hidden/running instance on relaunch.
When upgrading from much older releases, exit the old app if replacement is blocked.

To configure virtual desktops, create them with Win+Tab. Enable `Virtual desktop
wallpapers` and select each desktop from the list, assign its satellite source,
then click Start updating. Choose the interval in minutes. Background updates can
continue with the settings window hidden. The close-button dialog lets you hide
or actually exit. The tray menu also supports Exit. `Ctrl+Alt+E` restores the window.

Settings: `%LOCALAPPDATA%\CurrentEarthWallpaper\wallpaper_config.json`
Images: `%LOCALAPPDATA%\CurrentEarthWallpaper\wallpapers`
Legacy virtual cache (cleanup only): `%LOCALAPPDATA%\CurrentEarthWallpaper\virtual-cache`
Optional logs: `%LOCALAPPDATA%\CurrentEarthWallpaper\logs`

## Windows compatibility and limitations

The per-virtual-desktop wallpaper interface is **not publicly supported by Microsoft**
and may change in future Windows builds. Version range 26100..26399 is an initial
gate, not a guarantee. The helper is isolated, crash/timeout guarded, and fails
closed; a Windows update may require a new compatible implementation.

**Dual mode (experimental):** Enabling both virtual desktops and independent
physical monitors now gives each (virtual desktop GUID, physical monitor ID) pair
its own satellite selection and fixed BMP in the chosen wallpaper folder.
In the GUI, select a virtual desktop, then select a monitor, then choose that
monitor's satellite. The per-desktop default is used when no pair override exists.

Background refreshes render each pair one at a time without changing inactive
desktops' physical monitors. On a virtual-desktop switch, the app reads and reapplies
existing BMPs using the supported per-monitor `IDesktopWallpaper::SetWallpaper`
API. **No network request and no rendering occur on switching.**

Because Windows 11 does not officially guarantee per-monitor virtual-desktop
wallpaper independence, dual mode is a best-effort compatibility emulation: it may
briefly show old images or be overwritten by Explorer's own wallpaper restoration.
The user must keep this app running in the background for switch reapply to work.
The v1.0.0 release remains available as a rollback.

GitHub CI on Windows Server verifies unit tests, release compilation, offline WIC
rendering and helper fail-closed behavior. It does not replicate the Windows 11
desktop compositor, a physical sleep/wake cycle or multi-week runtime.

The active fixed-name BMP per desktop/monitor pair is retained. Obsolete desktop
profiles become eligible for cleanup after 14 days. A 128-pair safety limit bounds
the number of scheduled source downloads and wallpaper files.

## Build (Windows + Rust)

```powershell
cd rust-native
cargo test
cargo build --release
```

No administrative privileges or background Windows service are required.
