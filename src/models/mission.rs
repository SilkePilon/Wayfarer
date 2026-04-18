//! Central mission state and related enumerations.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::engine::mapping::LatLng;
use crate::models::camera::{default_presets, CameraPreset};

// ─── Project metadata (stored in the app-level projects database) ────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    pub path: PathBuf,
    pub location_name: String,
    pub lat: f64,
    pub lng: f64,
    pub created: String,       // ISO-8601
    pub last_modified: String, // ISO-8601
}

// ─── Enums ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FinishAction {
    #[default]
    NoAction,
    GoHome,
    AutoLand,
    GotoFirstWaypoint,
}

impl FinishAction {
    pub fn to_wpml(self) -> &'static str {
        match self {
            Self::NoAction => "noAction",
            Self::GoHome => "goHome",
            Self::AutoLand => "autoLand",
            Self::GotoFirstWaypoint => "gotoFirstWaypoint",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NoAction => "Hover",
            Self::GoHome => "Return to Home",
            Self::AutoLand => "Auto Land",
            Self::GotoFirstWaypoint => "Go to First Waypoint",
        }
    }

    pub fn all() -> &'static [FinishAction] {
        &[
            Self::NoAction,
            Self::GoHome,
            Self::AutoLand,
            Self::GotoFirstWaypoint,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RcLostAction {
    #[default]
    Hover,
    GoBack,
    Landing,
}

impl RcLostAction {
    pub fn to_wpml(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::GoBack => "goBack",
            Self::Landing => "landing",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hover => "Hover",
            Self::GoBack => "Return to Home",
            Self::Landing => "Land",
        }
    }

    pub fn all() -> &'static [RcLostAction] {
        &[Self::Hover, Self::GoBack, Self::Landing]
    }
}

// ─── Computed mission statistics ─────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct MissionStats {
    pub waypoint_count: usize,
    pub flight_distance_m: f64,
    pub area_m2: f64,
    pub estimated_time_min: f64,
    pub recommended_shutter: String,
    pub photo_interval_s: f64,
    pub gsd_cm: f64, // ground sampling distance in cm/px
}

// ─── Main application state ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    // Flight
    pub altitude: f64,
    pub ground_offset: f64,
    pub speed: f64,
    pub camera_angle: i32,
    pub delay_at_waypoint: i32,
    pub heading_angle: Option<i32>, // None = follow wayline, Some(0..359) = fixed heading

    // Overlap / grid
    pub forward_overlap: f64, // fraction
    pub side_overlap: f64,    // fraction
    pub rotation: f64,        // degrees

    // Camera (active values mirror the selected preset for editable ones)
    pub sensor_width: f64,
    pub sensor_height: f64,
    pub focal_length: f64,
    pub image_width: i32,
    pub image_height: i32,

    // Display toggles
    pub show_waypoints: bool,
    pub fill_grid: bool,
    pub create_camera_points: bool,
    pub satellite_map: bool,
    pub dark_mode: bool,
    pub terrain_following: bool,

    // Mission completion
    pub finish_action: FinishAction,
    pub rc_lost_action: RcLostAction,

    // Selected preset index into the merged (built-in + user) list
    pub selected_preset_idx: usize,

    // ── Geometry (persisted to project settings.json) ────────────────────
    #[serde(default)]
    pub polygon: Vec<LatLng>,
    #[serde(default)]
    pub home_point: Option<LatLng>,
    #[serde(default)]
    pub waypoints: Vec<LatLng>,

    // ── Project info (persisted) ──────────────────────────────────────────
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub location_name: String,
    #[serde(default)]
    pub center_lat: f64,
    #[serde(default)]
    pub center_lng: f64,

    // ── Runtime-only fields ───────────────────────────────────────────────
    #[serde(skip)]
    pub presets: Vec<CameraPreset>,
    #[serde(skip)]
    pub terrain_elevations: Vec<f64>,
    #[serde(skip)]
    pub project_dir: Option<PathBuf>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            altitude: 50.0,
            ground_offset: 0.0,
            speed: 4.0,
            camera_angle: -90,
            delay_at_waypoint: 2,
            heading_angle: None,
            forward_overlap: 0.60,
            side_overlap: 0.40,
            rotation: 0.0,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.8,
            image_width: 4000,
            image_height: 3000,
            show_waypoints: false,
            fill_grid: false,
            create_camera_points: false,
            satellite_map: false,
            dark_mode: false,
            terrain_following: false,
            finish_action: FinishAction::default(),
            rc_lost_action: RcLostAction::default(),
            selected_preset_idx: 0,
            presets: default_presets(),
            polygon: vec![],
            home_point: None,
            waypoints: vec![],
            terrain_elevations: vec![],
            project_name: String::new(),
            project_dir: None,
            location_name: String::new(),
            center_lat: 0.0,
            center_lng: 0.0,
        }
    }
}

impl AppState {
    /// Return the currently active camera preset.
    pub fn active_preset(&self) -> Option<&CameraPreset> {
        self.presets.get(self.selected_preset_idx)
    }

    /// Apply fields from the given preset to the camera parameters.
    pub fn apply_preset(&mut self, preset: &CameraPreset) {
        self.sensor_width = preset.sensor_width;
        self.sensor_height = preset.sensor_height;
        self.focal_length = preset.focal_length;
        self.image_width = preset.image_width;
        self.image_height = preset.image_height;
    }

    /// Compute statistics for the current waypoint list.
    pub fn compute_stats(&self) -> MissionStats {
        use crate::engine::mapping::MappingEngine;

        let n = self.waypoints.len();
        if n == 0 {
            return MissionStats::default();
        }

        let flight_dist = MappingEngine::total_distance(&self.waypoints);
        let area = if self.polygon.len() >= 3 {
            MappingEngine::calculate_area(&self.polygon)
        } else {
            0.0
        };

        let estimated_time_min =
            (flight_dist / self.speed.max(0.01) + n as f64 * self.delay_at_waypoint as f64) / 60.0;

        let shutter = MappingEngine::recommended_shutter(
            self.altitude,
            self.sensor_width,
            self.focal_length,
            self.image_width,
            self.speed,
        );

        let eff_alt = (self.altitude - self.ground_offset).max(0.01);
        let gsd_m = (eff_alt * self.sensor_width) / (self.image_width as f64 * self.focal_length);
        let gsd_cm = gsd_m * 100.0;

        // Interval (seconds) between photos at current speed / footprint
        let footprint_h = (eff_alt * self.sensor_height)
            / (self.image_height as f64 * self.focal_length)
            * self.image_height as f64;
        let spacing = footprint_h * (1.0 - self.forward_overlap);
        let photo_interval_s = spacing / self.speed.max(0.01);

        MissionStats {
            waypoint_count: n,
            flight_distance_m: flight_dist,
            area_m2: area,
            estimated_time_min,
            recommended_shutter: shutter,
            photo_interval_s,
            gsd_cm,
        }
    }
}
