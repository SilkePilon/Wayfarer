//! Core waypoint generation engine — boustrophedon (lawnmower) pattern.
//!
//! This is a 1:1 port of the Dart `DroneMappingEngine` found in the Flutter
//! Wayfarer project, translated to idiomatic Rust.

use std::f64::consts::PI;

// ─── Public types ────────────────────────────────────────────────────────────

/// A geographic coordinate pair.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

// ─── Internal 2-D point (local Cartesian metres) ─────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Pt {
    x: f64,
    y: f64,
}

// ─── Engine ───────────────────────────────────────────────────────────────────

/// All parameters needed to plan a survey mission.
#[derive(Debug, Clone)]
pub struct MappingEngine {
    pub altitude: f64,        // m above take-off point
    pub ground_offset: f64,   // m – height of target surface above ground
    pub forward_overlap: f64, // 0.0–1.0
    pub side_overlap: f64,    // 0.0–1.0
    pub sensor_width: f64,    // mm
    pub sensor_height: f64,   // mm
    pub focal_length: f64,    // mm
    pub image_width: i32,     // px
    pub image_height: i32,    // px
    pub angle: f64,           // rotation of grid in degrees
}

impl MappingEngine {
    // ─── derived geometry ────────────────────────────────────────────────

    pub fn effective_altitude(&self) -> f64 {
        self.altitude - self.ground_offset
    }

    pub fn gsd_x(&self) -> f64 {
        (self.effective_altitude() * self.sensor_width)
            / (self.image_width as f64 * self.focal_length)
    }

    pub fn gsd_y(&self) -> f64 {
        (self.effective_altitude() * self.sensor_height)
            / (self.image_height as f64 * self.focal_length)
    }

    pub fn footprint_width(&self) -> f64 {
        self.gsd_x() * self.image_width as f64
    }

    pub fn footprint_height(&self) -> f64 {
        self.gsd_y() * self.image_height as f64
    }

    /// Spacing between parallel flight lines (cross-track).
    pub fn horizontal_line_spacing(&self) -> f64 {
        self.footprint_height() * (1.0 - self.side_overlap)
    }

    /// Spacing between waypoints along a flight line.
    pub fn horizontal_waypoint_spacing(&self) -> f64 {
        self.footprint_width() * (1.0 - self.forward_overlap)
    }

    // ─── coordinate helpers ──────────────────────────────────────────────

    /// Project a LatLng slice to a local metric Cartesian plane whose origin
    /// is `polygon[0]`.  Uses equirectangular projection.
    fn to_metres(polygon: &[LatLng]) -> Vec<Pt> {
        let o = polygon[0];
        let metres_per_deg_lat = 40_075_000.0 / 360.0;
        let metres_per_deg_lng = 40_075_000.0 * (o.lat * PI / 180.0).cos() / 360.0;
        polygon
            .iter()
            .map(|ll| Pt {
                x: (ll.lng - o.lng) * metres_per_deg_lng,
                y: (ll.lat - o.lat) * metres_per_deg_lat,
            })
            .collect()
    }

    fn to_metres_one(ll: LatLng, origin: LatLng) -> Pt {
        let metres_per_deg_lat = 40_075_000.0 / 360.0;
        let metres_per_deg_lng = 40_075_000.0 * (origin.lat * PI / 180.0).cos() / 360.0;
        Pt {
            x: (ll.lng - origin.lng) * metres_per_deg_lng,
            y: (ll.lat - origin.lat) * metres_per_deg_lat,
        }
    }

    fn from_metres(pts: &[Pt], origin: LatLng) -> Vec<LatLng> {
        let metres_per_deg_lat = 40_075_000.0 / 360.0;
        let metres_per_deg_lng = 40_075_000.0 * (origin.lat * PI / 180.0).cos() / 360.0;
        pts.iter()
            .map(|p| LatLng {
                lat: origin.lat + p.y / metres_per_deg_lat,
                lng: origin.lng + p.x / metres_per_deg_lng,
            })
            .collect()
    }

    // ─── geometry helpers ────────────────────────────────────────────────

    fn rotate(p: Pt, deg: f64) -> Pt {
        let r = deg * PI / 180.0;
        Pt {
            x: p.x * r.cos() - p.y * r.sin(),
            y: p.x * r.sin() + p.y * r.cos(),
        }
    }

    fn rotate_all(pts: &[Pt], deg: f64) -> Vec<Pt> {
        pts.iter().map(|&p| Self::rotate(p, deg)).collect()
    }

    /// Ray-casting point-in-polygon test.
    #[allow(dead_code)]
    fn pip(p: Pt, poly: &[Pt]) -> bool {
        let n = poly.len();
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let pi = poly[i];
            let pj = poly[j];
            if (pi.y > p.y) != (pj.y > p.y)
                && p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x
            {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    fn dist(a: Pt, b: Pt) -> f64 {
        ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
    }

    /// Find all x-coordinates where the horizontal line `y` intersects the
    /// polygon edges. Returns sorted values; consecutive pairs form the
    /// inside intervals: [x0..x1], [x2..x3], etc.
    fn scanline_x_hits(poly: &[Pt], y: f64) -> Vec<f64> {
        let n = poly.len();
        let mut hits = Vec::new();
        let mut j = n - 1;
        for i in 0..n {
            let pi = poly[i];
            let pj = poly[j];
            if (pi.y <= y && pj.y > y) || (pj.y <= y && pi.y > y) {
                let x = pi.x + (y - pi.y) / (pj.y - pi.y) * (pj.x - pi.x);
                hits.push(x);
            }
            j = i;
        }
        hits.sort_by(|a, b| a.partial_cmp(b).unwrap());
        hits
    }

    /// Find all y-coordinates where the vertical line `x` intersects the
    /// polygon edges. Returns sorted values; consecutive pairs form the
    /// inside intervals.
    fn scanline_y_hits(poly: &[Pt], x: f64) -> Vec<f64> {
        let n = poly.len();
        let mut hits = Vec::new();
        let mut j = n - 1;
        for i in 0..n {
            let pi = poly[i];
            let pj = poly[j];
            if (pi.x <= x && pj.x > x) || (pj.x <= x && pi.x > x) {
                let y = pi.y + (x - pi.x) / (pj.x - pi.x) * (pj.y - pi.y);
                hits.push(y);
            }
            j = i;
        }
        hits.sort_by(|a, b| a.partial_cmp(b).unwrap());
        hits
    }

    // ─── public static utilities ─────────────────────────────────────────

    /// Shoelace area formula (returns m²).
    pub fn calculate_area(polygon: &[LatLng]) -> f64 {
        let local = Self::to_metres(polygon);
        let n = local.len();
        let mut area = 0.0_f64;
        for i in 0..n - 1 {
            area += local[i].x * local[i + 1].y - local[i + 1].x * local[i].y;
        }
        area += local[n - 1].x * local[0].y - local[0].x * local[n - 1].y;
        area.abs() / 2.0
    }

    /// Haversine total distance along a waypoint list (metres).
    pub fn total_distance(wps: &[LatLng]) -> f64 {
        wps.windows(2).map(|w| haversine(w[0], w[1])).sum()
    }

    /// Closest standard shutter speed that avoids motion blur.
    pub fn recommended_shutter(
        altitude: f64,
        sensor_width_mm: f64,
        focal_length_mm: f64,
        image_width_px: i32,
        speed_ms: f64,
    ) -> String {
        let gsd = (altitude * sensor_width_mm) / (image_width_px as f64 * focal_length_mm);
        let ideal = gsd / speed_ms.max(0.001);
        let speeds: &[f64] = &[
            1.0 / 16000.0,
            1.0 / 8000.0,
            1.0 / 4000.0,
            1.0 / 2000.0,
            1.0 / 1600.0,
            1.0 / 1250.0,
            1.0 / 1000.0,
            1.0 / 800.0,
            1.0 / 640.0,
            1.0 / 500.0,
            1.0 / 400.0,
            1.0 / 320.0,
            1.0 / 250.0,
            1.0 / 200.0,
            1.0 / 160.0,
            1.0 / 125.0,
            1.0 / 100.0,
            1.0 / 80.0,
            1.0 / 60.0,
            1.0 / 50.0,
            1.0 / 40.0,
            1.0 / 30.0,
            1.0 / 25.0,
            1.0 / 20.0,
            1.0 / 15.0,
            1.0 / 13.0,
            1.0 / 10.0,
            1.0 / 8.0,
            1.0 / 6.0,
            1.0 / 5.0,
            1.0 / 4.0,
            1.0 / 3.0,
            1.0 / 2.5,
            1.0 / 2.0,
        ];
        let &best = speeds
            .iter()
            .min_by(|&&a, &&b| (ideal - a).abs().partial_cmp(&(ideal - b).abs()).unwrap())
            .unwrap_or(&(1.0 / 500.0));
        format!("1/{}", (1.0 / best).round() as u32)
    }

    // ─── waypoint generation ─────────────────────────────────────────────

    /// Generate boustrophedon waypoints for the given polygon.
    ///
    /// * `create_camera_points` – dense photo-trigger points vs sparse WPs
    /// * `fill_grid`            – add a perpendicular cross-hatch pass
    /// * `home_point`           – if given, optimise path to minimise return
    pub fn generate_waypoints(
        &self,
        polygon: &[LatLng],
        create_camera_points: bool,
        fill_grid: bool,
        home_point: Option<LatLng>,
    ) -> Vec<LatLng> {
        if polygon.len() < 3 {
            return vec![];
        }

        let origin = polygon[0];
        let local = Self::to_metres(polygon);
        let rot = Self::rotate_all(&local, self.angle);

        let min_x = rot.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = rot.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = rot.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_y = rot.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);

        let selected: Vec<Pt> = if let Some(home) = home_point {
            // Rotate home point into the same rotated frame
            let home_local = Self::to_metres_one(home, origin);
            let home_rot = Self::rotate(home_local, self.angle);

            if !fill_grid {
                let horiz = self.gen_horizontal(
                    &rot,
                    create_camera_points,
                    home_rot,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                );
                if horiz.is_empty() {
                    return vec![];
                }
                let horiz_rev: Vec<Pt> = horiz.iter().rev().copied().collect();
                let d_fwd = Self::dist(*horiz.last().unwrap(), home_rot);
                let d_rev = Self::dist(*horiz_rev.last().unwrap(), home_rot);
                if d_fwd <= d_rev {
                    horiz
                } else {
                    horiz_rev
                }
            } else {
                // Evaluate 4 path permutations; pick the one whose last WP
                // is closest to home.
                let horiz = self.gen_horizontal(
                    &rot,
                    create_camera_points,
                    home_rot,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                );
                let vert_after_horiz = self.gen_vertical(
                    &rot,
                    create_camera_points,
                    horiz.last().copied().unwrap_or(home_rot),
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                );
                let path_hf: Vec<Pt> = horiz
                    .iter()
                    .chain(vert_after_horiz.iter())
                    .copied()
                    .collect();
                let path_hf_rev: Vec<Pt> = path_hf.iter().rev().copied().collect();

                let vert = self.gen_vertical(
                    &rot,
                    create_camera_points,
                    home_rot,
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                );
                let horiz_after_vert = self.gen_horizontal(
                    &rot,
                    create_camera_points,
                    vert.last().copied().unwrap_or(home_rot),
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                );
                let path_vf: Vec<Pt> = vert
                    .iter()
                    .chain(horiz_after_vert.iter())
                    .copied()
                    .collect();
                let path_vf_rev: Vec<Pt> = path_vf.iter().rev().copied().collect();

                let candidates = [path_hf, path_hf_rev, path_vf, path_vf_rev];
                candidates
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .min_by(|a, b| {
                        let da = Self::dist(*a.last().unwrap(), home_rot);
                        let db = Self::dist(*b.last().unwrap(), home_rot);
                        da.partial_cmp(&db).unwrap()
                    })
                    .unwrap_or_default()
            }
        } else {
            // No home point: simple boustrophedon sweep
            let mut pts: Vec<Pt> = Vec::new();
            let mut reverse = false;
            let line_sp = self.horizontal_line_spacing();
            // Start half a line-spacing inside the bounding box so the first
            // line is inset from the polygon edge (matching gen_horizontal).
            let mut y = min_y + line_sp / 2.0;
            while y <= max_y - line_sp / 2.0 + f64::EPSILON {
                let mut line = self.scan_row(&rot, y, min_x, max_x, create_camera_points);
                if !line.is_empty() {
                    if reverse {
                        line.reverse();
                    }
                    pts.extend_from_slice(&line);
                    reverse = !reverse;
                }
                y += line_sp;
            }
            if fill_grid {
                if let Some(&last) = pts.last() {
                    let vert = self.gen_vertical(
                        &rot,
                        create_camera_points,
                        last,
                        min_x,
                        max_x,
                        min_y,
                        max_y,
                    );
                    pts.extend_from_slice(&vert);
                }
            }
            pts
        };

        let unrot = Self::rotate_all(&selected, -self.angle);
        Self::from_metres(&unrot, origin)
    }

    // ─── sweep helpers ───────────────────────────────────────────────────

    /// All points along a single horizontal scanline `y`.
    /// Uses exact polygon-edge intersections for boundary points so waypoints
    /// extend to the polygon edges, with interior points on a global grid
    /// (aligned across all scanlines) for proper photogrammetry coverage.
    fn scan_row(&self, poly: &[Pt], y: f64, min_x: f64, _max_x: f64, dense: bool) -> Vec<Pt> {
        let hits = Self::scanline_x_hits(poly, y);
        if hits.len() < 2 {
            return vec![];
        }
        let spacing = self.horizontal_waypoint_spacing();

        let mut line: Vec<Pt> = Vec::new();
        // Process pairs of intersections (entry, exit)
        let mut i = 0;
        while i + 1 < hits.len() {
            let x_enter = hits[i];
            let x_exit = hits[i + 1];
            if dense {
                // Start with the exact polygon-edge entry point
                line.push(Pt { x: x_enter, y });
                // Interior points on a global grid aligned to min_x so all
                // scanlines share the same x-positions (important for
                // photogrammetry).
                let first_grid = min_x + ((x_enter - min_x) / spacing).ceil() * spacing;
                let mut x = first_grid;
                while x < x_exit - 1e-6 {
                    if x > x_enter + 1e-6 {
                        line.push(Pt { x, y });
                    }
                    x += spacing;
                }
                // End with the exact polygon-edge exit point
                if (x_exit - x_enter).abs() > 1e-6 {
                    line.push(Pt { x: x_exit, y });
                }
            } else {
                line.push(Pt { x: x_enter, y });
                if (x_exit - x_enter).abs() > 1e-6 {
                    line.push(Pt { x: x_exit, y });
                }
            }
            i += 2;
        }
        line
    }

    /// Generate horizontal waypoints starting from the end closest to `prev`.
    fn gen_horizontal(
        &self,
        poly: &[Pt],
        dense: bool,
        prev: Pt,
        min_x: f64,
        max_x: f64,
        min_y: f64,
        max_y: f64,
    ) -> Vec<Pt> {
        let line_sp = self.horizontal_line_spacing();
        let wp_sp = self.horizontal_waypoint_spacing();
        let offset = wp_sp * 0.1;

        // Pre-compute candidate x-coords for a scanline
        let mut xs: Vec<f64> = Vec::new();
        let mut x = min_x - wp_sp / 2.0 - offset;
        while x <= max_x + wp_sp / 2.0 {
            xs.push(x);
            x += wp_sp;
        }

        // Evaluate bottom and top first-row distances
        let y_bottom = min_y + line_sp / 2.0;
        let (d_bot_l, d_bot_r) = row_end_distances(&xs, y_bottom, poly, prev);
        let min_d_bot = d_bot_l.min(d_bot_r);

        let y_top = max_y - line_sp / 2.0;
        let (d_top_l, d_top_r) = row_end_distances(&xs, y_top, poly, prev);
        let min_d_top = d_top_l.min(d_top_r);

        // Choose start row and direction
        let (start_y, delta_y, mut reverse) = if min_d_bot <= min_d_top {
            (y_bottom, line_sp, d_bot_r < d_bot_l)
        } else {
            (y_top, -line_sp, d_top_r < d_top_l)
        };

        let mut out: Vec<Pt> = Vec::new();
        let mut y = start_y;
        loop {
            let ok = if delta_y > 0.0 {
                y <= max_y + line_sp / 2.0
            } else {
                y >= min_y - line_sp / 2.0
            };
            if !ok {
                break;
            }

            let mut line = self.scan_row_xs(poly, y, &xs, dense);
            if !line.is_empty() {
                if reverse {
                    line.reverse();
                }
                out.extend_from_slice(&line);
                reverse = !reverse;
            }
            y += delta_y;
        }
        out
    }

    /// Scan a row using exact polygon-edge intersections for boundary points
    /// and pre-computed x-coordinates for interior spacing.
    fn scan_row_xs(&self, poly: &[Pt], y: f64, xs: &[f64], dense: bool) -> Vec<Pt> {
        let hits = Self::scanline_x_hits(poly, y);
        if hits.len() < 2 {
            return vec![];
        }

        let mut line: Vec<Pt> = Vec::new();
        let mut i = 0;
        while i + 1 < hits.len() {
            let x_enter = hits[i];
            let x_exit = hits[i + 1];
            if dense {
                line.push(Pt { x: x_enter, y });
                for &x in xs {
                    if x > x_enter + 1e-6 && x < x_exit - 1e-6 {
                        line.push(Pt { x, y });
                    }
                }
                if (x_exit - x_enter).abs() > 1e-6 {
                    line.push(Pt { x: x_exit, y });
                }
            } else {
                line.push(Pt { x: x_enter, y });
                if (x_exit - x_enter).abs() > 1e-6 {
                    line.push(Pt { x: x_exit, y });
                }
            }
            i += 2;
        }
        line
    }

    /// Generate vertical waypoints starting from the column end closest to
    /// `last_h` (the last horizontal waypoint).
    fn gen_vertical(
        &self,
        poly: &[Pt],
        dense: bool,
        last_h: Pt,
        min_x: f64,
        max_x: f64,
        min_y: f64,
        max_y: f64,
    ) -> Vec<Pt> {
        let col_sp = self.footprint_width() * (1.0 - self.side_overlap);
        let row_sp = self.footprint_height() * (1.0 - self.forward_overlap);
        let offset = row_sp * 0.1;

        // Pre-compute y-coords
        let mut ys: Vec<f64> = Vec::new();
        let mut y = min_y - row_sp / 2.0 - offset;
        while y <= max_y + row_sp / 2.0 {
            ys.push(y);
            y += row_sp;
        }

        // Evaluate left and right first-column distances
        let x_left = min_x + col_sp / 2.0;
        let (d_l_bot, d_l_top) = col_end_distances(&ys, x_left, poly, last_h);
        let min_d_left = d_l_bot.min(d_l_top);

        let x_right = max_x - col_sp / 2.0;
        let (d_r_bot, d_r_top) = col_end_distances(&ys, x_right, poly, last_h);
        let min_d_right = d_r_bot.min(d_r_top);

        let (start_x, delta_x, mut reverse) = if min_d_left <= min_d_right {
            (x_left, col_sp, d_l_top < d_l_bot)
        } else {
            (x_right, -col_sp, d_r_top < d_r_bot)
        };

        let mut out: Vec<Pt> = Vec::new();
        let mut x = start_x;
        loop {
            let ok = if delta_x > 0.0 {
                x <= max_x + col_sp / 2.0
            } else {
                x >= min_x - col_sp / 2.0
            };
            if !ok {
                break;
            }

            let hits = Self::scanline_y_hits(poly, x);
            let mut col: Vec<Pt> = Vec::new();
            let mut hi = 0;
            while hi + 1 < hits.len() {
                let y_enter = hits[hi];
                let y_exit = hits[hi + 1];
                if dense {
                    col.push(Pt { x, y: y_enter });
                    for &yy in &ys {
                        if yy > y_enter + 1e-6 && yy < y_exit - 1e-6 {
                            col.push(Pt { x, y: yy });
                        }
                    }
                    if (y_exit - y_enter).abs() > 1e-6 {
                        col.push(Pt { x, y: y_exit });
                    }
                } else {
                    col.push(Pt { x, y: y_enter });
                    if (y_exit - y_enter).abs() > 1e-6 {
                        col.push(Pt { x, y: y_exit });
                    }
                }
                hi += 2;
            }

            if !col.is_empty() {
                if reverse {
                    col.reverse();
                }
                out.extend_from_slice(&col);
                reverse = !reverse;
            }
            x += delta_x;
        }
        out
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Returns (dist_to_left_end, dist_to_right_end) for the first/last inside
/// x-point on a horizontal row using exact edge intersections.
fn row_end_distances(_xs: &[f64], y: f64, poly: &[Pt], prev: Pt) -> (f64, f64) {
    let hits = MappingEngine::scanline_x_hits(poly, y);
    if hits.len() < 2 {
        return (f64::INFINITY, f64::INFINITY);
    }
    let xl = hits[0];
    let xr = hits[hits.len() - 1];
    let dl = MappingEngine::dist(Pt { x: xl, y }, prev);
    let dr = MappingEngine::dist(Pt { x: xr, y }, prev);
    (dl, dr)
}

/// Returns (dist_to_bottom_end, dist_to_top_end) for the first/last inside
/// y-point on a vertical column using exact edge intersections.
fn col_end_distances(_ys: &[f64], x: f64, poly: &[Pt], prev: Pt) -> (f64, f64) {
    let hits = MappingEngine::scanline_y_hits(poly, x);
    if hits.len() < 2 {
        return (f64::INFINITY, f64::INFINITY);
    }
    let yb = hits[0];
    let yt = hits[hits.len() - 1];
    let db = MappingEngine::dist(Pt { x, y: yb }, prev);
    let dt = MappingEngine::dist(Pt { x, y: yt }, prev);
    (db, dt)
}

/// Haversine distance between two coordinates (metres).
pub fn haversine(a: LatLng, b: LatLng) -> f64 {
    const R: f64 = 6_371_000.0;
    let dlat = (b.lat - a.lat) * PI / 180.0;
    let dlng = (b.lng - a.lng) * PI / 180.0;
    let lat1 = a.lat * PI / 180.0;
    let lat2 = b.lat * PI / 180.0;
    let s = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlng / 2.0).sin().powi(2);
    2.0 * R * s.sqrt().atan2((1.0 - s).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_square_generates_waypoints() {
        let engine = MappingEngine {
            altitude: 50.0,
            ground_offset: 0.0,
            forward_overlap: 0.6,
            side_overlap: 0.4,
            sensor_width: 13.2,
            sensor_height: 8.8,
            focal_length: 8.8,
            image_width: 4000,
            image_height: 3000,
            angle: 0.0,
        };
        let polygon = vec![
            LatLng {
                lat: 51.500,
                lng: -0.100,
            },
            LatLng {
                lat: 51.501,
                lng: -0.100,
            },
            LatLng {
                lat: 51.501,
                lng: -0.099,
            },
            LatLng {
                lat: 51.500,
                lng: -0.099,
            },
        ];
        let wps = engine.generate_waypoints(&polygon, false, false, None);
        assert!(!wps.is_empty(), "expected at least one waypoint");
    }
}
