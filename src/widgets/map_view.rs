//! Native libshumate map view – replaces the old WebKit + Leaflet approach.

use std::cell::RefCell;
use std::rc::Rc;

use gdk4 as gdk;
use glib;
use gtk4::gio;
use gtk4::prelude::*;
use libshumate::prelude::*;

use crate::engine::mapping::LatLng;

// ─── GNOME Adwaita colours ─────────────────────────────────────────────────
const ADWAITA_BLUE:   gdk::RGBA = gdk::RGBA::new(0.208, 0.518, 0.894, 1.0);  // #3584e4
const ADWAITA_GREEN:  gdk::RGBA = gdk::RGBA::new(0.180, 0.761, 0.494, 1.0);  // #2ec27e
const ADWAITA_YELLOW: gdk::RGBA = gdk::RGBA::new(0.898, 0.647, 0.039, 1.0);  // #e5a50a

// ─── MapView ──────────────────────────────────────────────────────────────────

/// Wrapper around `shumate::SimpleMap` that manages polygon, flight-path and
/// home-point layers.  Exposes the same public API as the old WebKit version.
#[derive(Clone)]
pub struct MapView {
    pub widget: libshumate::SimpleMap,
    inner: Rc<MapViewInner>,
}

struct MapViewInner {
    simple_map: libshumate::SimpleMap,
    /// User-drawn polygon vertices (Coordinate objects kept alive for PathLayer).
    polygon_coords: RefCell<Vec<libshumate::Coordinate>>,
    polygon_layer: libshumate::PathLayer,
    /// Vertex markers shown on the polygon corners.
    vertex_layer: libshumate::MarkerLayer,
    /// Flight-path polyline.
    flight_layer: libshumate::PathLayer,
    /// Waypoint dot markers.
    waypoint_layer: libshumate::MarkerLayer,
    /// Single home-point marker.
    home_layer: libshumate::MarkerLayer,
    /// Callback into window.rs state machine.
    on_message: RefCell<Option<Rc<dyn Fn(MapMessage)>>>,
}

impl MapView {
    /// Create the map widget, layers, and gesture handlers.
    pub fn new(on_message: Rc<dyn Fn(MapMessage)>) -> Self {
        let simple_map = libshumate::SimpleMap::new();
        simple_map.set_show_zoom_buttons(true);

        // Use OSM Mapnik as default tile source
        let registry = libshumate::MapSourceRegistry::with_defaults();
        if let Some(source) = registry.by_id(&libshumate::MAP_SOURCE_OSM_MAPNIK) {
            simple_map.set_map_source(Some(&source));
        }

        let viewport = simple_map.viewport().expect("SimpleMap must have a viewport");
        viewport.set_zoom_level(3.0);

        // ── Layers ────────────────────────────────────────────────────────
        let polygon_layer = libshumate::PathLayer::new(&viewport);
        polygon_layer.set_closed(true);
        polygon_layer.set_fill(true);
        polygon_layer.set_fill_color(Some(&gdk::RGBA::new(ADWAITA_BLUE.red(), ADWAITA_BLUE.green(), ADWAITA_BLUE.blue(), 0.15)));
        polygon_layer.set_stroke(true);
        polygon_layer.set_stroke_color(Some(&ADWAITA_BLUE));
        polygon_layer.set_stroke_width(2.5);

        let vertex_layer = libshumate::MarkerLayer::new(&viewport);

        let flight_layer = libshumate::PathLayer::new(&viewport);
        flight_layer.set_closed(false);
        flight_layer.set_fill(false);
        flight_layer.set_stroke(true);
        flight_layer.set_stroke_color(Some(&ADWAITA_YELLOW));
        flight_layer.set_stroke_width(2.5);

        let waypoint_layer = libshumate::MarkerLayer::new(&viewport);
        let home_layer = libshumate::MarkerLayer::new(&viewport);

        // Add layers to the map
        let map_widget = simple_map.map().expect("SimpleMap must have an inner Map");
        map_widget.add_layer(&polygon_layer);
        map_widget.add_layer(&vertex_layer);
        map_widget.add_layer(&flight_layer);
        map_widget.add_layer(&waypoint_layer);
        map_widget.add_layer(&home_layer);

        let inner = Rc::new(MapViewInner {
            simple_map: simple_map.clone(),
            polygon_coords: RefCell::new(Vec::new()),
            polygon_layer,
            vertex_layer,
            flight_layer,
            waypoint_layer,
            home_layer,
            on_message: RefCell::new(Some(on_message)),
        });

        // ── Click gesture: primary = add vertex, secondary = set home ─────
        // Track press position so we can ignore map-drag releases.
        let press_pos: Rc<RefCell<Option<(f64, f64)>>> = Rc::new(RefCell::new(None));

        let click = gtk4::GestureClick::new();
        click.set_button(0); // listen to all buttons
        {
            let pp = press_pos.clone();
            click.connect_pressed(move |_gesture, _n_press, x, y| {
                *pp.borrow_mut() = Some((x, y));
            });
        }
        {
            let inner = inner.clone();
            let pp = press_pos.clone();
            click.connect_released(move |gesture, _n_press, x, y| {
                // Ignore if the pointer moved more than 5px (i.e. it was a drag)
                if let Some((px, py)) = pp.borrow().as_ref() {
                    let dx = x - px;
                    let dy = y - py;
                    if (dx * dx + dy * dy).sqrt() > 5.0 {
                        return;
                    }
                }

                let button = gesture.current_button();
                let vp = inner.simple_map.viewport().unwrap();

                // Convert widget coords → lat/lng
                let map_w = inner.simple_map.map().unwrap();
                let (lat, lng) = vp.widget_coords_to_location(&map_w, x, y);

                if button == 1 {
                    // Primary click → add polygon vertex
                    let coord = libshumate::Coordinate::new_full(lat, lng);
                    inner.polygon_layer.add_node(&coord);

                    // Vertex marker
                    let marker = make_dot_marker(8.0, &ADWAITA_BLUE);
                    marker.set_location(lat, lng);
                    inner.vertex_layer.add_marker(&marker);

                    inner.polygon_coords.borrow_mut().push(coord);

                    // Notify
                    let pts = read_polygon(&inner.polygon_coords.borrow());
                    inner.emit(MapMessage::PolygonChanged(pts));
                } else if button == 3 {
                    // Secondary click → set home
                    inner.home_layer.remove_all();
                    let marker = make_home_marker();
                    marker.set_location(lat, lng);
                    inner.home_layer.add_marker(&marker);
                    inner.emit(MapMessage::HomeChanged(LatLng { lat, lng }));
                }
            });
        }
        simple_map.add_controller(click);

        MapView {
            widget: simple_map,
            inner,
        }
    }

    // ─── Public helpers matching the old API ──────────────────────────────

    pub fn set_center(&self, lat: f64, lng: f64, zoom: Option<u8>) {
        if let Some(vp) = self.widget.viewport() {
            vp.set_location(lat, lng);
            if let Some(z) = zoom {
                vp.set_zoom_level(z as f64);
            }
        }
    }

    pub fn update_flight_path(&self, waypoints: &[LatLng], home: Option<LatLng>) {
        let inner = &self.inner;
        // Clear previous flight path
        inner.flight_layer.remove_all();
        inner.waypoint_layer.remove_all();
        inner.home_layer.remove_all();

        for (i, wp) in waypoints.iter().enumerate() {
            let coord = libshumate::Coordinate::new_full(wp.lat, wp.lng);
            inner.flight_layer.add_node(&coord);

            // Compute flight direction (bearing) at this waypoint
            let bearing = if waypoints.len() < 2 {
                0.0
            } else if i == 0 {
                bearing_deg(wp, &waypoints[1])
            } else {
                bearing_deg(&waypoints[i - 1], wp)
            };

            let marker = make_arrow_marker(bearing, &ADWAITA_YELLOW);
            marker.set_location(wp.lat, wp.lng);
            inner.waypoint_layer.add_marker(&marker);
        }

        if let Some(h) = home {
            let marker = make_home_marker();
            marker.set_location(h.lat, h.lng);
            inner.home_layer.add_marker(&marker);
        }
    }

    pub fn clear_flight_path(&self) {
        self.inner.flight_layer.remove_all();
        self.inner.waypoint_layer.remove_all();
        self.inner.home_layer.remove_all();
    }

    pub fn import_polygon(&self, points: &[LatLng]) {
        self.clear_polygon();
        let inner = &self.inner;
        let mut coords = inner.polygon_coords.borrow_mut();
        for p in points {
            let coord = libshumate::Coordinate::new_full(p.lat, p.lng);
            inner.polygon_layer.add_node(&coord);

            let marker = make_dot_marker(8.0, &ADWAITA_BLUE);
            marker.set_location(p.lat, p.lng);
            inner.vertex_layer.add_marker(&marker);

            coords.push(coord);
        }
        let pts = read_polygon(&coords);
        inner.emit(MapMessage::PolygonChanged(pts));
    }

    pub fn clear_all(&self) {
        self.clear_polygon();
        self.clear_flight_path();
        self.inner.emit(MapMessage::Cleared);
    }

    pub fn set_show_waypoints(&self, show: bool) {
        self.inner.waypoint_layer.set_visible(show);
    }

    /// Switch the map to one of the built-in sources by ID (e.g. MAP_SOURCE_OSM_MAPNIK).
    pub fn set_map_source_by_id(&self, id: &str) {
        let registry = libshumate::MapSourceRegistry::with_defaults();
        if let Some(source) = registry.by_id(id) {
            self.widget.set_map_source(Some(&source));
        }
    }

    /// Switch the map to a custom tile URL (e.g. Google Maps satellite tiles).
    pub fn set_map_source_url(&self, id: &str, name: &str, url_template: &str) {
        self.set_map_source_url_with_tile_size(id, name, url_template, 256);
    }

    /// Switch the map to a custom tile URL with a specific tile size.
    pub fn set_map_source_url_with_tile_size(&self, id: &str, name: &str, url_template: &str, tile_size: u32) {
        let source = libshumate::RasterRenderer::new_full_from_url(
            id,
            name,
            "",   // license
            "",   // license_uri
            0,    // min_zoom
            21,   // max_zoom
            tile_size, // tile_size
            libshumate::MapProjection::Mercator,
            url_template,
        );
        self.widget.set_map_source(Some(&source));
    }

    pub fn set_tile_layer(&self, _satellite: bool) {
        // Kept for API compat – use set_map_source_by_id / set_map_source_url instead.
    }

    pub fn set_dark_mode(&self, _dark: bool) {
        // No built-in dark tile variant; could swap map source later.
    }

    /// Try to get the user's location via GeoClue (freedesktop portal) and
    /// centre the map on it.  Runs asynchronously – the map jumps once the
    /// location is available.
    pub fn request_user_location(&self) {
        let widget = self.widget.clone();
        glib::spawn_future_local(async move {
            match gio::Cancellable::NONE {
                _ => {
                    // Use GeoClue Simple via gio's D-Bus proxy.
                    // Fall back to a sensible default if unavailable.
                    let connection = match gio::bus_get_future(gio::BusType::System).await {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    let reply = connection
                        .call_future(
                            Some("org.freedesktop.GeoClue2"),
                            "/org/freedesktop/GeoClue2/Manager",
                            "org.freedesktop.GeoClue2.Manager",
                            "GetClient",
                            None,
                            Some(&glib::VariantType::new("(o)").unwrap()),
                            gio::DBusCallFlags::NONE,
                            5000,
                        )
                        .await;
                    let client_path = match reply {
                        Ok(v) => {
                            let child = v.child_value(0);
                            child.get::<String>().unwrap_or_default()
                        }
                        Err(_) => return,
                    };
                    if client_path.is_empty() {
                        return;
                    }

                    // Set DesktopId so GeoClue allows our request
                    let _ = connection
                        .call_future(
                            Some("org.freedesktop.GeoClue2"),
                            &client_path,
                            "org.freedesktop.DBus.Properties",
                            "Set",
                            Some(&(
                                "org.freedesktop.GeoClue2.Client",
                                "DesktopId",
                                glib::Variant::from("io.github.silkepilon.Wayfarer").to_variant(),
                            ).to_variant()),
                            None,
                            gio::DBusCallFlags::NONE,
                            5000,
                        )
                        .await;

                    // Set requested accuracy level (Exact = 8)
                    let _ = connection
                        .call_future(
                            Some("org.freedesktop.GeoClue2"),
                            &client_path,
                            "org.freedesktop.DBus.Properties",
                            "Set",
                            Some(&(
                                "org.freedesktop.GeoClue2.Client",
                                "RequestedAccuracyLevel",
                                glib::Variant::from(8u32).to_variant(),
                            ).to_variant()),
                            None,
                            gio::DBusCallFlags::NONE,
                            5000,
                        )
                        .await;

                    // Start the client
                    let _ = connection
                        .call_future(
                            Some("org.freedesktop.GeoClue2"),
                            &client_path,
                            "org.freedesktop.GeoClue2.Client",
                            "Start",
                            None,
                            None,
                            gio::DBusCallFlags::NONE,
                            5000,
                        )
                        .await;

                    // Read the Location property
                    let loc_reply = connection
                        .call_future(
                            Some("org.freedesktop.GeoClue2"),
                            &client_path,
                            "org.freedesktop.DBus.Properties",
                            "Get",
                            Some(&(
                                "org.freedesktop.GeoClue2.Client",
                                "Location",
                            ).to_variant()),
                            Some(&glib::VariantType::new("(v)").unwrap()),
                            gio::DBusCallFlags::NONE,
                            10000,
                        )
                        .await;
                    let loc_path = match loc_reply {
                        Ok(v) => {
                            let inner_v = v.child_value(0).child_value(0);
                            inner_v.get::<String>().unwrap_or_default()
                        }
                        Err(_) => return,
                    };
                    if loc_path.is_empty() || loc_path == "/" {
                        return;
                    }

                    // Read Latitude and Longitude from the Location object
                    let get_prop = |prop: &'static str| {
                        let connection = connection.clone();
                        let loc_path = loc_path.clone();
                        async move {
                            let r = connection
                                .call_future(
                                    Some("org.freedesktop.GeoClue2"),
                                    &loc_path,
                                    "org.freedesktop.DBus.Properties",
                                    "Get",
                                    Some(&(
                                        "org.freedesktop.GeoClue2.Location",
                                        prop,
                                    ).to_variant()),
                                    Some(&glib::VariantType::new("(v)").unwrap()),
                                    gio::DBusCallFlags::NONE,
                                    5000,
                                )
                                .await;
                            r.ok()
                                .and_then(|v| v.child_value(0).child_value(0).get::<f64>())
                        }
                    };

                    let lat = get_prop("Latitude").await;
                    let lng = get_prop("Longitude").await;

                    // Stop the client (best effort)
                    let _ = connection
                        .call_future(
                            Some("org.freedesktop.GeoClue2"),
                            &client_path,
                            "org.freedesktop.GeoClue2.Client",
                            "Stop",
                            None,
                            None,
                            gio::DBusCallFlags::NONE,
                            5000,
                        )
                        .await;

                    if let (Some(lat), Some(lng)) = (lat, lng) {
                        if let Some(vp) = widget.viewport() {
                            vp.set_location(lat, lng);
                            vp.set_zoom_level(14.0);
                        }
                    }
                }
            }
        });
    }

    fn clear_polygon(&self) {
        let inner = &self.inner;
        inner.polygon_layer.remove_all();
        inner.vertex_layer.remove_all();
        inner.polygon_coords.borrow_mut().clear();
    }
}

// ─── Message types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum MapMessage {
    Ready,
    PolygonChanged(Vec<LatLng>),
    HomeChanged(LatLng),
    HomeRemoved,
    Cleared,
}

impl MapViewInner {
    fn emit(&self, msg: MapMessage) {
        if let Some(cb) = self.on_message.borrow().as_ref() {
            cb(msg);
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn read_polygon(coords: &[libshumate::Coordinate]) -> Vec<LatLng> {
    coords
        .iter()
        .map(|c| {
            use libshumate::prelude::LocationExt;
            LatLng {
                lat: c.latitude(),
                lng: c.longitude(),
            }
        })
        .collect()
}

/// Create a small coloured circle marker.
fn make_dot_marker(size: f64, colour: &gdk::RGBA) -> libshumate::Marker {
    let dia = (size * 2.0).ceil() as i32;
    let area = gtk4::DrawingArea::new();
    area.set_content_width(dia);
    area.set_content_height(dia);
    let c = *colour;
    let r = size / 2.0;
    area.set_draw_func(move |_area, cr, w, h| {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        // White outline
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
        let _ = cr.arc(cx, cy, r + 1.5, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();
        // Coloured fill
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        let _ = cr.arc(cx, cy, r, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();
    });
    let marker = libshumate::Marker::new();
    marker.set_child(Some(&area));
    marker
}

/// Compute bearing in degrees (0=N, 90=E) from `a` to `b`.
fn bearing_deg(a: &LatLng, b: &LatLng) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let d_lng = (b.lng - a.lng).to_radians();
    let x = d_lng.cos() * lat2.cos();
    let y = lat1.cos() * lat2.sin() - lat1.sin() * x;
    let x2 = d_lng.sin() * lat2.cos();
    x2.atan2(y).to_degrees().rem_euclid(360.0)
}

/// Create an arrow marker that points in the given compass bearing.
/// The arrow shows the flight direction at each waypoint.
fn make_arrow_marker(bearing_degrees: f64, colour: &gdk::RGBA) -> libshumate::Marker {
    let size = 14;
    let area = gtk4::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    let c = *colour;
    let angle = bearing_degrees;
    area.set_draw_func(move |_area, cr, w, h| {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let s = w.min(h) as f64;
        let rad = angle.to_radians();

        cr.save().ok();
        cr.translate(cx, cy);
        cr.rotate(rad); // rotate so 0° = up (north)

        // Arrow shape: triangle pointing up
        let half = s / 2.0 - 1.0;
        cr.move_to(0.0, -half);          // tip (forward)
        cr.line_to(-half * 0.55, half * 0.5);  // bottom-left
        cr.line_to(0.0, half * 0.2);     // notch
        cr.line_to(half * 0.55, half * 0.5);   // bottom-right
        cr.close_path();

        // White outline
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
        cr.set_line_width(2.0);
        let _ = cr.stroke_preserve();
        // Fill with colour
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        let _ = cr.fill();

        cr.restore().ok();
    });
    let marker = libshumate::Marker::new();
    marker.set_child(Some(&area));
    marker
}

/// Create a home point marker (green circle with white H).
fn make_home_marker() -> libshumate::Marker {
    let size = 22;
    let area = gtk4::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_draw_func(move |_area, cr, w, h| {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let r = (w.min(h) as f64 / 2.0) - 1.0;

        // White ring
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        let _ = cr.arc(cx, cy, r + 2.0, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();

        // Green fill
        let g = ADWAITA_GREEN;
        cr.set_source_rgba(g.red() as f64, g.green() as f64, g.blue() as f64, 1.0);
        let _ = cr.arc(cx, cy, r, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();

        // White "H" letter
        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        cr.set_font_size(r * 1.2);
        let extents = cr.text_extents("H").unwrap();
        cr.move_to(cx - extents.width() / 2.0 - extents.x_bearing(), cy - extents.height() / 2.0 - extents.y_bearing());
        let _ = cr.show_text("H");
    });
    let marker = libshumate::Marker::new();
    marker.set_child(Some(&area));
    marker
}
