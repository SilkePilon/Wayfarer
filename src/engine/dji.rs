//! DJI Fly (.kmz / WPML) mission file generator.
//!
//! Produces a ZIP archive containing:
//!   wpmz/template.kml   — mission metadata in DJI WPML format
//!   wpmz/waylines.wpml  — waypoints in DJI WPML format

use std::io::{Cursor, Write};
use zip::{write::FileOptions, ZipWriter};

use crate::engine::mapping::LatLng;
use crate::models::mission::{FinishAction, RcLostAction};

// ─── Public entry point ───────────────────────────────────────────────────────

/// Render the full mission to a `.kmz` byte payload.
pub fn generate_kmz(
    waypoints: &[LatLng],
    altitude: f64,
    speed: f64,
    camera_angle: i32,
    delay_at_waypoint: i32,
    heading_angle: Option<i32>,
    finish_action: FinishAction,
    rc_lost_action: RcLostAction,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();

    let template = build_template_kml(now, speed, finish_action, rc_lost_action);
    let waylines = build_waylines_wpml(
        waypoints,
        altitude,
        speed,
        camera_angle,
        delay_at_waypoint,
        heading_angle,
        finish_action,
        rc_lost_action,
    );

    // Pack into ZIP (= KMZ)
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts =
            FileOptions::<()>::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("wpmz/template.kml", opts)?;
        zip.write_all(template.as_bytes())?;
        zip.start_file("wpmz/waylines.wpml", opts)?;
        zip.write_all(waylines.as_bytes())?;
        zip.finish()?;
    }
    Ok(buf)
}

// ─── template.kml ─────────────────────────────────────────────────────────────

fn build_template_kml(
    timestamp_ms: u128,
    global_speed: f64,
    finish_action: FinishAction,
    rc_lost_action: RcLostAction,
) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2" xmlns:wpml="http://www.dji.com/wpmz/1.0.2">
  <Document>
    <wpml:author>fly</wpml:author>
    <wpml:createTime>{ts}</wpml:createTime>
    <wpml:updateTime>{ts}</wpml:updateTime>
    <wpml:missionConfig>
      <wpml:flyToWaylineMode>safely</wpml:flyToWaylineMode>
      <wpml:finishAction>{finish}</wpml:finishAction>
      <wpml:exitOnRCLost>executeLostAction</wpml:exitOnRCLost>
      <wpml:executeRCLostAction>{rc_lost}</wpml:executeRCLostAction>
      <wpml:globalTransitionalSpeed>{speed}</wpml:globalTransitionalSpeed>
      <wpml:droneInfo>
        <wpml:droneEnumValue>68</wpml:droneEnumValue>
        <wpml:droneSubEnumValue>0</wpml:droneSubEnumValue>
      </wpml:droneInfo>
    </wpml:missionConfig>
  </Document>
</kml>"#,
        ts = timestamp_ms,
        finish = finish_action.to_wpml(),
        rc_lost = rc_lost_action.to_wpml(),
        speed = global_speed,
    )
}

// ─── waylines.wpml ────────────────────────────────────────────────────────────

fn build_waylines_wpml(
    waypoints: &[LatLng],
    altitude: f64,
    speed: f64,
    camera_angle: i32,
    delay_at_waypoint: i32,
    heading_angle: Option<i32>,
    finish_action: FinishAction,
    rc_lost_action: RcLostAction,
) -> String {
    let placemarks: String = waypoints
        .iter()
        .enumerate()
        .map(|(idx, wp)| {
            build_placemark(
                idx,
                wp,
                altitude,
                speed,
                camera_angle,
                delay_at_waypoint,
                heading_angle,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2" xmlns:wpml="http://www.dji.com/wpmz/1.0.2">
  <Document>
    <wpml:missionConfig>
      <wpml:flyToWaylineMode>pointToPoint</wpml:flyToWaylineMode>
      <wpml:finishAction>{finish}</wpml:finishAction>
      <wpml:exitOnRCLost>executeLostAction</wpml:exitOnRCLost>
      <wpml:executeRCLostAction>{rc_lost}</wpml:executeRCLostAction>
      <wpml:globalTransitionalSpeed>{speed}</wpml:globalTransitionalSpeed>
      <wpml:droneInfo>
        <wpml:droneEnumValue>68</wpml:droneEnumValue>
        <wpml:droneSubEnumValue>0</wpml:droneSubEnumValue>
      </wpml:droneInfo>
    </wpml:missionConfig>
    <Folder>
      <wpml:templateId>0</wpml:templateId>
      <wpml:executeHeightMode>relativeToStartPoint</wpml:executeHeightMode>
      <wpml:waylineId>0</wpml:waylineId>
      <wpml:distance>0</wpml:distance>
      <wpml:duration>0</wpml:duration>
      <wpml:autoFlightSpeed>{speed}</wpml:autoFlightSpeed>
{placemarks}
    </Folder>
  </Document>
</kml>"#,
        finish = finish_action.to_wpml(),
        rc_lost = rc_lost_action.to_wpml(),
        speed = speed,
        placemarks = placemarks,
    )
}

fn build_placemark(
    index: usize,
    wp: &LatLng,
    altitude: f64,
    speed: f64,
    camera_angle: i32,
    delay_at_waypoint: i32,
    heading_angle: Option<i32>,
) -> String {
    // Actions run in sequence when the drone reaches this waypoint.
    // Order: gimbal rotate (first wp) → hover (settle) → hover (user delay) → photo
    let mut actions = String::new();
    let mut action_id = 0usize;

    // Gimbal pitch — only on the first waypoint (it stays at that angle)
    if index == 0 {
        actions.push_str(&format!(
            r#"        <wpml:action>
          <wpml:actionId>{aid}</wpml:actionId>
          <wpml:actionActuatorFunc>gimbalRotate</wpml:actionActuatorFunc>
          <wpml:actionActuatorFuncParam>
            <wpml:gimbalRotateMode>absoluteAngle</wpml:gimbalRotateMode>
            <wpml:gimbalPitchRotateEnable>1</wpml:gimbalPitchRotateEnable>
            <wpml:gimbalPitchRotateAngle>{angle}</wpml:gimbalPitchRotateAngle>
            <wpml:gimbalRollRotateEnable>0</wpml:gimbalRollRotateEnable>
            <wpml:gimbalRollRotateAngle>0</wpml:gimbalRollRotateAngle>
            <wpml:gimbalYawRotateEnable>0</wpml:gimbalYawRotateEnable>
            <wpml:gimbalYawRotateAngle>0</wpml:gimbalYawRotateAngle>
            <wpml:gimbalRotateTimeEnable>1</wpml:gimbalRotateTimeEnable>
            <wpml:gimbalRotateTime>2</wpml:gimbalRotateTime>
            <wpml:payloadPositionIndex>0</wpml:payloadPositionIndex>
          </wpml:actionActuatorFuncParam>
        </wpml:action>
"#,
            aid = action_id,
            angle = camera_angle,
        ));
        action_id += 1;

        // Wait for gimbal to physically reach the target angle
        actions.push_str(&format!(
            r#"        <wpml:action>
          <wpml:actionId>{aid}</wpml:actionId>
          <wpml:actionActuatorFunc>hover</wpml:actionActuatorFunc>
          <wpml:actionActuatorFuncParam>
            <wpml:hoverTime>3</wpml:hoverTime>
          </wpml:actionActuatorFuncParam>
        </wpml:action>
"#,
            aid = action_id,
        ));
        action_id += 1;
    }

    // Optional hover (before photo, so gimbal has time to settle)
    if delay_at_waypoint > 0 {
        actions.push_str(&format!(
            r#"        <wpml:action>
          <wpml:actionId>{aid}</wpml:actionId>
          <wpml:actionActuatorFunc>hover</wpml:actionActuatorFunc>
          <wpml:actionActuatorFuncParam>
            <wpml:hoverTime>{delay}</wpml:hoverTime>
          </wpml:actionActuatorFuncParam>
        </wpml:action>
"#,
            aid = action_id,
            delay = delay_at_waypoint,
        ));
        action_id += 1;
    }

    // Take photo
    actions.push_str(&format!(
        r#"        <wpml:action>
          <wpml:actionId>{aid}</wpml:actionId>
          <wpml:actionActuatorFunc>takePhoto</wpml:actionActuatorFunc>
          <wpml:actionActuatorFuncParam>
            <wpml:payloadPositionIndex>0</wpml:payloadPositionIndex>
          </wpml:actionActuatorFuncParam>
        </wpml:action>
"#,
        aid = action_id,
    ));
    let _ = action_id; // suppress unused warning

    // Heading configuration
    let (heading_mode, heading_deg, heading_enable) = match heading_angle {
        Some(angle) => ("smoothTransition", angle, 1),
        None => ("followWayline", 0, 0),
    };

    format!(
        r#"      <Placemark>
        <Point>
          <coordinates>{lng},{lat}</coordinates>
        </Point>
        <wpml:index>{idx}</wpml:index>
        <wpml:executeHeight>{alt}</wpml:executeHeight>
        <wpml:waypointSpeed>{spd}</wpml:waypointSpeed>
        <wpml:waypointHeadingParam>
          <wpml:waypointHeadingMode>{heading_mode}</wpml:waypointHeadingMode>
          <wpml:waypointHeadingAngle>{heading_deg}</wpml:waypointHeadingAngle>
          <wpml:waypointPoiPoint>0.000000,0.000000,0.000000</wpml:waypointPoiPoint>
          <wpml:waypointHeadingAngleEnable>{heading_enable}</wpml:waypointHeadingAngleEnable>
          <wpml:waypointHeadingPathMode>followBadArc</wpml:waypointHeadingPathMode>
        </wpml:waypointHeadingParam>
        <wpml:waypointTurnParam>
          <wpml:waypointTurnMode>toPointAndStopWithDiscontinuityCurvature</wpml:waypointTurnMode>
          <wpml:waypointTurnDampingDist>0</wpml:waypointTurnDampingDist>
        </wpml:waypointTurnParam>
        <wpml:useStraightLine>1</wpml:useStraightLine>
        <wpml:actionGroup>
          <wpml:actionGroupId>{idx}</wpml:actionGroupId>
          <wpml:actionGroupStartIndex>{idx}</wpml:actionGroupStartIndex>
          <wpml:actionGroupEndIndex>{idx}</wpml:actionGroupEndIndex>
          <wpml:actionGroupMode>sequence</wpml:actionGroupMode>
          <wpml:actionTrigger>
            <wpml:actionTriggerType>reachPoint</wpml:actionTriggerType>
          </wpml:actionTrigger>
{actions}</wpml:actionGroup>
      </Placemark>"#,
        lng = wp.lng,
        lat = wp.lat,
        idx = index,
        alt = altitude as i64,
        spd = speed,
        actions = actions,
    )
}
