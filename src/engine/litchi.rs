//! Litchi Mission Hub CSV generator.
//!
//! Column spec matches the Litchi CSV format exactly.

use crate::engine::mapping::LatLng;

/// Generate a Litchi-compatible CSV string from a list of waypoints.
///
/// * `altitude`      – mission altitude in metres (all WPs share it)
/// * `speed`         – flight speed in m/s
/// * `camera_angle`  – gimbal pitch in degrees (e.g. -90)
pub fn generate_csv(
    waypoints: &[LatLng],
    altitude: f64,
    speed: f64,
    camera_angle: i32,
) -> String {
    let header = "latitude,longitude,altitude(m),heading(deg),curvesize(m),\
rotationdir,gimbalmode,gimbalpitchangle,\
actiontype1,actionparam1,\
altitudemode,speed(m/s),\
poi_latitude,poi_longitude,poi_altitude(m),poi_altitudemode,\
photo_timeinterval,photo_distinterval";

    let rows: Vec<String> = waypoints
        .iter()
        .map(|wp| {
            format!(
                "{lat},{lng},{alt},0,0,0,2,{angle},1,0,0,{speed},0,0,0,0,-1,-1",
                lat = wp.lat,
                lng = wp.lng,
                alt = altitude,
                angle = camera_angle,
                speed = speed,
            )
        })
        .collect();

    let mut out = header.to_string();
    for row in rows {
        out.push('\n');
        out.push_str(&row);
    }
    out
}
