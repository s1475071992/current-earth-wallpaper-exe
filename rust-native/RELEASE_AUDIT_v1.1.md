# v1.1.0 Release Readiness / Security Audit (2026-10-08)

Status: **v1.1.0 stable release, user-authorized unsigned build**.

Scope: Rust Windows x64 Win32 client; desktop-monitor wallpaper scheduler, caches,
native COM helper, display hotplug, sleep/resume, and network image retrieval.

## Fixed during final review

1. Cache freshness now validates the BMP header, output dimensions and expected
   pixel payload length. Screen size/orientation changes no longer silently
   reuse wrong-sized BMP files until the next ordinary interval.
2. `WM_DISPLAYCHANGE` debounce now schedules one size-aware worker pass after
   hardware changes and retains the request even if another cycle is busy.
   Virtual-desktop switching itself remains completely offline.
3. When the native wallpaper assignment fails, the scheduled worker now reports
   a failed cycle rather than falsely declaring all wallpaper changes successful.
   Incompatible private COM implementations abort the attempted cycle safely.
4. The virtual-desktop diagnostics GUI now repopulates its desktop picker and
   preserves the user's selected GUID if that GUID still exists; otherwise it
   chooses the foreground desktop.
5. Release builds now use `panic=unwind` so worker `catch_unwind` is meaningful.
   Panic-prone private COM ABI calls remain isolated in a subprocess.
6. The FY4B source now uses HTTPS, same as every other built-in satellite source.
   A CI smoke test exercises actual TLS download and WIC decoding. Windows'
   documented WinHTTP default redirect policy disallows HTTPS -> HTTP downgrade.
7. A refresh cycle downloads each distinct satellite image **at most once** if
   more than one desktop/monitor pair needs it. The validated raw source is
   locally reused and the temporary copy removed at the end of the cycle.
8. A bound of 128 desktop × monitor combinations prevents unexpected excessive
   network traffic and BMP disk growth.
9. The single-GUI-instance check, hidden-window 3-process takeover test, output
   checksums, offline WIC render, and low-idle-memory sampling remain enabled.

## Known limitations requiring release decisions

- Windows 11 virtual desktop `SetDesktopWallpaper` COM is **undocumented** and
  supported only as a narrowly version-gated, read-only-probed subprocess call.
  Future Windows builds within the broad range might still change the ABI.
- The simultaneous virtual desktop and multi-monitor mode emulates independent
  physical monitor wallpaper application on desktop switch. Windows does not
  officially guarantee this. CI VMs cannot replicate the real display topology.
- GUI text is translated into Chinese, English, Japanese, and Korean; some
  detailed diagnostic log lines intentionally remain Chinese or English.
- The EXE is currently **unsigned**. Microsoft Defender SmartScreen may warn
  on first launch, even for a legitimate release.
- The release source ZIP contains `Cargo.lock`, generated before and used by
  the release pipeline for `cargo test --locked` and `cargo build --release --locked`.
- Long-run memory, sleep/wake, and hot-unplug have not been measured for multiple
  days in a real Windows 11 26H2 session.

## Final release acceptance tests on actual machine

- Two monitors × three virtual desktops, each with a different satellite,
  then rapid virtual-desktop switching without internet downloads.
- Unplug display #2, switch all desktops, reconnect display #2,
  then verify correct monitor identity and image resolution.
- Leave app running over laptop sleep/hibernate and wake; verify only one
  delayed catch-up and no duplicated GUI processes.
- Verify folder BMP files remain fixed per pair and disk usage plateaus.
- Disable tray, hide window, relaunch executable to replace the old instance.
- Repeat after major Windows feature updates.

No blanket claim of "security audit complete" or "zero vulnerabilities" is made.
Windows Actions and Clippy complement but do not replace real user testing.
