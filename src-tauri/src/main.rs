// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// When launched from inside an AppImage, `$APPDIR` is set by the AppImage
/// runtime before this binary ever starts. WebKitGTK's helper processes
/// (`WebKitWebProcess`/`WebKitNetworkProcess`) otherwise resolve via their
/// compiled-in absolute path (`/usr/libexec/webkit2gtk-4.1/...`) instead of
/// the copies bundled in this AppImage — and if that happens to find a
/// *different* webkit2gtk build already on the host, the shared-memory IPC
/// between mismatched builds is ABI-incompatible and the helper processes
/// crash with SIGBUS almost immediately (confirmed via `coredumpctl` during
/// a real reproduction, not a guess). `linuxdeploy`'s own generated AppRun
/// hook doesn't set this and gets regenerated on every build regardless of
/// what's injected via `tauri.conf.json`'s `bundle.linux.appimage.files`, so
/// this has to happen in Rust — the one place a rebuild can't clobber it.
/// Must run before any GTK/WebKit initialization, hence first line of main().
#[cfg(target_os = "linux")]
fn fix_appimage_webkit_exec_path() {
    if std::env::var_os("WEBKIT_EXEC_PATH").is_some() {
        return;
    }
    if let Some(appdir) = std::env::var_os("APPDIR") {
        let path = std::path::Path::new(&appdir).join("usr/libexec/webkit2gtk-4.1");
        if path.exists() {
            // SAFETY: called at the very start of main(), single-threaded,
            // before any other code (Tauri, GTK, WebKit) reads or writes
            // process environment.
            unsafe {
                std::env::set_var("WEBKIT_EXEC_PATH", path);
            }
        }
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    fix_appimage_webkit_exec_path();

    omnideck_desktop_lib::run()
}
