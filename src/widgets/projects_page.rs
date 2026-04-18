//! Projects landing page — shows saved projects in a grid of cards.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::prelude::*;
use gdk4 as gdk;

use crate::models::mission::ProjectMeta;
use crate::engine::mapping::LatLng;

// ─── Public interface ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ProjectAction {
    Open(ProjectMeta),
    Delete(ProjectMeta),
}

/// Build the projects landing page.
/// Returns (widget, refresh_callback).
pub fn build_projects_page(
    on_action: Rc<dyn Fn(ProjectAction)>,
) -> (gtk4::Widget, Rc<dyn Fn(Vec<ProjectMeta>)>) {
    let outer = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .build();

    // Empty state
    let status_page = adw::StatusPage::builder()
        .icon_name("airplane-mode-symbolic")
        .title("No Projects Yet")
        .description("Create a new project to start mapping")
        .vexpand(true)
        .build();

    // Scrollable grid of project cards
    let scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let clamp = adw::Clamp::builder()
        .maximum_size(900)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let flow = gtk4::FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .homogeneous(false)
        .column_spacing(16)
        .row_spacing(16)
        .min_children_per_line(2)
        .max_children_per_line(4)
        .valign(gtk4::Align::Start)
        .build();

    clamp.set_child(Some(&flow));
    scroll.set_child(Some(&clamp));

    // Stack: either status page or card grid
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.add_named(&status_page, Some("empty"));
    stack.add_named(&scroll, Some("grid"));

    outer.append(&stack);

    // ── Bottom action bar (separator + Open button) ──────────────────────
    let action_bar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .build();
    action_bar.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let bar_inner = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_start(24)
        .margin_end(24)
        .margin_top(12)
        .margin_bottom(12)
        .halign(gtk4::Align::End)
        .build();

    let open_btn = gtk4::Button::builder()
        .label("Open Project")
        .css_classes(["suggested-action", "pill"])
        .sensitive(false)
        .build();
    bar_inner.append(&open_btn);

    action_bar.append(&bar_inner);
    outer.append(&action_bar);

    // ── Track selected project ────────────────────────────────────────────
    let selected_meta: Rc<RefCell<Option<ProjectMeta>>> = Rc::new(RefCell::new(None));
    // Store project list alongside flow for index lookup
    let project_list: Rc<RefCell<Vec<ProjectMeta>>> = Rc::new(RefCell::new(Vec::new()));

    {
        let selected = selected_meta.clone();
        let projects = project_list.clone();
        let btn = open_btn.clone();
        flow.connect_selected_children_changed(move |flow| {
            let children = flow.selected_children();
            if let Some(child) = children.first() {
                let idx = child.index() as usize;
                let list = projects.borrow();
                if let Some(meta) = list.get(idx) {
                    *selected.borrow_mut() = Some(meta.clone());
                    btn.set_sensitive(true);
                } else {
                    *selected.borrow_mut() = None;
                    btn.set_sensitive(false);
                }
            } else {
                *selected.borrow_mut() = None;
                btn.set_sensitive(false);
            }
        });
    }

    // Open button handler
    {
        let selected = selected_meta.clone();
        let on_action = on_action.clone();
        open_btn.connect_clicked(move |_| {
            if let Some(meta) = selected.borrow().clone() {
                on_action(ProjectAction::Open(meta));
            }
        });
    }

    // ── Refresh callback: rebuild cards ───────────────────────────────────
    let flow_ref = flow.clone();
    let stack_ref = stack.clone();
    let project_list_ref = project_list.clone();
    let action_bar_ref = action_bar.clone();
    let open_btn_ref = open_btn.clone();
    let selected_ref = selected_meta.clone();

    let refresh: Rc<dyn Fn(Vec<ProjectMeta>)> = Rc::new(move |projects: Vec<ProjectMeta>| {
        while let Some(child) = flow_ref.first_child() {
            flow_ref.remove(&child);
        }

        *selected_ref.borrow_mut() = None;
        open_btn_ref.set_sensitive(false);

        if projects.is_empty() {
            stack_ref.set_visible_child_name("empty");
            action_bar_ref.set_visible(false);
            return;
        }

        stack_ref.set_visible_child_name("grid");
        action_bar_ref.set_visible(true);

        *project_list_ref.borrow_mut() = projects.clone();

        for meta in &projects {
            let card = build_project_card(meta);
            flow_ref.insert(&card, -1);
        }
    });

    (outer.upcast(), refresh)
}

// ─── Project card ─────────────────────────────────────────────────────────────

fn build_project_card(
    meta: &ProjectMeta,
) -> gtk4::Widget {
    let card = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(0)
        .css_classes(["card"])
        .width_request(180)
        .valign(gtk4::Align::Start)
        .build();
    card.set_overflow(gtk4::Overflow::Hidden);

    // ── Flight path preview (drawn with Cairo) ────────────────────────────
    let preview = build_flight_preview(&meta.path);
    card.append(&preview);

    // ── Info section ──────────────────────────────────────────────────────
    let info = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();

    let name_label = gtk4::Label::builder()
        .label(&meta.name)
        .css_classes(["title-4"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    info.append(&name_label);

    let location_label = gtk4::Label::builder()
        .label(&meta.location_name)
        .css_classes(["dim-label", "caption"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    info.append(&location_label);

    // Stats row
    let stats = load_project_stats(&meta.path);
    let stats_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_top(2)
        .build();

    if stats.waypoint_count > 0 {
        stats_box.append(&make_stat_chip(
            "camera-photo-symbolic",
            &format!("{}", stats.waypoint_count),
        ));
    }
    if stats.flight_distance_m > 0.0 {
        let dist_str = if stats.flight_distance_m > 1000.0 {
            format!("{:.1} km", stats.flight_distance_m / 1000.0)
        } else {
            format!("{:.0} m", stats.flight_distance_m)
        };
        stats_box.append(&make_stat_chip(
            "emblem-system-symbolic",
            &dist_str,
        ));
    }
    info.append(&stats_box);

    card.append(&info);

    card.upcast()
}

fn make_stat_chip(icon: &str, text: &str) -> gtk4::Box {
    let b = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(4)
        .build();
    b.append(
        &gtk4::Image::builder()
            .icon_name(icon)
            .pixel_size(14)
            .css_classes(["dim-label"])
            .build(),
    );
    b.append(
        &gtk4::Label::builder()
            .label(text)
            .css_classes(["dim-label", "caption"])
            .build(),
    );
    b
}

// ─── Flight path preview drawing ──────────────────────────────────────────────

struct QuickStats {
    waypoint_count: usize,
    flight_distance_m: f64,
    estimated_time_min: f64,
}

impl Default for QuickStats {
    fn default() -> Self {
        Self {
            waypoint_count: 0,
            flight_distance_m: 0.0,
            estimated_time_min: 0.0,
        }
    }
}

fn load_project_stats(project_dir: &PathBuf) -> QuickStats {
    let settings_path = project_dir.join("settings.json");
    let json = match std::fs::read_to_string(&settings_path) {
        Ok(j) => j,
        Err(_) => return QuickStats::default(),
    };
    // Parse just what we need
    let v: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return QuickStats::default(),
    };

    let waypoints: Vec<LatLng> = v.get("waypoints")
        .and_then(|w| serde_json::from_value(w.clone()).ok())
        .unwrap_or_default();

    let speed = v.get("speed").and_then(|s| s.as_f64()).unwrap_or(4.0);
    let delay = v.get("delay_at_waypoint").and_then(|d| d.as_i64()).unwrap_or(2) as f64;

    let n = waypoints.len();
    let dist = crate::engine::mapping::MappingEngine::total_distance(&waypoints);
    let time = if n > 0 {
        (dist / speed.max(0.01) + n as f64 * delay) / 60.0
    } else {
        0.0
    };

    QuickStats {
        waypoint_count: n,
        flight_distance_m: dist,
        estimated_time_min: time,
    }
}

fn build_flight_preview(project_dir: &PathBuf) -> gtk4::DrawingArea {
    let settings_path = project_dir.join("settings.json");
    let polygon: Vec<LatLng>;
    let waypoints: Vec<LatLng>;

    if let Ok(json) = std::fs::read_to_string(&settings_path) {
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        polygon = v.get("polygon")
            .and_then(|p| serde_json::from_value(p.clone()).ok())
            .unwrap_or_default();
        waypoints = v.get("waypoints")
            .and_then(|w| serde_json::from_value(w.clone()).ok())
            .unwrap_or_default();
    } else {
        polygon = vec![];
        waypoints = vec![];
    }

    let area = gtk4::DrawingArea::builder()
        .content_height(80)
        .content_width(180)
        .build();
    area.add_css_class("card");

    area.set_draw_func(move |_area, cr, w, h| {
        let w = w as f64;
        let h = h as f64;

        // Background
        cr.set_source_rgba(0.15, 0.15, 0.15, 0.3);
        let _ = cr.rectangle(0.0, 0.0, w, h);
        let _ = cr.fill();

        if polygon.is_empty() && waypoints.is_empty() {
            // Empty state
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.3);
            cr.set_font_size(12.0);
            let extents = cr.text_extents("No flight path").unwrap();
            cr.move_to(w / 2.0 - extents.width() / 2.0, h / 2.0);
            let _ = cr.show_text("No flight path");
            return;
        }

        // Compute bounding box of all points
        let all_points: Vec<(f64, f64)> = polygon
            .iter()
            .chain(waypoints.iter())
            .map(|p| (p.lng, p.lat))
            .collect();

        let min_x = all_points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let max_x = all_points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = all_points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_y = all_points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);

        let range_x = (max_x - min_x).max(1e-8);
        let range_y = (max_y - min_y).max(1e-8);

        let padding = 16.0;
        let draw_w = w - padding * 2.0;
        let draw_h = h - padding * 2.0;
        let scale = (draw_w / range_x).min(draw_h / range_y);

        let to_screen = |lng: f64, lat: f64| -> (f64, f64) {
            let sx = padding + (lng - min_x) * scale + (draw_w - range_x * scale) / 2.0;
            let sy = padding + (max_y - lat) * scale + (draw_h - range_y * scale) / 2.0;
            (sx, sy)
        };

        // Draw polygon fill
        if polygon.len() >= 3 {
            let (sx, sy) = to_screen(polygon[0].lng, polygon[0].lat);
            cr.move_to(sx, sy);
            for p in &polygon[1..] {
                let (sx, sy) = to_screen(p.lng, p.lat);
                cr.line_to(sx, sy);
            }
            cr.close_path();
            cr.set_source_rgba(0.208, 0.518, 0.894, 0.15);
            let _ = cr.fill_preserve();
            cr.set_source_rgba(0.208, 0.518, 0.894, 0.6);
            cr.set_line_width(1.5);
            let _ = cr.stroke();
        }

        // Draw flight path
        if waypoints.len() >= 2 {
            let (sx, sy) = to_screen(waypoints[0].lng, waypoints[0].lat);
            cr.move_to(sx, sy);
            for wp in &waypoints[1..] {
                let (sx, sy) = to_screen(wp.lng, wp.lat);
                cr.line_to(sx, sy);
            }
            cr.set_source_rgba(0.898, 0.647, 0.039, 0.8);
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        }
    });

    area
}
