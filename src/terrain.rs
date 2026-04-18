//! Terrain elevation lookup using the Open-Elevation public API.
//!
//! Requests are run on a background thread so the GTK main loop is never
//! blocked.  Results are delivered back via an mpsc channel polled from
//! `glib::idle_add_local`.

use crate::engine::mapping::LatLng;

#[derive(Debug, Clone)]
pub enum ElevationResult {
    Ok(Vec<f64>),
    Err(String),
}

/// Fetch terrain elevation (metres ASL) for every waypoint in `points`.
///
/// The callback `on_done` is invoked on the GTK main thread when the request
/// completes.
pub fn fetch_elevations<F>(points: Vec<LatLng>, on_done: F)
where
    F: Fn(ElevationResult) + 'static,
{
    // Deliver result back on the GTK main loop via mpsc + idle_add_local.
    const CHUNK: usize = 100;
    let (tx, rx) = std::sync::mpsc::sync_channel::<ElevationResult>(1);

    std::thread::spawn(move || {
        let mut all_elevations: Vec<f64> = Vec::with_capacity(points.len());

        for chunk in points.chunks(CHUNK) {
            let locations: Vec<serde_json::Value> = chunk
                .iter()
                .map(|p| {
                    serde_json::json!({ "latitude": p.lat, "longitude": p.lng })
                })
                .collect();
            let body = serde_json::json!({ "locations": locations });

            let resp = ureq::post(
                "https://api.open-elevation.com/api/v1/lookup",
            )
            .set("Content-Type", "application/json")
            .send_json(body);

            match resp {
                Ok(r) => {
                    let json: serde_json::Value =
                        match r.into_json() {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = tx.send(ElevationResult::Err(
                                    format!("JSON parse error: {e}"),
                                ));
                                return;
                            }
                        };
                    if let Some(results) =
                        json.get("results").and_then(|v| v.as_array())
                    {
                        for r in results {
                            let elev = r
                                .get("elevation")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            all_elevations.push(elev);
                        }
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(ElevationResult::Err(format!("HTTP error: {e}")));
                    return;
                }
            }
        }

        let _ = tx.send(ElevationResult::Ok(all_elevations));
    });

    // Poll from the GTK main loop until the background thread sends the result.
    glib::idle_add_local(move || {
        use std::sync::mpsc::TryRecvError;
        match rx.try_recv() {
            Ok(result) => {
                on_done(result);
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                on_done(ElevationResult::Err("Elevation thread disconnected".into()));
                glib::ControlFlow::Break
            }
        }
    });
}
