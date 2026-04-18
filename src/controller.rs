//! DJI controller detection and mission upload via MTP/GVFS.

use std::path::{Path, PathBuf};

/// Known DJI controller types detected from GVFS mount names.
#[derive(Debug, Clone)]
pub struct DjiController {
    pub name: String,
    pub mount_path: PathBuf,
    pub waypoint_dir: PathBuf,
}

/// The DJI Fly waypoint path inside the controller's Android filesystem.
const WAYPOINT_PATHS: &[&str] = &[
    "Internal storage/Android/data/dji.go.v5/files/waypoint",
    "Internal shared storage/Android/data/dji.go.v5/files/waypoint",
    "Interner gemeinsamer Speicher/Android/data/dji.go.v5/files/waypoint",
    "Interne opslag/Android/data/dji.go.v5/files/waypoint",
    "Almacenamiento interno compartido/Android/data/dji.go.v5/files/waypoint",
    "Stockage interne partagé/Android/data/dji.go.v5/files/waypoint",
];

/// Detect a friendly controller name from a GVFS mount directory name.
fn identify_controller(mount_name: &str) -> Option<String> {
    let lower = mount_name.to_lowercase();
    if lower.contains("dji") || lower.contains("rc") {
        // Try to extract a nice name from the GVFS mount URI
        // Typical: mtp:host=DJI_RC_2_SERIAL or mtp:host=DJI_RC_Pro_...
        if let Some(host_part) = mount_name.strip_prefix("mtp:host=") {
            let name = host_part
                .split('_')
                .take_while(|s| {
                    // Stop at serial-number-looking segments
                    s.len() < 12 && !s.chars().all(|c| c.is_ascii_hexdigit())
                })
                .collect::<Vec<_>>()
                .join(" ");
            if !name.is_empty() {
                return Some(name);
            }
        }
        Some(mount_name.replace('_', " "))
    } else {
        None
    }
}

/// Scan GVFS mount points for connected DJI controllers.
///
/// On GNOME/Linux, MTP devices are mounted under:
///   `/run/user/<UID>/gvfs/mtp:host=<DEVICE_NAME>`
pub fn detect_controllers() -> Vec<DjiController> {
    let uid = unsafe { libc::getuid() };
    let gvfs_root = PathBuf::from(format!("/run/user/{uid}/gvfs"));

    let mut controllers = Vec::new();

    let entries = match std::fs::read_dir(&gvfs_root) {
        Ok(e) => e,
        Err(_) => return controllers,
    };

    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();

        // Only look at MTP mounts
        if !dir_name.starts_with("mtp:") {
            continue;
        }

        let mount_path = entry.path();

        // Try each known waypoint path variant
        for wp_rel in WAYPOINT_PATHS {
            let wp_dir = mount_path.join(wp_rel);
            if wp_dir.is_dir() {
                let name = identify_controller(&dir_name)
                    .unwrap_or_else(|| "DJI Controller".to_string());
                controllers.push(DjiController {
                    name,
                    mount_path: mount_path.clone(),
                    waypoint_dir: wp_dir,
                });
                break;
            }
        }
    }

    controllers
}

/// Check if a folder name is a valid GUID (e.g. `4B20BF76-C5BD-49B7-8985-9E72045AC5A6`).
fn is_guid(name: &str) -> bool {
    // Pattern: 8-4-4-4-12 hex characters separated by hyphens
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected_lens = [8, 4, 4, 4, 12];
    parts
        .iter()
        .zip(expected_lens.iter())
        .all(|(part, &len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Find the most recent GUID-named mission folder inside the waypoint directory.
/// Skips non-mission folders like `map_preview`.
fn find_latest_mission(waypoint_dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;

    let entries = std::fs::read_dir(waypoint_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Only consider GUID-named folders (user-created missions)
        let folder_name = path.file_name()?.to_str()?;
        if !is_guid(folder_name) {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if best.as_ref().map_or(true, |(t, _)| modified > *t) {
                    best = Some((modified, path));
                }
            }
        }
    }

    best.map(|(_, p)| p)
}

/// Check whether the controller already has at least one saved mission (GUID folder).
pub fn has_existing_mission(controller: &DjiController) -> bool {
    find_latest_mission(&controller.waypoint_dir).is_some()
}

/// Upload a KMZ mission to the controller by replacing the most recent mission.
///
/// The procedure matches DJI's expected layout:
///   1. Find the waypoint directory on the controller
///   2. Find the most recent GUID-named mission folder
///   3. Write `{GUID}.kmz` into that folder, replacing the existing one
///
/// Returns the path it was written to on success.
pub fn upload_mission(controller: &DjiController, kmz_data: &[u8]) -> Result<PathBuf, String> {
    // Find the most recent mission folder
    let mission_dir = find_latest_mission(&controller.waypoint_dir)
        .ok_or_else(|| {
            "No existing mission found on controller. \
             Open DJI Fly on the controller, create and save a simple \
             1-waypoint mission first, then try again."
                .to_string()
        })?;

    // The folder name is a GUID like "6103D3C8-E79A-4B48-BBFE-50932D2E1306"
    let folder_name = mission_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let dest = mission_dir.join(format!("{folder_name}.kmz"));

    // First remove existing KMZ files in the mission folder so DJI Fly
    // doesn't get confused by stale data.
    if let Ok(entries) = std::fs::read_dir(&mission_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("kmz") {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    // Try direct write first (works on DJI RC via GVFS-fuse)
    match std::fs::write(&dest, kmz_data) {
        Ok(()) => {
            // Verify the write actually landed (GVFS can silently fail)
            match std::fs::metadata(&dest) {
                Ok(meta) if meta.len() == kmz_data.len() as u64 => return Ok(dest),
                _ => {}
            }
        }
        Err(_) => {}
    }

    // Fallback: use gio command-line tool which handles MTP natively
    let tmp = std::env::temp_dir().join(format!("{folder_name}.kmz"));
    std::fs::write(&tmp, kmz_data)
        .map_err(|e| format!("Failed to write temp file: {e}"))?;

    let status = std::process::Command::new("gio")
        .args(["copy", "-p"])
        .arg(&tmp)
        .arg(&dest)
        .status()
        .map_err(|e| format!("Failed to run gio copy: {e}"))?;

    let _ = std::fs::remove_file(&tmp);

    if status.success() {
        Ok(dest)
    } else {
        Err(format!(
            "Failed to copy mission to controller. \
             You can manually copy the .kmz file to:\n\
             {}\n\n\
             Make sure DJI Fly has at least one saved mission on the controller.",
            dest.display()
        ))
    }
}
