mod application;
mod config;
mod controller;
mod engine;
mod models;
mod terrain;
mod widgets;
mod window;

fn main() {
    // On Windows, GLib/GIO cannot find GSettings schemas unless we point it at
    // the right directory.  Without schemas libsoup can't read proxy settings
    // (blank map) and FileDialog can't load its state (crash).
    //
    // Strategy: search common locations for `gschemas.compiled`.
    // If nothing is found, fall back to the in-memory backend so at least
    // tile fetching works (FileDialog will still be degraded).
    #[cfg(target_os = "windows")]
    {
        if std::env::var_os("GSETTINGS_SCHEMA_DIR").is_none() {
            if let Some(schema_dir) = find_windows_schema_dir() {
                std::env::set_var("GSETTINGS_SCHEMA_DIR", &schema_dir);
            } else if std::env::var_os("GSETTINGS_BACKEND").is_none() {
                // Last resort: memory backend avoids the NULL source assertion
                // in libsoup, so map tiles still load.
                std::env::set_var("GSETTINGS_BACKEND", "memory");
            }
        }
    }

    let app = application::WayfarerApp::new();
    std::process::exit(app.run());
}

/// Search well-known locations for `gschemas.compiled` on Windows.
#[cfg(target_os = "windows")]
fn find_windows_schema_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    let suffix: PathBuf = ["share", "glib-2.0", "schemas"].iter().collect();

    // 1. Next to the executable  (shipped app)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(&suffix);
            if candidate.join("gschemas.compiled").exists() {
                return Some(candidate);
            }
            // Also try one level up (exe might be in a bin/ subfolder)
            if let Some(parent) = dir.parent() {
                let candidate = parent.join(&suffix);
                if candidate.join("gschemas.compiled").exists() {
                    return Some(candidate);
                }
            }
        }
    }

    // 2. MSYS2 / MINGW prefixes (common GTK install paths on Windows)
    for prefix in &[
        r"C:\msys64\mingw64",
        r"C:\msys64\ucrt64",
        r"C:\msys64\clang64",
        r"C:\msys64\mingw32",
        r"C:\gtk\gtk-4",
    ] {
        let candidate = PathBuf::from(prefix).join(&suffix);
        if candidate.join("gschemas.compiled").exists() {
            return Some(candidate);
        }
    }

    // 3. GTK_BASEPATH or similar env vars set by some installers
    for var in &["GTK_BASEPATH", "GTK_PATH", "MINGW_PREFIX"] {
        if let Some(val) = std::env::var_os(var) {
            let candidate = PathBuf::from(val).join(&suffix);
            if candidate.join("gschemas.compiled").exists() {
                return Some(candidate);
            }
        }
    }

    None
}
