//! Main application window — project-based navigation with tabbed editor.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::prelude::*;

use crate::config;
use crate::controller;
use crate::engine::mapping::MappingEngine;
use crate::engine::{dji, litchi};
use crate::models::camera::CameraPreset;
use crate::models::mission::{AppState, FinishAction, MissionStats, ProjectMeta, RcLostAction};
use crate::terrain;
use crate::widgets::map_view::{MapMessage, MapView};
use crate::widgets::projects_page::{self, ProjectAction};

// ─── Step identifiers ─────────────────────────────────────────────────────────

const STEP_NAMES: [&str; 4] = ["draw", "aircraft", "camera", "review"];
const STEP_LABELS: [&str; 4] = ["Draw Area", "Aircraft", "Camera", "Review & Export"];
const STEP_ICONS: [&str; 4] = [
    "edit-select-symbolic",
    "airplane-mode-symbolic",
    "camera-photo-symbolic",
    "document-send-symbolic",
];
const CHECK_ICON: &str = "emblem-ok-symbolic";

// ─── Projects database ───────────────────────────────────────────────────────

fn projects_db_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("io", "github.silkepilon", "Wayfarer")
        .map(|d| d.config_dir().join("projects.json"))
}

fn presets_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("io", "github.silkepilon", "Wayfarer")
        .map(|d| d.config_dir().join("presets.json"))
}

fn load_projects() -> Vec<ProjectMeta> {
    projects_db_path()
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn save_projects(projects: &[ProjectMeta]) {
    if let Some(path) = projects_db_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(projects) {
            let _ = std::fs::write(&path, json);
        }
    }
}

fn load_presets() -> Vec<CameraPreset> {
    let mut presets = crate::models::camera::default_presets();
    if let Some(path) = presets_path() {
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(user_presets) = serde_json::from_str::<Vec<CameraPreset>>(&json) {
                let default_names: std::collections::HashSet<String> =
                    presets.iter().map(|p| p.name.clone()).collect();
                presets.extend(
                    user_presets
                        .into_iter()
                        .filter(|p| !default_names.contains(&p.name)),
                );
            }
        }
    }
    presets
}

fn save_presets(presets: &[CameraPreset]) {
    if let Some(path) = presets_path() {
        let user: Vec<_> = presets.iter().filter(|p| !p.default_preset).collect();
        if let Ok(json) = serde_json::to_string_pretty(&user) {
            let _ = std::fs::write(&path, json);
        }
    }
}

// ─── Per-project settings ─────────────────────────────────────────────────────

fn load_project_state(project_dir: &PathBuf) -> AppState {
    let settings_path = project_dir.join("settings.json");
    let mut state = AppState::default();
    state.presets = load_presets();
    if let Ok(json) = std::fs::read_to_string(&settings_path) {
        if let Ok(s) = serde_json::from_str::<AppState>(&json) {
            state.altitude = s.altitude;
            state.ground_offset = s.ground_offset;
            state.speed = s.speed;
            state.camera_angle = s.camera_angle;
            state.delay_at_waypoint = s.delay_at_waypoint;
            state.heading_angle = s.heading_angle;
            state.forward_overlap = s.forward_overlap;
            state.side_overlap = s.side_overlap;
            state.rotation = s.rotation;
            state.sensor_width = s.sensor_width;
            state.sensor_height = s.sensor_height;
            state.focal_length = s.focal_length;
            state.image_width = s.image_width;
            state.image_height = s.image_height;
            state.show_waypoints = s.show_waypoints;
            state.fill_grid = s.fill_grid;
            state.create_camera_points = s.create_camera_points;
            state.satellite_map = s.satellite_map;
            state.dark_mode = s.dark_mode;
            state.terrain_following = s.terrain_following;
            state.finish_action = s.finish_action;
            state.rc_lost_action = s.rc_lost_action;
            state.selected_preset_idx = s.selected_preset_idx;
            state.polygon = s.polygon;
            state.home_point = s.home_point;
            state.waypoints = s.waypoints;
            state.project_name = s.project_name;
            state.location_name = s.location_name;
            state.center_lat = s.center_lat;
            state.center_lng = s.center_lng;
        }
    }
    state.project_dir = Some(project_dir.clone());
    state
}

fn save_project_state(state: &AppState) {
    if let Some(ref dir) = state.project_dir {
        let path = dir.join("settings.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(state) {
            let _ = std::fs::write(&path, json);
        }
    }
    save_presets(&state.presets);

    // Update project DB metadata
    if let Some(ref dir) = state.project_dir {
        let mut projects = load_projects();
        let now = chrono_now();
        if let Some(p) = projects.iter_mut().find(|p| p.path == *dir) {
            p.name = state.project_name.clone();
            p.last_modified = now;
        }
        save_projects(&projects);
    }
}

fn chrono_now() -> String {
    // Simple ISO-8601 timestamp without external crate
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Approximate date from days since epoch (good enough for display)
    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn default_project_parent() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|d| d.document_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Wayfarer Projects")
}

// ─── Build window ─────────────────────────────────────────────────────────────

pub fn build_window(app: &adw::Application) {
    let toast_overlay = adw::ToastOverlay::new();

    // ── Top-level stack: projects / setup / editor ────────────────────────
    let main_stack = gtk4::Stack::new();
    main_stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
    main_stack.set_transition_duration(200);

    // ── State ─────────────────────────────────────────────────────────────
    let state: Rc<RefCell<AppState>> = Rc::new(RefCell::new(AppState::default()));
    state.borrow_mut().presets = load_presets();

    // ── Map view (shared between setup + editor) ──────────────────────────
    let recalc_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let map_view: Rc<MapView> = {
        let state = state.clone();
        let toast = toast_overlay.clone();
        let recalc_slot = recalc_slot.clone();
        Rc::new(MapView::new(Rc::new(move |msg| {
            handle_map_message(msg, &state, &toast, &recalc_slot);
        })))
    };

    map_view.request_user_location();

    // ── Stats labels (populated by review tab) ────────────────────────────
    let stats_labels: Rc<RefCell<Option<StatsLabels>>> = Rc::new(RefCell::new(None));

    // ── Editor ViewStack ──────────────────────────────────────────────────
    let view_stack = adw::ViewStack::new();
    view_stack.set_vhomogeneous(false);

    // ── Recalculate closure ───────────────────────────────────────────────
    let recalculate: Rc<dyn Fn()> = {
        let state = state.clone();
        let map = map_view.clone();
        let toast = toast_overlay.clone();
        let stats_ref = stats_labels.clone();

        Rc::new(move || {
            let s = state.borrow();
            if s.polygon.len() < 3 || s.altitude < 5.0 {
                map.clear_flight_path();
                if let Some(labels) = stats_ref.borrow().as_ref() {
                    labels.update(&MissionStats::default());
                }
                return;
            }

            let engine = MappingEngine {
                altitude: s.altitude,
                ground_offset: s.ground_offset,
                forward_overlap: s.forward_overlap,
                side_overlap: s.side_overlap,
                sensor_width: s.sensor_width,
                sensor_height: s.sensor_height,
                focal_length: s.focal_length,
                image_width: s.image_width,
                image_height: s.image_height,
                angle: s.rotation,
            };

            let waypoints = engine.generate_waypoints(
                &s.polygon,
                s.create_camera_points,
                s.fill_grid,
                s.home_point,
            );
            let home = s.home_point;
            let show_wps = s.show_waypoints;
            let terrain = s.terrain_following;
            drop(s);

            map.set_show_waypoints(show_wps);
            state.borrow_mut().waypoints = waypoints.clone();
            map.update_flight_path(&waypoints, home);

            if let Some(labels) = stats_ref.borrow().as_ref() {
                let stats = state.borrow().compute_stats();
                labels.update(&stats);
            }

            if terrain && !waypoints.is_empty() {
                let state2 = state.clone();
                let toast2 = toast.clone();
                let stats_ref2 = stats_ref.clone();
                terrain::fetch_elevations(waypoints.clone(), move |result| match result {
                    terrain::ElevationResult::Ok(elevs) => {
                        state2.borrow_mut().terrain_elevations = elevs;
                        if let Some(labels) = stats_ref2.borrow().as_ref() {
                            let stats = state2.borrow().compute_stats();
                            labels.update(&stats);
                        }
                    }
                    terrain::ElevationResult::Err(e) => {
                        toast2.add_toast(adw::Toast::new(&format!("Terrain fetch failed: {e}")));
                    }
                });
            }

            save_project_state(&state.borrow());
        })
    };

    *recalc_slot.borrow_mut() = Some(recalculate.clone());

    // ── Step completion tracking ──────────────────────────────────────────
    let completed: Rc<RefCell<[bool; 4]>> = Rc::new(RefCell::new([false; 4]));

    // ── Build tab pages ───────────────────────────────────────────────────
    let draw_page = build_tab_draw(
        state.clone(),
        recalculate.clone(),
        map_view.clone(),
        view_stack.clone(),
        completed.clone(),
    );
    let aircraft_page = build_tab_aircraft(
        state.clone(),
        recalculate.clone(),
        view_stack.clone(),
        completed.clone(),
    );
    let camera_page = build_tab_camera(
        state.clone(),
        recalculate.clone(),
        view_stack.clone(),
        completed.clone(),
    );
    let review_page = build_tab_review(
        state.clone(),
        recalculate.clone(),
        toast_overlay.clone(),
        stats_labels.clone(),
        completed.clone(),
        view_stack.clone(),
    );

    for (i, widget) in [&draw_page, &aircraft_page, &camera_page, &review_page]
        .iter()
        .enumerate()
    {
        let page = view_stack.add_titled(*widget, Some(STEP_NAMES[i]), STEP_LABELS[i]);
        page.set_icon_name(Some(STEP_ICONS[i]));
    }

    // ── Projects page ─────────────────────────────────────────────────────
    let projects_db: Rc<RefCell<Vec<ProjectMeta>>> = Rc::new(RefCell::new(load_projects()));

    let (projects_widget, refresh_projects) = {
        let state = state.clone();
        let map = map_view.clone();
        let main_stack = main_stack.clone();
        let recalc = recalculate.clone();
        let completed = completed.clone();
        let view_stack2 = view_stack.clone();
        let toast = toast_overlay.clone();

        projects_page::build_projects_page(Rc::new(move |action| match action {
            ProjectAction::Open(meta) => {
                let project_dir = meta.path.clone();
                let loaded = load_project_state(&project_dir);

                // Extract values before any map calls (which trigger
                // signal handlers that borrow state).
                let polygon;
                let center_lat;
                let center_lng;
                {
                    let mut s = state.borrow_mut();
                    *s = loaded;
                    polygon = s.polygon.clone();
                    center_lat = s.center_lat;
                    center_lng = s.center_lng;
                }
                // State borrow is dropped — safe to call map methods
                // (they emit signals that borrow_mut state).
                if !polygon.is_empty() {
                    map.import_polygon(&polygon);
                }
                if center_lat != 0.0 || center_lng != 0.0 {
                    map.set_center(center_lat, center_lng, Some(17));
                }
                // Reset steps
                *completed.borrow_mut() = [false; 4];
                refresh_tab_icons(&view_stack2, &completed.borrow());
                view_stack2.set_visible_child_name("draw");
                recalc();
                main_stack.set_visible_child_name("editor");
                toast.add_toast(adw::Toast::new(&format!(
                    "Opened project \"{}\"",
                    meta.name
                )));
            }
            ProjectAction::Delete(meta) => {
                // Just remove from DB, don't delete files
                let _ = meta;
            }
        }))
    };

    // ── Shared location selection (setup → project-details) ─────────────
    let selected_location_name: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let selected_lat: Rc<std::cell::Cell<f64>> = Rc::new(std::cell::Cell::new(0.0));
    let selected_lng: Rc<std::cell::Cell<f64>> = Rc::new(std::cell::Cell::new(0.0));

    // ── Setup page (new project — search for location) ────────────────────
    let setup_page = build_setup_page(
        main_stack.clone(),
        selected_location_name.clone(),
        selected_lat.clone(),
        selected_lng.clone(),
    );

    // ── Project details page (name, folder, continue) ─────────────────────
    let project_details_page = build_project_details_page(
        state.clone(),
        map_view.clone(),
        main_stack.clone(),
        toast_overlay.clone(),
        projects_db.clone(),
        refresh_projects.clone(),
        recalculate.clone(),
        completed.clone(),
        view_stack.clone(),
        selected_location_name.clone(),
        selected_lat.clone(),
        selected_lng.clone(),
    );

    // ── Add pages to main stack ───────────────────────────────────────────
    main_stack.add_named(&projects_widget, Some("projects"));
    main_stack.add_named(&setup_page, Some("setup"));
    main_stack.add_named(&project_details_page, Some("project-details"));
    main_stack.add_named(&view_stack.clone().upcast::<gtk4::Widget>(), Some("editor"));

    // ── Header bar ────────────────────────────────────────────────────────
    let header = adw::HeaderBar::new();

    // Title widget switches between projects title and ViewSwitcher
    let projects_title = adw::WindowTitle::new("Wayfarer", "Your Projects");
    let view_switcher = adw::ViewSwitcher::builder()
        .stack(&view_stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    let setup_title = adw::WindowTitle::new("New Project", "Search for your mapping area");
    let details_title = adw::WindowTitle::new("New Project", "Configure your project");

    let title_stack = gtk4::Stack::new();
    title_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    title_stack.add_named(&projects_title, Some("projects"));
    title_stack.add_named(&view_switcher, Some("editor"));
    title_stack.add_named(&setup_title, Some("setup"));
    title_stack.add_named(&details_title, Some("project-details"));
    header.set_title_widget(Some(&title_stack));

    // "New Project" button (visible on projects page)
    let new_project_btn = gtk4::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("New Project")
        .build();
    new_project_btn.add_css_class("suggested-action");

    // "Back to Projects" button (visible in editor)
    let back_btn = gtk4::Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text("Back to Projects")
        .build();

    // "Cancel" button (visible in setup)
    let cancel_btn = gtk4::Button::builder().label("Cancel").build();

    // "Back" button (visible in project-details, goes back to search)
    let details_back_btn = gtk4::Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text("Back to Search")
        .build();

    // Button stack for start side
    let start_stack = gtk4::Stack::new();
    start_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    start_stack.add_named(&new_project_btn, Some("projects"));
    start_stack.add_named(&back_btn, Some("editor"));
    start_stack.add_named(&cancel_btn, Some("setup"));
    start_stack.add_named(&details_back_btn, Some("project-details"));
    header.pack_start(&start_stack);

    // Menu (always visible)
    let menu = gio::Menu::new();

    let map_section = gio::Menu::new();
    let layer_submenu = gio::Menu::new();
    layer_submenu.append(Some("OpenStreetMap"), Some("win.map-layer::osm"));
    layer_submenu.append(
        Some("Google Satellite"),
        Some("win.map-layer::google-satellite"),
    );
    map_section.append_submenu(Some("Map Layer"), &layer_submenu);
    map_section.append(Some("Show Labels"), Some("win.show-labels"));
    map_section.append(Some("Clear Markers"), Some("win.clear-markers"));
    menu.append_section(None, &map_section);

    let appearance_menu = gio::Menu::new();
    appearance_menu.append(Some("Dark Mode"), Some("win.dark-mode"));
    menu.append_submenu(Some("Appearance"), &appearance_menu);
    menu.append(Some("About Wayfarer"), Some("win.about"));

    let menu_btn = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();
    header.pack_end(&menu_btn);

    // ── Assemble ──────────────────────────────────────────────────────────
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&main_stack));

    toast_overlay.set_child(Some(&toolbar));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(config::APP_NAME)
        .default_width(1100)
        .default_height(750)
        .content(&toast_overlay)
        .build();

    // ── CSS ───────────────────────────────────────────────────────────────
    let css = gtk4::CssProvider::new();
    css.load_from_string(
        r#"
        .step-done-icon { color: @success_color; }
        .stat-value { font-weight: bold; font-size: 1.15em; }
        .step-description { font-size: 1.05em; }
        .map-container { border-radius: 12px; }
        .map-rounded-right { border-radius: 0 12px 12px 0; border: none; padding: 0; }
        .upload-progress {
            background: linear-gradient(
                to right,
                @accent_bg_color var(--progress, 0%),
                shade(@accent_bg_color, 0.6) var(--progress, 0%)
            );
            color: @accent_fg_color;
            transition: none;
        }
        .setup-search-bar {
            min-width: 420px;
            border-radius: 12px;
        }
        .setup-search-container {
            background: alpha(@window_bg_color, 0.92);
            border-radius: 14px;
            padding: 8px;
        }
        .setup-confirm-bar {
            background: alpha(@window_bg_color, 0.92);
            border-radius: 12px;
            padding: 12px 24px;
        }
        "#,
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::prelude::WidgetExt::display(&window),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // ── Update header when main stack changes ─────────────────────────────
    {
        let title_stack = title_stack.clone();
        let start_stack = start_stack.clone();
        main_stack.connect_visible_child_name_notify(move |stack| {
            if let Some(name) = stack.visible_child_name() {
                title_stack.set_visible_child_name(&name);
                start_stack.set_visible_child_name(&name);
            }
        });
    }

    // Update tab checkmarks
    {
        let stack = view_stack.clone();
        let done = completed.clone();
        view_stack.connect_visible_child_name_notify(move |_| {
            refresh_tab_icons(&stack, &done.borrow());
        });
    }

    // ── Actions ───────────────────────────────────────────────────────────

    // About
    {
        let win = window.clone();
        let action = gio::SimpleAction::new("about", None);
        action.connect_activate(move |_, _| {
            let about = adw::AboutDialog::builder()
                .application_name(config::APP_NAME)
                .application_icon(config::APP_ID)
                .version(config::APP_VERSION)
                .comments(config::APP_DESCRIPTION)
                .website(config::APP_WEBSITE)
                .developer_name(config::DEVELOPER)
                .copyright(config::COPYRIGHT)
                .license_type(gtk4::License::Gpl30)
                .build();
            about.present(Some(&win));
        });
        window.add_action(&action);
    }

    // Dark mode
    {
        let state = state.clone();
        let map = map_view.clone();
        let dark_init = state.borrow().dark_mode;
        let action = gio::SimpleAction::new_stateful("dark-mode", None, &dark_init.to_variant());
        action.connect_activate(move |action, _| {
            let current: bool = action.state().unwrap().get().unwrap_or(false);
            let new_val = !current;
            action.set_state(&new_val.to_variant());
            state.borrow_mut().dark_mode = new_val;
            let style = adw::StyleManager::default();
            style.set_color_scheme(if new_val {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::Default
            });
            map.set_dark_mode(new_val);
        });
        window.add_action(&action);
        if dark_init {
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        }
    }

    // Map layer switching
    let current_layer: Rc<RefCell<String>> = Rc::new(RefCell::new("osm".to_string()));
    let labels_on: Rc<std::cell::Cell<bool>> = Rc::new(std::cell::Cell::new(true));

    let apply_map_source = {
        let map = map_view.clone();
        let layer = current_layer.clone();
        let labels = labels_on.clone();
        Rc::new(move || {
            let l = layer.borrow();
            let show = labels.get();
            match l.as_str() {
                "osm" => {
                    if show {
                        map.set_map_source_url(
                            "osm-mapnik",
                            "OpenStreetMap",
                            "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
                        );
                    } else {
                        map.set_map_source_url(
                            "osm-nolabels",
                            "OpenStreetMap (no labels)",
                            "https://basemaps.cartocdn.com/rastertiles/voyager_nolabels/{z}/{x}/{y}.png",
                        );
                    }
                }
                "google-satellite" => {
                    let lyrs = if show { "y" } else { "s" };
                    let url = format!(
                        "https://mt1.google.com/vt/lyrs={}&x={{x}}&y={{y}}&z={{z}}",
                        lyrs
                    );
                    map.set_map_source_url("google-satellite", "Google Satellite", &url);
                }
                _ => {}
            }
        })
    };

    // Apply map source at init so we don't depend on MapSourceRegistry
    // (which may not include OSM Mapnik on Windows).
    apply_map_source();

    {
        let layer = current_layer.clone();
        let apply = apply_map_source.clone();
        let action = gio::SimpleAction::new_stateful(
            "map-layer",
            Some(&String::static_variant_type()),
            &"osm".to_variant(),
        );
        action.connect_activate(move |action, param| {
            let new_layer: String = param.unwrap().get().unwrap();
            action.set_state(&new_layer.to_variant());
            *layer.borrow_mut() = new_layer;
            apply();
        });
        window.add_action(&action);
    }

    {
        let labels = labels_on.clone();
        let apply = apply_map_source.clone();
        let action = gio::SimpleAction::new_stateful("show-labels", None, &true.to_variant());
        action.connect_activate(move |action, _| {
            let current: bool = action.state().unwrap().get().unwrap_or(true);
            let new_val = !current;
            action.set_state(&new_val.to_variant());
            labels.set(new_val);
            apply();
        });
        window.add_action(&action);
    }

    // Clear markers
    {
        let state = state.clone();
        let map = map_view.clone();
        let recalc = recalculate.clone();
        let action = gio::SimpleAction::new("clear-markers", None);
        action.connect_activate(move |_, _| {
            {
                let mut s = state.borrow_mut();
                s.polygon.clear();
                s.home_point = None;
                s.waypoints.clear();
            }
            map.clear_all();
            recalc();
        });
        window.add_action(&action);
    }

    // Initial map source
    {
        let apply = apply_map_source.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
            apply();
        });
    }

    // ── Wire navigation buttons ───────────────────────────────────────────

    // New Project button
    {
        let main_stack = main_stack.clone();
        new_project_btn.connect_clicked(move |_| {
            main_stack.set_visible_child_name("setup");
        });
    }

    // Back to Projects button
    {
        let main_stack = main_stack.clone();
        let state = state.clone();
        let map = map_view.clone();
        let projects_db = projects_db.clone();
        let refresh = refresh_projects.clone();
        back_btn.connect_clicked(move |_| {
            // Save current project before going back
            save_project_state(&state.borrow());
            map.clear_all();
            // Refresh projects list
            let projects = load_projects();
            *projects_db.borrow_mut() = projects.clone();
            refresh(projects);
            main_stack.set_visible_child_name("projects");
        });
    }

    // Cancel button (setup page)
    {
        let main_stack = main_stack.clone();
        let projects_db = projects_db.clone();
        let refresh = refresh_projects.clone();
        cancel_btn.connect_clicked(move |_| {
            let projects = projects_db.borrow().clone();
            refresh(projects);
            main_stack.set_visible_child_name("projects");
        });
    }

    // Details back button (project-details → setup search)
    {
        let main_stack = main_stack.clone();
        details_back_btn.connect_clicked(move |_| {
            main_stack.set_visible_child_name("setup");
        });
    }

    // ── Start on projects page ────────────────────────────────────────────
    {
        let projects = projects_db.borrow().clone();
        refresh_projects(projects);
    }
    main_stack.set_visible_child_name("projects");

    window.present();
}

// ═══════════════════════════════════════════════════════════════════════════════
// SETUP PAGE — Step 1: Search for location
// ═══════════════════════════════════════════════════════════════════════════════

fn build_setup_page(
    main_stack: gtk4::Stack,
    selected_name: Rc<RefCell<String>>,
    selected_lat: Rc<std::cell::Cell<f64>>,
    selected_lng: Rc<std::cell::Cell<f64>>,
) -> gtk4::Widget {
    let clamp = adw::Clamp::builder()
        .maximum_size(600)
        .vexpand(true)
        .valign(gtk4::Align::Center)
        .margin_start(24)
        .margin_end(24)
        .build();

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(24)
        .valign(gtk4::Align::Center)
        .build();

    let icon = gtk4::Image::builder()
        .icon_name("find-location-symbolic")
        .pixel_size(64)
        .css_classes(["dim-label"])
        .build();
    vbox.append(&icon);

    let heading = gtk4::Label::builder()
        .label("Where are you mapping?")
        .css_classes(["title-1"])
        .build();
    vbox.append(&heading);

    let subtitle = gtk4::Label::builder()
        .label("Search for a city, address, or landmark to center your project")
        .css_classes(["dim-label"])
        .wrap(true)
        .justify(gtk4::Justification::Center)
        .build();
    vbox.append(&subtitle);

    let search_entry = gtk4::SearchEntry::builder()
        .placeholder_text("Search location…")
        .hexpand(true)
        .build();
    search_entry.add_css_class("setup-search-bar");
    vbox.append(&search_entry);

    let results_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .css_classes(["boxed-list"])
        .build();

    let results_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(300)
        .propagate_natural_height(true)
        .child(&results_list)
        .build();

    let results_revealer = gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::SlideDown)
        .transition_duration(150)
        .reveal_child(false)
        .build();
    results_revealer.set_child(Some(&results_scroll));
    vbox.append(&results_revealer);

    clamp.set_child(Some(&vbox));

    // ── Search logic ──────────────────────────────────────────────────────
    {
        let results = results_list.clone();
        let revealer = results_revealer.clone();
        let main_stack = main_stack.clone();
        let sel_name = selected_name.clone();
        let sel_lat = selected_lat.clone();
        let sel_lng = selected_lng.clone();
        let search = search_entry.clone();

        let timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        search_entry.connect_search_changed(move |entry| {
            let q = entry.text().to_string();

            if let Some(id) = timeout_id.borrow_mut().take() {
                id.remove();
            }

            if q.trim().is_empty() {
                revealer.set_reveal_child(false);
                while let Some(child) = results.first_child() {
                    results.remove(&child);
                }
                return;
            }

            let results = results.clone();
            let revealer = revealer.clone();
            let main_stack = main_stack.clone();
            let sel_name = sel_name.clone();
            let sel_lat = sel_lat.clone();
            let sel_lng = sel_lng.clone();
            let search = search.clone();
            let tid = timeout_id.clone();

            *timeout_id.borrow_mut() = Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(400),
                move || {
                    tid.borrow_mut().take();
                    geocode_suggestions_setup(
                        &q,
                        &results,
                        &revealer,
                        &main_stack,
                        &sel_name,
                        &sel_lat,
                        &sel_lng,
                        &search,
                    );
                },
            ));
        });

        let results2 = results_list.clone();
        search_entry.connect_activate(move |_| {
            if results2.first_child().is_some() {
                if let Some(row) = results2.row_at_index(0) {
                    results2.emit_by_name::<()>("row-activated", &[&row]);
                }
            }
        });
    }

    clamp.upcast()
}

// ═══════════════════════════════════════════════════════════════════════════════
// PROJECT DETAILS PAGE — Step 2: Name, folder, and settings
// ═══════════════════════════════════════════════════════════════════════════════

fn build_project_details_page(
    state: Rc<RefCell<AppState>>,
    map_view: Rc<MapView>,
    main_stack: gtk4::Stack,
    toast: adw::ToastOverlay,
    projects_db: Rc<RefCell<Vec<ProjectMeta>>>,
    _refresh_projects: Rc<dyn Fn(Vec<ProjectMeta>)>,
    recalculate: Rc<dyn Fn()>,
    completed: Rc<RefCell<[bool; 4]>>,
    view_stack: adw::ViewStack,
    selected_name: Rc<RefCell<String>>,
    selected_lat: Rc<std::cell::Cell<f64>>,
    selected_lng: Rc<std::cell::Cell<f64>>,
) -> gtk4::Widget {
    let clamp = adw::Clamp::builder()
        .maximum_size(600)
        .vexpand(true)
        .valign(gtk4::Align::Center)
        .margin_start(24)
        .margin_end(24)
        .build();

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(24)
        .valign(gtk4::Align::Center)
        .build();

    // Location info banner
    let location_icon = gtk4::Image::builder()
        .icon_name("mark-location-symbolic")
        .pixel_size(48)
        .css_classes(["dim-label"])
        .build();
    vbox.append(&location_icon);

    let location_label = gtk4::Label::builder()
        .css_classes(["title-2"])
        .wrap(true)
        .justify(gtk4::Justification::Center)
        .build();
    vbox.append(&location_label);

    let coords_label = gtk4::Label::builder().css_classes(["dim-label"]).build();
    vbox.append(&coords_label);

    // Settings list
    let list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();

    let name_row = adw::EntryRow::builder().title("Project Name").build();
    list.append(&name_row);

    let folder_row = adw::ActionRow::builder()
        .title("Save Location")
        .subtitle(&default_project_parent().to_string_lossy().to_string())
        .activatable(true)
        .build();
    folder_row.add_suffix(
        &gtk4::Image::builder()
            .icon_name("folder-open-symbolic")
            .build(),
    );
    list.append(&folder_row);

    vbox.append(&list);

    let chosen_dir: Rc<RefCell<PathBuf>> = Rc::new(RefCell::new(default_project_parent()));

    // Folder chooser
    {
        let chosen = chosen_dir.clone();
        let row = folder_row.clone();
        folder_row.connect_activated(move |_| {
            let initial = gtk4::gio::File::for_path(&*chosen.borrow());
            let chooser = gtk4::FileDialog::builder()
                .title("Choose Project Location")
                .initial_folder(&initial)
                .build();
            let chosen = chosen.clone();
            let row = row.clone();
            // Need to get the window from the row
            if let Some(win) = row.root().and_downcast::<adw::ApplicationWindow>() {
                chooser.select_folder(Some(&win), gtk4::gio::Cancellable::NONE, move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            row.set_subtitle(&path.to_string_lossy().to_string());
                            *chosen.borrow_mut() = path;
                        }
                    }
                });
            }
        });
    }

    // Continue button
    let continue_btn = gtk4::Button::builder()
        .label("Create Project")
        .css_classes(["suggested-action", "pill"])
        .halign(gtk4::Align::Center)
        .build();
    vbox.append(&continue_btn);

    clamp.set_child(Some(&vbox));

    // ── Update fields when this page becomes visible ──────────────────────
    {
        let sel_name = selected_name.clone();
        let sel_lat = selected_lat.clone();
        let sel_lng = selected_lng.clone();
        let loc_label = location_label.clone();
        let crd_label = coords_label.clone();
        let name_entry = name_row.clone();

        // We use a map notify on the clamp's parent (main_stack) but simpler:
        // just update on every map of the widget
        let clamp_ref = clamp.clone();
        clamp_ref.connect_map(move |_| {
            let name = sel_name.borrow().clone();
            let lat = sel_lat.get();
            let lng = sel_lng.get();

            // Short name for display
            let short = name.split(',').next().unwrap_or("Unknown").trim();
            loc_label.set_label(short);
            crd_label.set_label(&format!("{lat:.5}, {lng:.5}"));

            // Suggest project name from location
            name_entry.set_text(short);
        });
    }

    // ── Continue → create project and open editor ─────────────────────────
    {
        let state = state.clone();
        let main_stack = main_stack.clone();
        let toast = toast.clone();
        let projects_db = projects_db.clone();
        let recalc = recalculate.clone();
        let completed = completed.clone();
        let view_stack = view_stack.clone();
        let map = map_view.clone();
        let sel_name = selected_name.clone();
        let sel_lat = selected_lat.clone();
        let sel_lng = selected_lng.clone();
        let name_row = name_row.clone();
        let chosen_dir = chosen_dir.clone();

        continue_btn.connect_clicked(move |_| {
            let name = name_row.text().to_string();
            if name.trim().is_empty() {
                toast.add_toast(adw::Toast::new("Please enter a project name"));
                return;
            }

            let loc_name = sel_name.borrow().clone();
            let lat = sel_lat.get();
            let lng = sel_lng.get();
            let parent = chosen_dir.borrow().clone();
            let project_dir = parent.join(&name);

            match std::fs::create_dir_all(&project_dir) {
                Ok(()) => {
                    // Initialize state
                    {
                        let mut s = state.borrow_mut();
                        s.project_name = name.clone();
                        s.project_dir = Some(project_dir.clone());
                        s.location_name = loc_name.clone();
                        s.center_lat = lat;
                        s.center_lng = lng;
                        s.polygon.clear();
                        s.home_point = None;
                        s.waypoints.clear();
                    }

                    save_project_state(&state.borrow());

                    // Add to projects DB
                    let now = chrono_now();
                    let meta = ProjectMeta {
                        name: name.clone(),
                        path: project_dir,
                        location_name: loc_name,
                        lat,
                        lng,
                        created: now.clone(),
                        last_modified: now,
                    };
                    projects_db.borrow_mut().push(meta);
                    save_projects(&projects_db.borrow());

                    // Reset steps and open editor
                    *completed.borrow_mut() = [false; 4];
                    refresh_tab_icons(&view_stack, &completed.borrow());
                    view_stack.set_visible_child_name("draw");

                    map.set_center(lat, lng, Some(17));
                    map.clear_all();

                    recalc();
                    main_stack.set_visible_child_name("editor");
                    toast.add_toast(adw::Toast::new(&format!("Project \"{name}\" created")));
                }
                Err(e) => {
                    toast.add_toast(adw::Toast::new(&format!("Failed to create project: {e}")));
                }
            }
        });
    }

    clamp.upcast()
}

/// Geocode suggestions for the setup flow — populates results and navigates
/// to the project-details page when a result is clicked.
fn geocode_suggestions_setup(
    query: &str,
    results_list: &gtk4::ListBox,
    revealer: &gtk4::Revealer,
    main_stack: &gtk4::Stack,
    selected_name: &Rc<RefCell<String>>,
    selected_lat: &Rc<std::cell::Cell<f64>>,
    selected_lng: &Rc<std::cell::Cell<f64>>,
    search_entry: &gtk4::SearchEntry,
) {
    #[derive(Clone)]
    struct GeoResult {
        display_name: String,
        lat: f64,
        lon: f64,
    }

    let url = format!(
        "https://nominatim.openstreetmap.org/search?format=json&q={}&limit=5&addressdetails=1",
        urlencoding::encode(query)
    );
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<GeoResult>>(1);
    std::thread::spawn(move || {
        let results = ureq::get(&url)
            .set("User-Agent", "Wayfarer/1.0")
            .call()
            .ok()
            .and_then(|resp| resp.into_json::<serde_json::Value>().ok())
            .and_then(|json| {
                json.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let display_name = item["display_name"].as_str()?.to_string();
                            let lat = item["lat"].as_str()?.parse::<f64>().ok()?;
                            let lon = item["lon"].as_str()?.parse::<f64>().ok()?;
                            Some(GeoResult {
                                display_name,
                                lat,
                                lon,
                            })
                        })
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        let _ = tx.send(results);
    });

    let results_list = results_list.clone();
    let revealer = revealer.clone();
    let main_stack = main_stack.clone();
    let selected_name = selected_name.clone();
    let selected_lat = selected_lat.clone();
    let selected_lng = selected_lng.clone();
    let search_entry = search_entry.clone();

    glib::idle_add_local(move || {
        use std::sync::mpsc::TryRecvError;
        match rx.try_recv() {
            Ok(items) => {
                while let Some(child) = results_list.first_child() {
                    results_list.remove(&child);
                }

                if items.is_empty() {
                    revealer.set_reveal_child(false);
                    return glib::ControlFlow::Break;
                }

                for item in &items {
                    let row = adw::ActionRow::builder()
                        .title(&item.display_name)
                        .activatable(true)
                        .build();
                    row.add_prefix(&gtk4::Image::from_icon_name("find-location-symbolic"));

                    let lat = item.lat;
                    let lon = item.lon;
                    let name = item.display_name.clone();
                    let rev = revealer.clone();
                    let rl = results_list.clone();
                    let ms = main_stack.clone();
                    let sn = selected_name.clone();
                    let slat = selected_lat.clone();
                    let slng = selected_lng.clone();
                    let se = search_entry.clone();

                    row.connect_activated(move |_| {
                        // Store selection
                        *sn.borrow_mut() = name.clone();
                        slat.set(lat);
                        slng.set(lon);

                        // Hide results & clear search
                        rev.set_reveal_child(false);
                        while let Some(child) = rl.first_child() {
                            rl.remove(&child);
                        }
                        se.set_text("");

                        // Navigate to project details page
                        ms.set_visible_child_name("project-details");
                    });
                    results_list.append(&row);
                }
                revealer.set_reveal_child(true);

                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                revealer.set_reveal_child(false);
                glib::ControlFlow::Break
            }
        }
    });
}

// ─── Refresh tab icons based on completion ────────────────────────────────────

fn refresh_tab_icons(stack: &adw::ViewStack, done: &[bool; 4]) {
    for (i, name) in STEP_NAMES.iter().enumerate() {
        if let Some(child) = stack.child_by_name(name) {
            let page = stack.page(&child);
            if done[i] {
                page.set_icon_name(Some(CHECK_ICON));
            } else {
                page.set_icon_name(Some(STEP_ICONS[i]));
            }
        }
    }
}

fn mark_step_done(
    step: usize,
    completed: &Rc<RefCell<[bool; 4]>>,
    stack: &adw::ViewStack,
    next_step: Option<&str>,
) {
    completed.borrow_mut()[step] = true;
    refresh_tab_icons(stack, &completed.borrow());
    if let Some(name) = next_step {
        stack.set_visible_child_name(name);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TAB 1: Draw Area — full-screen map with controls sidebar (no search bar)
// ═══════════════════════════════════════════════════════════════════════════════

fn build_tab_draw(
    state: Rc<RefCell<AppState>>,
    recalculate: Rc<dyn Fn()>,
    map_view: Rc<MapView>,
    view_stack: adw::ViewStack,
    completed: Rc<RefCell<[bool; 4]>>,
) -> gtk4::Widget {
    let hbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    map_view.widget.set_hexpand(true);
    map_view.widget.set_vexpand(true);

    let map_frame = gtk4::Frame::builder()
        .child(&map_view.widget)
        .hexpand(true)
        .vexpand(true)
        .build();
    map_frame.add_css_class("map-rounded-right");
    map_frame.set_overflow(gtk4::Overflow::Hidden);
    hbox.append(&map_frame);

    // ── Right sidebar ─────────────────────────────────────────────────────
    let sidebar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .build();

    let sidebar_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&sidebar)
        .build();

    let sidebar_content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();

    let title = gtk4::Label::builder()
        .label("Draw Your Area")
        .css_classes(["title-2"])
        .halign(gtk4::Align::Start)
        .build();
    sidebar_content.append(&title);

    let desc = gtk4::Label::builder()
        .label("Click on the map to place polygon vertices.\nRight-click to set a home point.")
        .css_classes(["dim-label"])
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    sidebar_content.append(&desc);

    // Separator
    sidebar_content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    // Toggles
    let toggle_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();

    let camera_row = adw::SwitchRow::builder()
        .title("Photo Points")
        .subtitle("Dense photo triggers")
        .build();
    camera_row.set_active(state.borrow().create_camera_points);

    let show_row = adw::SwitchRow::builder()
        .title("Show Waypoints")
        .subtitle("Markers at each waypoint")
        .build();
    show_row.set_active(state.borrow().show_waypoints);

    let fill_row = adw::SwitchRow::builder()
        .title("Crosshatch")
        .subtitle("Perpendicular second pass")
        .build();
    fill_row.set_active(state.borrow().fill_grid);

    toggle_list.append(&camera_row);
    toggle_list.append(&show_row);
    toggle_list.append(&fill_row);
    sidebar_content.append(&toggle_list);

    sidebar.append(&sidebar_content);

    let continue_btn = gtk4::Button::builder()
        .label("Continue to Aircraft →")
        .css_classes(["suggested-action", "pill"])
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .margin_top(8)
        .build();

    let sidebar_wrapper = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .width_request(300)
        .vexpand(true)
        .build();
    sidebar_wrapper.add_css_class("background");
    sidebar_wrapper.append(&sidebar_scroll);
    sidebar_wrapper.append(&continue_btn);

    hbox.append(&sidebar_wrapper);

    // ── Wire signals ──────────────────────────────────────────────────────

    {
        let state = state.clone();
        let recalc = recalculate.clone();
        camera_row.connect_active_notify(move |row| {
            state.borrow_mut().create_camera_points = row.is_active();
            recalc();
        });
    }

    {
        let state = state.clone();
        let recalc = recalculate.clone();
        show_row.connect_active_notify(move |row| {
            state.borrow_mut().show_waypoints = row.is_active();
            recalc();
        });
    }

    {
        let state = state.clone();
        let recalc = recalculate.clone();
        fill_row.connect_active_notify(move |row| {
            state.borrow_mut().fill_grid = row.is_active();
            recalc();
        });
    }

    {
        let stack = view_stack.clone();
        let done = completed.clone();
        let recalc = recalculate.clone();
        continue_btn.connect_clicked(move |_| {
            recalc();
            mark_step_done(0, &done, &stack, Some("aircraft"));
        });
    }

    hbox.upcast()
}

// ═══════════════════════════════════════════════════════════════════════════════
// TAB 2: Aircraft — full-screen settings
// ═══════════════════════════════════════════════════════════════════════════════

fn build_tab_aircraft(
    state: Rc<RefCell<AppState>>,
    recalculate: Rc<dyn Fn()>,
    view_stack: adw::ViewStack,
    completed: Rc<RefCell<[bool; 4]>>,
) -> gtk4::Widget {
    let clamp = adw::Clamp::builder()
        .maximum_size(700)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(24)
        .build();

    let header = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .build();
    let title = gtk4::Label::builder()
        .label("Aircraft Settings")
        .css_classes(["title-1"])
        .halign(gtk4::Align::Start)
        .build();
    let desc = gtk4::Label::builder()
        .label("Configure your drone's flight parameters, overlap settings, and safety behavior.")
        .css_classes(["dim-label", "step-description"])
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    header.append(&title);
    header.append(&desc);
    vbox.append(&header);

    // ── Flight Parameters ─────────────────────────────────────────────────
    let flight_group = adw::PreferencesGroup::builder()
        .title("Flight Parameters")
        .description("Core flight behavior")
        .build();

    let altitude_row = adw::SpinRow::with_range(10.0, 500.0, 1.0);
    altitude_row.set_title("Altitude");
    altitude_row.set_subtitle("Height above ground (meters)");
    altitude_row.set_value(state.borrow().altitude);
    altitude_row.set_digits(0);
    connect_spin(&altitude_row, state.clone(), recalculate.clone(), |s, v| {
        s.altitude = v;
        if s.ground_offset >= v {
            s.ground_offset = (v - 1.0).max(0.0);
        }
    });

    let ground_offset_row = adw::SpinRow::with_range(0.0, 499.0, 1.0);
    ground_offset_row.set_title("Ground Offset");
    ground_offset_row.set_subtitle("Starting elevation offset (meters)");
    ground_offset_row.set_value(state.borrow().ground_offset);
    ground_offset_row.set_digits(0);
    connect_spin(
        &ground_offset_row,
        state.clone(),
        recalculate.clone(),
        |s, v| {
            s.ground_offset = v;
        },
    );

    let speed_row = adw::SpinRow::with_range(0.1, 9.0, 0.1);
    speed_row.set_title("Flight Speed");
    speed_row.set_subtitle("Cruise speed (m/s)");
    speed_row.set_value(state.borrow().speed);
    speed_row.set_digits(1);
    connect_spin(&speed_row, state.clone(), recalculate.clone(), |s, v| {
        s.speed = v;
    });

    let cam_angle_row = adw::SpinRow::with_range(-90.0, 0.0, 1.0);
    cam_angle_row.set_title("Gimbal Pitch");
    cam_angle_row.set_subtitle("Camera tilt angle (degrees)");
    cam_angle_row.set_value(state.borrow().camera_angle as f64);
    cam_angle_row.set_digits(0);
    connect_spin(
        &cam_angle_row,
        state.clone(),
        recalculate.clone(),
        |s, v| {
            s.camera_angle = v as i32;
        },
    );

    let delay_row = adw::SpinRow::with_range(0.0, 10.0, 0.1);
    delay_row.set_title("Delay at Waypoint");
    delay_row.set_subtitle("Hover time per waypoint (seconds)");
    delay_row.set_value(state.borrow().delay_at_waypoint as f64);
    delay_row.set_digits(1);
    connect_spin(&delay_row, state.clone(), recalculate.clone(), |s, v| {
        s.delay_at_waypoint = v as i32;
    });

    // ── Heading ───────────────────────────────────────────────────────
    let heading_switch = adw::SwitchRow::builder()
        .title("Custom Heading")
        .subtitle("Lock drone nose to a fixed compass bearing")
        .active(state.borrow().heading_angle.is_some())
        .build();

    let heading_spin = adw::SpinRow::with_range(0.0, 359.0, 1.0);
    heading_spin.set_title("Heading Angle");
    heading_spin.set_subtitle("Compass bearing 0°=N, 90°=E, 180°=S, 270°=W");
    heading_spin.set_value(state.borrow().heading_angle.unwrap_or(0) as f64);
    heading_spin.set_digits(0);
    heading_spin.set_sensitive(state.borrow().heading_angle.is_some());

    {
        let heading_spin2 = heading_spin.clone();
        let state2 = state.clone();
        let recalc2 = recalculate.clone();
        heading_switch.connect_active_notify(move |sw| {
            let enabled = sw.is_active();
            heading_spin2.set_sensitive(enabled);
            let mut s = state2.borrow_mut();
            if enabled {
                s.heading_angle = Some(heading_spin2.value() as i32);
            } else {
                s.heading_angle = None;
            }
            drop(s);
            recalc2();
        });
    }
    connect_spin(&heading_spin, state.clone(), recalculate.clone(), |s, v| {
        if s.heading_angle.is_some() {
            s.heading_angle = Some(v as i32);
        }
    });

    flight_group.add(&altitude_row);
    flight_group.add(&ground_offset_row);
    flight_group.add(&speed_row);
    flight_group.add(&cam_angle_row);
    flight_group.add(&delay_row);
    flight_group.add(&heading_switch);
    flight_group.add(&heading_spin);
    vbox.append(&flight_group);

    // ── Overlap & Grid ────────────────────────────────────────────────────
    let overlap_group = adw::PreferencesGroup::builder()
        .title("Overlap &amp; Grid")
        .description("Image overlap and scan pattern")
        .build();

    let forward_row = adw::SpinRow::with_range(1.0, 90.0, 1.0);
    forward_row.set_title("Forward Overlap");
    forward_row.set_subtitle("Overlap between consecutive photos (%)");
    forward_row.set_value(state.borrow().forward_overlap * 100.0);
    forward_row.set_digits(0);
    connect_spin(&forward_row, state.clone(), recalculate.clone(), |s, v| {
        s.forward_overlap = v / 100.0;
    });

    let side_row = adw::SpinRow::with_range(1.0, 90.0, 1.0);
    side_row.set_title("Side Overlap");
    side_row.set_subtitle("Overlap between adjacent flight lines (%)");
    side_row.set_value(state.borrow().side_overlap * 100.0);
    side_row.set_digits(0);
    connect_spin(&side_row, state.clone(), recalculate.clone(), |s, v| {
        s.side_overlap = v / 100.0;
    });

    let rotation_row = adw::SpinRow::with_range(-180.0, 180.0, 1.0);
    rotation_row.set_title("Grid Rotation");
    rotation_row.set_subtitle("Rotate the scan pattern (degrees)");
    rotation_row.set_value(state.borrow().rotation);
    rotation_row.set_digits(0);
    connect_spin(&rotation_row, state.clone(), recalculate.clone(), |s, v| {
        s.rotation = v;
    });

    overlap_group.add(&forward_row);
    overlap_group.add(&side_row);
    overlap_group.add(&rotation_row);
    vbox.append(&overlap_group);

    // ── Safety ────────────────────────────────────────────────────────────
    let safety_group = adw::PreferencesGroup::builder()
        .title("Mission Safety")
        .description("Behavior on mission end or signal loss")
        .build();

    let finish_model = gtk4::StringList::new(&[]);
    for fa in FinishAction::all() {
        finish_model.append(fa.label());
    }
    let finish_row = adw::ComboRow::builder()
        .title("On Finished")
        .subtitle("Action when mission completes")
        .model(&finish_model)
        .build();
    let cur_finish = FinishAction::all()
        .iter()
        .position(|&f| f == state.borrow().finish_action)
        .unwrap_or(0) as u32;
    finish_row.set_selected(cur_finish);
    {
        let state = state.clone();
        let recalc = recalculate.clone();
        finish_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some(&fa) = FinishAction::all().get(idx) {
                state.borrow_mut().finish_action = fa;
                recalc();
            }
        });
    }

    let rclost_model = gtk4::StringList::new(&[]);
    for ra in RcLostAction::all() {
        rclost_model.append(ra.label());
    }
    let rclost_row = adw::ComboRow::builder()
        .title("On Signal Loss")
        .subtitle("Action when RC connection is lost")
        .model(&rclost_model)
        .build();
    let cur_rc = RcLostAction::all()
        .iter()
        .position(|&r| r == state.borrow().rc_lost_action)
        .unwrap_or(0) as u32;
    rclost_row.set_selected(cur_rc);
    {
        let state = state.clone();
        let recalc = recalculate.clone();
        rclost_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            if let Some(&ra) = RcLostAction::all().get(idx) {
                state.borrow_mut().rc_lost_action = ra;
                recalc();
            }
        });
    }

    let terrain_row = adw::SwitchRow::builder()
        .title("Terrain Following")
        .subtitle("Adjust altitude based on elevation data")
        .build();
    terrain_row.set_active(state.borrow().terrain_following);
    {
        let state = state.clone();
        let recalc = recalculate.clone();
        terrain_row.connect_active_notify(move |row| {
            state.borrow_mut().terrain_following = row.is_active();
            recalc();
        });
    }

    safety_group.add(&finish_row);
    safety_group.add(&rclost_row);
    safety_group.add(&terrain_row);
    vbox.append(&safety_group);

    let continue_btn = gtk4::Button::builder()
        .label("Continue to Camera →")
        .css_classes(["suggested-action", "pill"])
        .halign(gtk4::Align::End)
        .margin_top(8)
        .build();
    {
        let stack = view_stack.clone();
        let done = completed.clone();
        let recalc = recalculate.clone();
        continue_btn.connect_clicked(move |_| {
            recalc();
            mark_step_done(1, &done, &stack, Some("camera"));
        });
    }
    vbox.append(&continue_btn);

    clamp.set_child(Some(&vbox));

    let scroll = gtk4::ScrolledWindow::builder()
        .child(&clamp)
        .vexpand(true)
        .hexpand(true)
        .build();
    scroll.upcast()
}

// ═══════════════════════════════════════════════════════════════════════════════
// TAB 3: Camera — full-screen settings
// ═══════════════════════════════════════════════════════════════════════════════

fn build_tab_camera(
    state: Rc<RefCell<AppState>>,
    recalculate: Rc<dyn Fn()>,
    view_stack: adw::ViewStack,
    completed: Rc<RefCell<[bool; 4]>>,
) -> gtk4::Widget {
    let clamp = adw::Clamp::builder()
        .maximum_size(700)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(24)
        .build();

    let header = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .build();
    let title = gtk4::Label::builder()
        .label("Camera Settings")
        .css_classes(["title-1"])
        .halign(gtk4::Align::Start)
        .build();
    let desc = gtk4::Label::builder()
        .label("Select a camera preset or enter your sensor details manually for accurate GSD calculation.")
        .css_classes(["dim-label", "step-description"])
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    header.append(&title);
    header.append(&desc);
    vbox.append(&header);

    // ── Preset ────────────────────────────────────────────────────────────
    let preset_group = adw::PreferencesGroup::builder()
        .title("Camera Preset")
        .description("Choose a built-in camera or define your own")
        .build();

    let preset_names: Vec<String> = state
        .borrow()
        .presets
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let preset_names_ref: Vec<&str> = preset_names.iter().map(|s| s.as_str()).collect();
    let model = gtk4::StringList::new(&preset_names_ref);

    let combo_row = adw::ComboRow::builder()
        .title("Preset")
        .subtitle("Select your drone's camera")
        .model(&model)
        .build();
    combo_row.set_selected(state.borrow().selected_preset_idx as u32);
    preset_group.add(&combo_row);
    vbox.append(&preset_group);

    // ── Sensor Details ────────────────────────────────────────────────────
    let sensor_group = adw::PreferencesGroup::builder()
        .title("Sensor Details")
        .description("Physical sensor dimensions and optics")
        .build();

    let sw_row = adw::SpinRow::with_range(1.0, 50.0, 0.1);
    sw_row.set_title("Sensor Width");
    sw_row.set_subtitle("Physical width (mm)");
    sw_row.set_digits(1);
    sw_row.set_value(state.borrow().sensor_width);

    let sh_row = adw::SpinRow::with_range(1.0, 50.0, 0.1);
    sh_row.set_title("Sensor Height");
    sh_row.set_subtitle("Physical height (mm)");
    sh_row.set_digits(1);
    sh_row.set_value(state.borrow().sensor_height);

    let fl_row = adw::SpinRow::with_range(1.0, 200.0, 0.01);
    fl_row.set_title("Focal Length");
    fl_row.set_subtitle("Lens focal length (mm)");
    fl_row.set_digits(2);
    fl_row.set_value(state.borrow().focal_length);

    sensor_group.add(&sw_row);
    sensor_group.add(&sh_row);
    sensor_group.add(&fl_row);
    vbox.append(&sensor_group);

    // ── Resolution ────────────────────────────────────────────────────────
    let res_group = adw::PreferencesGroup::builder()
        .title("Image Resolution")
        .description("Output photo dimensions")
        .build();

    let iw_row = adw::SpinRow::with_range(300.0, 20000.0, 1.0);
    iw_row.set_title("Image Width");
    iw_row.set_subtitle("Horizontal pixels");
    iw_row.set_digits(0);
    iw_row.set_value(state.borrow().image_width as f64);

    let ih_row = adw::SpinRow::with_range(300.0, 20000.0, 1.0);
    ih_row.set_title("Image Height");
    ih_row.set_subtitle("Vertical pixels");
    ih_row.set_digits(0);
    ih_row.set_value(state.borrow().image_height as f64);

    res_group.add(&iw_row);
    res_group.add(&ih_row);
    vbox.append(&res_group);

    connect_spin(&sw_row, state.clone(), recalculate.clone(), |s, v| {
        s.sensor_width = v
    });
    connect_spin(&sh_row, state.clone(), recalculate.clone(), |s, v| {
        s.sensor_height = v
    });
    connect_spin(&fl_row, state.clone(), recalculate.clone(), |s, v| {
        s.focal_length = v
    });
    connect_spin(&iw_row, state.clone(), recalculate.clone(), |s, v| {
        s.image_width = v as i32
    });
    connect_spin(&ih_row, state.clone(), recalculate.clone(), |s, v| {
        s.image_height = v as i32
    });

    let refresh_fields = {
        let sw = sw_row.clone();
        let sh = sh_row.clone();
        let fl = fl_row.clone();
        let iw = iw_row.clone();
        let ih = ih_row.clone();
        move |preset: &CameraPreset| {
            sw.set_value(preset.sensor_width);
            sh.set_value(preset.sensor_height);
            fl.set_value(preset.focal_length);
            iw.set_value(preset.image_width as f64);
            ih.set_value(preset.image_height as f64);
            let editable = !preset.default_preset;
            sw.set_sensitive(editable);
            sh.set_sensitive(editable);
            fl.set_sensitive(editable);
            iw.set_sensitive(editable);
            ih.set_sensitive(editable);
        }
    };

    {
        let preset = state.borrow().active_preset().cloned();
        if let Some(p) = preset {
            refresh_fields(&p);
        }
    }

    {
        let state = state.clone();
        let recalc = recalculate.clone();
        let refresh = refresh_fields.clone();
        combo_row.connect_selected_notify(move |row| {
            let idx = row.selected() as usize;
            let preset = {
                let mut s = state.borrow_mut();
                s.selected_preset_idx = idx;
                s.presets.get(idx).cloned()
            };
            if let Some(p) = preset {
                state.borrow_mut().apply_preset(&p);
                refresh(&p);
                recalc();
            }
        });
    }

    let continue_btn = gtk4::Button::builder()
        .label("Continue to Review →")
        .css_classes(["suggested-action", "pill"])
        .halign(gtk4::Align::End)
        .margin_top(8)
        .build();
    {
        let stack = view_stack.clone();
        let done = completed.clone();
        let recalc = recalculate.clone();
        continue_btn.connect_clicked(move |_| {
            recalc();
            mark_step_done(2, &done, &stack, Some("review"));
        });
    }
    vbox.append(&continue_btn);

    clamp.set_child(Some(&vbox));

    let scroll = gtk4::ScrolledWindow::builder()
        .child(&clamp)
        .vexpand(true)
        .hexpand(true)
        .build();
    scroll.upcast()
}

// ═══════════════════════════════════════════════════════════════════════════════
// TAB 4: Review & Export
// ═══════════════════════════════════════════════════════════════════════════════

fn build_tab_review(
    state: Rc<RefCell<AppState>>,
    recalculate: Rc<dyn Fn()>,
    toast_overlay: adw::ToastOverlay,
    stats_cell: Rc<RefCell<Option<StatsLabels>>>,
    completed: Rc<RefCell<[bool; 4]>>,
    view_stack: adw::ViewStack,
) -> gtk4::Widget {
    let clamp = adw::Clamp::builder()
        .maximum_size(700)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(24)
        .build();

    let header_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .build();
    let title = gtk4::Label::builder()
        .label("Review & Export")
        .css_classes(["title-1"])
        .halign(gtk4::Align::Start)
        .build();
    let desc = gtk4::Label::builder()
        .label("Review your mission statistics and upload directly to your controller.")
        .css_classes(["dim-label", "step-description"])
        .halign(gtk4::Align::Start)
        .wrap(true)
        .build();
    header_box.append(&title);
    header_box.append(&desc);
    vbox.append(&header_box);

    // ── Statistics ────────────────────────────────────────────────────────
    let stats_group = adw::PreferencesGroup::builder()
        .title("Mission Statistics")
        .description("Calculated from your area, altitude, and camera")
        .build();

    fn stat_row(title: &str) -> (adw::ActionRow, gtk4::Label) {
        let label = gtk4::Label::builder()
            .label("—")
            .css_classes(["stat-value"])
            .halign(gtk4::Align::End)
            .build();
        let row = adw::ActionRow::builder().title(title).build();
        row.add_suffix(&label);
        row.set_activatable(false);
        (row, label)
    }

    let (r_wp, l_wp) = stat_row("Waypoints");
    let (r_dist, l_dist) = stat_row("Flight Distance");
    let (r_area, l_area) = stat_row("Coverage Area");
    let (r_time, l_time) = stat_row("Estimated Flight Time");
    let (r_gsd, l_gsd) = stat_row("Ground Sampling Distance");
    let (r_shut, l_shut) = stat_row("Recommended Shutter Speed");
    let (r_int, l_int) = stat_row("Photo Interval");

    stats_group.add(&r_wp);
    stats_group.add(&r_dist);
    stats_group.add(&r_area);
    stats_group.add(&r_time);
    stats_group.add(&r_gsd);
    stats_group.add(&r_shut);
    stats_group.add(&r_int);
    vbox.append(&stats_group);

    let labels = StatsLabels {
        waypoints: l_wp,
        distance: l_dist,
        area: l_area,
        time: l_time,
        gsd: l_gsd,
        shutter: l_shut,
        interval: l_int,
    };
    *stats_cell.borrow_mut() = Some(labels);

    // ── Upload to Controller ──────────────────────────────────────────────
    let ctrl_group = adw::PreferencesGroup::builder()
        .title("Upload to Controller")
        .description("Connect your DJI controller via USB to upload directly")
        .build();

    let instructions_row = adw::ActionRow::builder()
        .title("First time setup required")
        .subtitle(
            "No waypoint mission found on controller.\n\n\
             1. Open DJI Fly on your controller\n\
             2. Create a simple 1-waypoint mission\n\
             3. Save the mission and quit DJI Fly\n\
             4. Connect the controller to your PC via USB-C\n\n\
             This creates a mission slot that the app will reuse for all future uploads.",
        )
        .activatable(false)
        .visible(false)
        .build();
    instructions_row.add_prefix(
        &gtk4::Image::builder()
            .icon_name("dialog-warning-symbolic")
            .build(),
    );
    ctrl_group.add(&instructions_row);

    let ctrl_status_label = gtk4::Label::builder()
        .label("Scanning…")
        .css_classes(["dim-label"])
        .halign(gtk4::Align::End)
        .build();

    let ctrl_icon = gtk4::Image::builder()
        .icon_name("network-wireless-offline-symbolic")
        .build();

    let ctrl_status_row = adw::ActionRow::builder()
        .title("Controller")
        .subtitle("Looking for a DJI controller…")
        .activatable(false)
        .build();
    ctrl_status_row.add_prefix(&ctrl_icon);
    ctrl_status_row.add_suffix(&ctrl_status_label);
    ctrl_group.add(&ctrl_status_row);

    let upload_btn = gtk4::Button::builder()
        .label("Upload Mission")
        .css_classes(["suggested-action", "pill"])
        .halign(gtk4::Align::Center)
        .sensitive(false)
        .build();

    let upload_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    upload_box.append(&upload_btn);
    ctrl_group.add(&adw::PreferencesRow::builder().child(&upload_box).build());
    vbox.append(&ctrl_group);

    let detected_controller: Rc<RefCell<Option<controller::DjiController>>> =
        Rc::new(RefCell::new(None));
    let mission_ready: Rc<std::cell::Cell<bool>> = Rc::new(std::cell::Cell::new(false));

    let do_scan = {
        let ctrl_status_label = ctrl_status_label.clone();
        let ctrl_icon = ctrl_icon.clone();
        let ctrl_status_row = ctrl_status_row.clone();
        let detected = detected_controller.clone();
        let ready = mission_ready.clone();
        let upload_btn = upload_btn.clone();
        let instructions_row = instructions_row.clone();
        Rc::new(move || {
            let controllers = controller::detect_controllers();
            if let Some(ctrl) = controllers.into_iter().next() {
                ctrl_status_label.set_label(&ctrl.name);
                ctrl_icon.set_icon_name(Some("network-wireless-symbolic"));
                if controller::has_existing_mission(&ctrl) {
                    ctrl_status_row.set_subtitle("Connected — ready to upload");
                    instructions_row.set_visible(false);
                    ready.set(true);
                    upload_btn.set_sensitive(true);
                } else {
                    ctrl_status_row.set_subtitle("Connected — no mission found");
                    instructions_row.set_visible(true);
                    ready.set(false);
                    upload_btn.set_sensitive(false);
                }
                *detected.borrow_mut() = Some(ctrl);
            } else {
                ctrl_status_label.set_label("Not connected");
                ctrl_icon.set_icon_name(Some("network-wireless-offline-symbolic"));
                ctrl_status_row.set_subtitle("Connect a DJI RC via USB-C");
                instructions_row.set_visible(false);
                ready.set(false);
                upload_btn.set_sensitive(false);
                *detected.borrow_mut() = None;
            }
        })
    };

    do_scan();
    {
        let do_scan = do_scan.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(2), move || {
            do_scan();
            glib::ControlFlow::Continue
        });
    }

    // Wire upload button
    {
        let state = state.clone();
        let toast = toast_overlay.clone();
        let done = completed.clone();
        let stack = view_stack.clone();
        let detected_for_upload = detected_controller.clone();
        let do_scan2 = do_scan.clone();

        upload_btn.connect_clicked(move |btn| {
            let win: adw::ApplicationWindow = btn.root().and_downcast().unwrap();
            do_scan2();
            let det = detected_for_upload.borrow();
            let ctrl = match det.as_ref() {
                Some(c) => c.clone(),
                None => {
                    toast.add_toast(adw::Toast::new("Controller disconnected. Reconnect and try again."));
                    return;
                }
            };
            drop(det);

            if !mission_ready.get() {
                toast.add_toast(adw::Toast::new("No mission found on controller. Complete first-time setup first."));
                return;
            }

            let s = state.borrow();
            if s.waypoints.is_empty() {
                toast.add_toast(adw::Toast::new("Draw an area first to generate waypoints."));
                return;
            }

            let kmz = match dji::generate_kmz(
                &s.waypoints,
                s.altitude,
                s.speed,
                s.camera_angle,
                s.delay_at_waypoint,
                s.heading_angle,
                s.finish_action,
                s.rc_lost_action,
            ) {
                Ok(b) => b,
                Err(e) => {
                    toast.add_toast(adw::Toast::new(&format!("Export error: {e}")));
                    return;
                }
            };
            drop(s);

            let dialog = adw::AlertDialog::builder()
                .heading("Overwrite mission on controller?")
                .body("This will replace the most recent waypoint mission saved on your controller. The original mission cannot be recovered.")
                .close_response("cancel")
                .default_response("upload")
                .build();
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("upload", "Upload");
            dialog.set_response_appearance("upload", adw::ResponseAppearance::Destructive);

            let btn = btn.clone();
            let toast = toast.clone();
            let done = done.clone();
            let stack = stack.clone();
            let ctrl = Rc::new(ctrl);
            let kmz = Rc::new(kmz);
            dialog.connect_response(None, move |_dialog, response| {
                if response != "upload" {
                    return;
                }

                let btn = btn.clone();
                let toast = toast.clone();
                let done = done.clone();
                let stack = stack.clone();

                btn.set_sensitive(false);
                btn.set_label("Uploading…");
                btn.remove_css_class("suggested-action");
                btn.add_css_class("upload-progress");

                let progress_css = gtk4::CssProvider::new();
                gtk4::style_context_add_provider_for_display(
                    &gtk4::prelude::WidgetExt::display(&btn),
                    &progress_css,
                    gtk4::STYLE_PROVIDER_PRIORITY_USER,
                );

                let tick: Rc<std::cell::Cell<u32>> = Rc::new(std::cell::Cell::new(0));
                const DURATION_MS: u32 = 3000;
                const INTERVAL_MS: u32 = 16;
                const TOTAL_TICKS: u32 = DURATION_MS / INTERVAL_MS;

                let btn2 = btn.clone();
                let toast2 = toast.clone();
                let done2 = done.clone();
                let stack2 = stack.clone();
                let progress_css2 = progress_css.clone();
                let ctrl2 = ctrl.clone();
                let kmz2 = kmz.clone();
                glib::timeout_add_local(std::time::Duration::from_millis(INTERVAL_MS as u64), move || {
                    let t = tick.get() + 1;
                    tick.set(t);

                    let pct = ((t as f64 / TOTAL_TICKS as f64) * 100.0).min(100.0);
                    progress_css2.load_from_string(&format!(
                        ".upload-progress {{ --progress: {pct:.1}%; }}"
                    ));

                    if t >= TOTAL_TICKS {
                        btn2.remove_css_class("upload-progress");
                        btn2.add_css_class("suggested-action");
                        gtk4::style_context_remove_provider_for_display(
                            &gtk4::prelude::WidgetExt::display(&btn2),
                            &progress_css2,
                        );

                        match controller::upload_mission(&ctrl2, &kmz2) {
                            Ok(_path) => {
                                btn2.set_label("Uploaded ✓");
                                toast2.add_toast(adw::Toast::new(
                                    "Mission uploaded to controller successfully",
                                ));
                                mark_step_done(3, &done2, &stack2, None);
                            }
                            Err(e) => {
                                btn2.set_label("Upload Failed");
                                toast2.add_toast(adw::Toast::new(&e));
                            }
                        }

                        let btn3 = btn2.clone();
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(2000),
                            move || {
                                btn3.set_label("Upload Mission");
                                btn3.set_sensitive(true);
                            },
                        );

                        return glib::ControlFlow::Break;
                    }

                    glib::ControlFlow::Continue
                });
            });

            dialog.present(Some(&win));
        });
    }

    // ── Other Export Options ───────────────────────────────────────────────
    let other_group = adw::PreferencesGroup::builder()
        .title("Other Export Options")
        .description("Export to file for manual transfer")
        .build();

    let other_expander = adw::ExpanderRow::builder()
        .title("File exports")
        .subtitle("DJI KMZ, Litchi CSV, KML boundary")
        .show_enable_switch(false)
        .build();
    other_expander.add_prefix(
        &gtk4::Image::builder()
            .icon_name("folder-documents-symbolic")
            .build(),
    );

    let dji_row = adw::ActionRow::builder()
        .title("Export DJI Waypoint Mission")
        .subtitle("Generates a .kmz file in WPML format")
        .activatable(true)
        .build();
    dji_row.add_prefix(
        &gtk4::Image::builder()
            .icon_name("airplane-mode-symbolic")
            .build(),
    );
    dji_row.add_suffix(&gtk4::Image::builder().icon_name("go-next-symbolic").build());

    let litchi_row = adw::ActionRow::builder()
        .title("Export Litchi Mission")
        .subtitle("Generates a .csv for Litchi Mission Hub")
        .activatable(true)
        .build();
    litchi_row.add_prefix(
        &gtk4::Image::builder()
            .icon_name("document-send-symbolic")
            .build(),
    );
    litchi_row.add_suffix(&gtk4::Image::builder().icon_name("go-next-symbolic").build());

    let kml_export_row = adw::ActionRow::builder()
        .title("Export Boundary KML")
        .subtitle("Save polygon boundary as .kml")
        .activatable(true)
        .build();
    kml_export_row.add_prefix(
        &gtk4::Image::builder()
            .icon_name("mark-location-symbolic")
            .build(),
    );
    kml_export_row.add_suffix(&gtk4::Image::builder().icon_name("go-next-symbolic").build());

    let kml_import_row = adw::ActionRow::builder()
        .title("Import Boundary KML")
        .subtitle("Load polygon from an existing .kml file")
        .activatable(true)
        .build();
    kml_import_row.add_prefix(
        &gtk4::Image::builder()
            .icon_name("document-open-symbolic")
            .build(),
    );
    kml_import_row.add_suffix(&gtk4::Image::builder().icon_name("go-next-symbolic").build());

    other_expander.add_row(&dji_row);
    other_expander.add_row(&litchi_row);
    other_expander.add_row(&kml_export_row);
    other_expander.add_row(&kml_import_row);
    other_group.add(&other_expander);
    vbox.append(&other_group);

    // Wire exports
    {
        let state = state.clone();
        let toast = toast_overlay.clone();
        let done = completed.clone();
        let stack = view_stack.clone();
        dji_row.connect_activated(move |_| {
            let s = state.borrow();
            if s.waypoints.is_empty() {
                toast.add_toast(adw::Toast::new("Draw an area first to generate waypoints."));
                return;
            }
            let kmz = match dji::generate_kmz(
                &s.waypoints,
                s.altitude,
                s.speed,
                s.camera_angle,
                s.delay_at_waypoint,
                s.heading_angle,
                s.finish_action,
                s.rc_lost_action,
            ) {
                Ok(b) => b,
                Err(e) => {
                    toast.add_toast(adw::Toast::new(&format!("Export error: {e}")));
                    return;
                }
            };
            if let Some(ref dir) = s.project_dir {
                let path = dir.join("mission.kmz");
                drop(s);
                match std::fs::write(&path, &kmz) {
                    Ok(()) => {
                        toast.add_toast(adw::Toast::new(&format!("Saved to {}", path.display())));
                        mark_step_done(3, &done, &stack, None);
                    }
                    Err(e) => toast.add_toast(adw::Toast::new(&format!("Save failed: {e}"))),
                }
            } else {
                drop(s);
                toast.add_toast(adw::Toast::new("Create a project first."));
            }
        });
    }

    {
        let state = state.clone();
        let toast = toast_overlay.clone();
        let done = completed.clone();
        let stack = view_stack.clone();
        litchi_row.connect_activated(move |_| {
            let s = state.borrow();
            if s.waypoints.is_empty() {
                toast.add_toast(adw::Toast::new("Draw an area first to generate waypoints."));
                return;
            }
            let csv = litchi::generate_csv(&s.waypoints, s.altitude, s.speed, s.camera_angle);
            if let Some(ref dir) = s.project_dir {
                let path = dir.join("mission.csv");
                drop(s);
                match std::fs::write(&path, csv.as_bytes()) {
                    Ok(()) => {
                        toast.add_toast(adw::Toast::new(&format!("Saved to {}", path.display())));
                        mark_step_done(3, &done, &stack, None);
                    }
                    Err(e) => toast.add_toast(adw::Toast::new(&format!("Save failed: {e}"))),
                }
            } else {
                drop(s);
                toast.add_toast(adw::Toast::new("Create a project first."));
            }
        });
    }

    {
        let state = state.clone();
        let toast = toast_overlay.clone();
        kml_export_row.connect_activated(move |_| {
            let s = state.borrow();
            if s.polygon.len() < 3 {
                toast.add_toast(adw::Toast::new("Draw at least 3 boundary points first."));
                return;
            }
            let kml = build_polygon_kml(&s.polygon);
            if let Some(ref dir) = s.project_dir {
                let path = dir.join("boundary.kml");
                drop(s);
                match std::fs::write(&path, kml.as_bytes()) {
                    Ok(()) => {
                        toast.add_toast(adw::Toast::new(&format!("Saved to {}", path.display())))
                    }
                    Err(e) => toast.add_toast(adw::Toast::new(&format!("Save failed: {e}"))),
                }
            } else {
                drop(s);
                toast.add_toast(adw::Toast::new("Create a project first."));
            }
        });
    }

    {
        let state = state.clone();
        let toast = toast_overlay.clone();
        let recalc = recalculate.clone();
        kml_import_row.connect_activated(move |_| {
            let state = state.clone();
            let toast = toast.clone();
            let recalc = recalc.clone();

            let dialog = gtk4::FileDialog::builder()
                .title("Import KML Boundary")
                .build();
            let filter = gtk4::FileFilter::new();
            filter.add_pattern("*.kml");
            filter.set_name(Some("KML files"));
            let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));

            dialog.open(
                gtk4::Window::NONE,
                gtk4::gio::Cancellable::NONE,
                move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            match std::fs::read_to_string(&path) {
                                Ok(content) => {
                                    if let Some(polygon) = parse_kml_polygon(&content) {
                                        state.borrow_mut().polygon = polygon;
                                        recalc();
                                        toast.add_toast(adw::Toast::new(
                                            "Boundary imported successfully.",
                                        ));
                                    } else {
                                        toast.add_toast(adw::Toast::new(
                                            "No valid polygon found in KML.",
                                        ));
                                    }
                                }
                                Err(e) => {
                                    toast.add_toast(adw::Toast::new(&format!("Read error: {e}")));
                                }
                            }
                        }
                    }
                },
            );
        });
    }

    {
        let recalc = recalculate.clone();
        let do_scan = do_scan.clone();
        vbox.connect_map(move |_| {
            recalc();
            do_scan();
        });
    }

    clamp.set_child(Some(&vbox));

    let scroll = gtk4::ScrolledWindow::builder()
        .child(&clamp)
        .vexpand(true)
        .hexpand(true)
        .build();
    scroll.upcast()
}

// ─── Stats labels helper ──────────────────────────────────────────────────────

struct StatsLabels {
    waypoints: gtk4::Label,
    distance: gtk4::Label,
    area: gtk4::Label,
    time: gtk4::Label,
    gsd: gtk4::Label,
    shutter: gtk4::Label,
    interval: gtk4::Label,
}

impl StatsLabels {
    fn update(&self, stats: &MissionStats) {
        self.waypoints.set_label(&stats.waypoint_count.to_string());
        self.distance
            .set_label(&format!("{:.0} m", stats.flight_distance_m));
        self.area.set_label(&format_area(stats.area_m2));
        self.time
            .set_label(&format!("{:.1} min", stats.estimated_time_min));
        self.gsd.set_label(&format!("{:.1} cm/px", stats.gsd_cm));
        self.shutter.set_label(&stats.recommended_shutter);
        self.interval
            .set_label(&format!("{:.1} s", stats.photo_interval_s));
    }
}

fn format_area(m2: f64) -> String {
    if m2 >= 10_000.0 {
        format!("{:.2} ha", m2 / 10_000.0)
    } else {
        format!("{:.0} m²", m2)
    }
}

// ─── Spin row helper ──────────────────────────────────────────────────────────

fn connect_spin<F>(
    row: &adw::SpinRow,
    state: Rc<RefCell<AppState>>,
    recalculate: Rc<dyn Fn()>,
    setter: F,
) where
    F: Fn(&mut AppState, f64) + 'static,
{
    row.connect_value_notify(move |r| {
        setter(&mut *state.borrow_mut(), r.value());
        recalculate();
    });
}

// ─── Map message handler ──────────────────────────────────────────────────────

fn handle_map_message(
    msg: MapMessage,
    state: &Rc<RefCell<AppState>>,
    _toast: &adw::ToastOverlay,
    recalc_slot: &Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) {
    match msg {
        MapMessage::Ready => {}
        MapMessage::PolygonChanged(pts) => {
            let count = pts.len();
            state.borrow_mut().polygon = pts;
            if count >= 3 {
                if let Some(recalc) = recalc_slot.borrow().as_ref() {
                    recalc();
                }
            }
        }
        MapMessage::HomeChanged(ll) => {
            state.borrow_mut().home_point = Some(ll);
        }
        MapMessage::HomeRemoved => {
            state.borrow_mut().home_point = None;
        }
        MapMessage::Cleared => {
            let mut s = state.borrow_mut();
            s.polygon.clear();
            s.home_point = None;
            s.waypoints.clear();
        }
    }
}

// ─── Geocoding ────────────────────────────────────────────────────────────────

fn geocode_and_move(map: &Rc<MapView>, query: &str) {
    let url = format!(
        "https://nominatim.openstreetmap.org/search?format=json&q={}&limit=1",
        urlencoding::encode(query)
    );
    let (tx, rx) = std::sync::mpsc::sync_channel::<Option<(f64, f64)>>(1);
    std::thread::spawn(move || {
        let coords = ureq::get(&url)
            .set("User-Agent", "Wayfarer/1.0")
            .call()
            .ok()
            .and_then(|resp| resp.into_json::<serde_json::Value>().ok())
            .and_then(|json| json.as_array()?.first().cloned())
            .and_then(|first| {
                let lat = first["lat"].as_str()?.parse::<f64>().ok()?;
                let lon = first["lon"].as_str()?.parse::<f64>().ok()?;
                Some((lat, lon))
            });
        let _ = tx.send(coords);
    });
    let map = map.clone();
    glib::idle_add_local(move || {
        use std::sync::mpsc::TryRecvError;
        match rx.try_recv() {
            Ok(Some((lat, lon))) => {
                map.set_center(lat, lon, Some(17));
                glib::ControlFlow::Break
            }
            Ok(None) => glib::ControlFlow::Break,
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

// ─── KML helpers ──────────────────────────────────────────────────────────────

fn build_polygon_kml(polygon: &[crate::engine::mapping::LatLng]) -> String {
    let coords: String = polygon
        .iter()
        .map(|p| format!("{:.7},{:.7},0", p.lng, p.lat))
        .collect::<Vec<_>>()
        .join("\n      ");
    let first = polygon.first().unwrap();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2">
  <Placemark>
    <name>Wayfarer Boundary</name>
    <Polygon>
      <outerBoundaryIs>
        <LinearRing>
          <coordinates>
      {coords}
      {first_lng:.7},{first_lat:.7},0
          </coordinates>
        </LinearRing>
      </outerBoundaryIs>
    </Polygon>
  </Placemark>
</kml>"#,
        coords = coords,
        first_lng = first.lng,
        first_lat = first.lat,
    )
}

fn parse_kml_polygon(kml: &str) -> Option<Vec<crate::engine::mapping::LatLng>> {
    let start = kml.find("<coordinates>")?;
    let end = kml[start..].find("</coordinates>")?;
    let block = &kml[start + "<coordinates>".len()..start + end];
    let points: Vec<crate::engine::mapping::LatLng> = block
        .split_whitespace()
        .filter_map(|tok| {
            let parts: Vec<&str> = tok.split(',').collect();
            if parts.len() >= 2 {
                let lng = parts[0].parse::<f64>().ok()?;
                let lat = parts[1].parse::<f64>().ok()?;
                Some(crate::engine::mapping::LatLng { lat, lng })
            } else {
                None
            }
        })
        .collect();
    if points.len() >= 3 {
        Some(points)
    } else {
        None
    }
}

// ─── URL encoding ─────────────────────────────────────────────────────────────

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }
}
