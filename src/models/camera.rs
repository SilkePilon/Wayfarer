//! Camera preset definitions.
//!
//! Built-in presets are read-only; user presets are persisted to
//! `~/.config/wayfarer/presets.json`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraPreset {
    pub name: String,
    pub default_preset: bool,
    pub sensor_width: f64,  // mm
    pub sensor_height: f64, // mm
    pub focal_length: f64,  // mm
    pub image_width: i32,   // px
    pub image_height: i32,  // px
}

impl CameraPreset {
    pub fn is_editable(&self) -> bool {
        !self.default_preset
    }
}

/// All built-in camera presets.
pub fn default_presets() -> Vec<CameraPreset> {
    vec![
        // Generic / custom
        CameraPreset {
            name: "Custom".into(),
            default_preset: true,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.8,
            image_width: 4000,
            image_height: 3000,
        },
        // ── DJI ──────────────────────────────────────────────────────────
        CameraPreset {
            name: "DJI Mini 2".into(),
            default_preset: true,
            sensor_width: 6.16,
            sensor_height: 4.62,
            focal_length: 4.49,
            image_width: 4000,
            image_height: 3000,
        },
        CameraPreset {
            name: "DJI Mini 3".into(),
            default_preset: true,
            sensor_width: 9.6,
            sensor_height: 7.2,
            focal_length: 6.7,
            image_width: 4000,
            image_height: 3000,
        },
        CameraPreset {
            name: "DJI Mini 3 Pro".into(),
            default_preset: true,
            sensor_width: 9.6,
            sensor_height: 7.2,
            focal_length: 6.7,
            image_width: 4032,
            image_height: 3024,
        },
        CameraPreset {
            name: "DJI Mini 4 Pro".into(),
            default_preset: true,
            sensor_width: 9.6,
            sensor_height: 7.2,
            focal_length: 6.7,
            image_width: 4032,
            image_height: 3024,
        },
        CameraPreset {
            name: "DJI Flip".into(),
            default_preset: true,
            sensor_width: 9.6,
            sensor_height: 7.2,
            focal_length: 6.7,
            image_width: 8064,
            image_height: 6048,
        },
        CameraPreset {
            name: "DJI Mini 5 Pro".into(),
            default_preset: true,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.8,
            image_width: 8192,
            image_height: 6144,
        },
        CameraPreset {
            name: "DJI Air 2S".into(),
            default_preset: true,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.38,
            image_width: 5472,
            image_height: 3648,
        },
        CameraPreset {
            name: "DJI Air 3".into(),
            default_preset: true,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.4,
            image_width: 4032,
            image_height: 3024,
        },
        CameraPreset {
            name: "DJI Air 3S".into(),
            default_preset: true,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.8,
            image_width: 8192,
            image_height: 6144,
        },
        CameraPreset {
            name: "DJI Mavic 3 Classic".into(),
            default_preset: true,
            sensor_width: 17.3,
            sensor_height: 13.0,
            focal_length: 12.3,
            image_width: 5280,
            image_height: 3956,
        },
        CameraPreset {
            name: "DJI Mavic 3 Enterprise".into(),
            default_preset: true,
            sensor_width: 17.3,
            sensor_height: 13.0,
            focal_length: 12.3,
            image_width: 5280,
            image_height: 3956,
        },
        CameraPreset {
            name: "DJI Mavic 3 Pro".into(),
            default_preset: true,
            sensor_width: 17.3,
            sensor_height: 13.0,
            focal_length: 12.3,
            image_width: 5280,
            image_height: 3956,
        },
        CameraPreset {
            name: "DJI Phantom 4 Pro".into(),
            default_preset: true,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.8,
            image_width: 5472,
            image_height: 3648,
        },
        CameraPreset {
            name: "DJI Phantom 4 RTK".into(),
            default_preset: true,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.8,
            image_width: 5472,
            image_height: 3648,
        },
        // ── Autel ─────────────────────────────────────────────────────────
        CameraPreset {
            name: "Autel EVO Nano+".into(),
            default_preset: true,
            sensor_width: 6.4,
            sensor_height: 4.8,
            focal_length: 4.3,
            image_width: 4000,
            image_height: 3000,
        },
        CameraPreset {
            name: "Autel EVO Lite+".into(),
            default_preset: true,
            sensor_width: 9.6,
            sensor_height: 7.2,
            focal_length: 6.24,
            image_width: 6000,
            image_height: 4000,
        },
    ]
}
