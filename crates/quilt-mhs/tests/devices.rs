//! Phase 215 — Tests for the 3 new canonical devices: incubator, microscope,
//! plate-handler. All derived from the MHS-SPEC-WATCH.md F16 partner list
//! and the device-cookbook.md pattern.

use quilt_mhs::mhs::client::MhsClient;
use quilt_mhs::mhs::mock::{
    mock_incubator, mock_incubator_state, mock_microscope, mock_microscope_state,
    mock_plate_handler, mock_plate_handler_state, MockMHS,
};
use quilt_mhs::mhs::types::*;

// ── incubator (Thermo Fisher Heracell VIOS-class) ────────────────────

#[test]
fn incubator_manifest_is_well_formed() {
    let m = mock_incubator();
    assert_eq!(m.device_id, "mock-incubator-01");
    assert!(!m.tags.is_empty(), "incubator needs natural-language tags");
    // door.lock is destructive (per tag: unlatching mid-experiment collapses gas)
    let door = m.writable.iter().find(|c| c.name == "door.lock").unwrap();
    assert!(door.destructive, "door.lock must be destructive");
    // CO2 setpoint 0..20
    let co2 = m.safety.channel_limits.get("chamber.co2_setpoint").unwrap();
    assert_eq!(*co2, (0.0, 20.0));
}

#[test]
fn incubator_door_requires_grant() {
    let mut m = MockMHS::new();
    // default grant_required = true
    let e = m.write(&"mock-incubator-01".into(), "door.lock", MhsValue::Float(1.0)).unwrap_err();
    assert!(matches!(e, MhsError::GrantRequired(_)), "expected GrantRequired, got {e:?}");
    m.hold_grant(&"mock-incubator-01".into(), "door.lock").unwrap();
    m.write(&"mock-incubator-01".into(), "door.lock", MhsValue::Float(1.0)).unwrap();
}

#[test]
fn incubator_thermal_relaxes_toward_setpoint() {
    let mut m = MockMHS::new();
    // chamber starts at 37.0; drive to 40.0 and tick.
    m.write(&"mock-incubator-01".into(), "chamber.setpoint", MhsValue::Float(40.0)).unwrap();
    let before = m.read(&"mock-incubator-01".into(), "chamber.temperature").unwrap();
    m.tick(30.0).unwrap(); // 30s
    let after = m.read(&"mock-incubator-01".into(), "chamber.temperature").unwrap();
    let bf = before.value.as_f64().unwrap();
    let af = after.value.as_f64().unwrap();
    assert!((37.0..=40.0).contains(&bf) && (bf..=40.0).contains(&af),
            "relaxation toward setpoint: {bf} -> {af}");
    assert!(af > bf, "after tick, temperature must increase toward setpoint: bf={bf} af={af}");
}

#[test]
fn incubator_out_of_range_rejected() {
    let mut m = MockMHS::new();
    // setpoint > 50 °C is out of declared range
    let e = m.write(&"mock-incubator-01".into(), "chamber.setpoint", MhsValue::Float(75.0)).unwrap_err();
    assert!(matches!(e, MhsError::SafetyViolation(..)), "expected SafetyViolation, got {e:?}");
}

// ── microscope (Zeiss Axio Observer 7 + Hamamatsu ORCA-Flash 4.0) ────

#[test]
fn microscope_manifest_lists_camera_and_stage() {
    let m = mock_microscope();
    let names: Vec<&str> = m.writable.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"stage.x_target") && names.contains(&"stage.y_target"),
            "XY stage targets must be writable: {names:?}");
    assert!(names.contains(&"camera.exposure_ms") && names.contains(&"camera.gain"),
            "camera controls must be writable: {names:?}");
    // camera.intensity is the only readable feedback
    let reads: Vec<&str> = m.readable.iter().map(|c| c.name.as_str()).collect();
    assert!(reads.contains(&"camera.intensity"), "intensity feedback missing: {reads:?}");
}

#[test]
fn microscope_stage_relaxes_quickly() {
    let mut m = MockMHS::new();
    m.write(&"mock-microscope-01".into(), "stage.x_target", MhsValue::Float(10.0)).unwrap();
    m.tick(0.5).unwrap(); // 0.5 s; τ=50ms so should reach ~63% in 50ms, ~99% in 500ms
    let after = m.read(&"mock-microscope-01".into(), "stage.x").unwrap();
    let af = after.value.as_f64().unwrap();
    assert!(af > 9.0, "after 0.5s stage should be very near target: got {af}");
}

// ── plate-handler (Tecan Fluent-class) ──────────────────────────────

#[test]
fn plate_handler_overvolume_rejected() {
    let mut m = MockMHS::new();
    // 250 µL > 200 µL max — well overflow
    let e = m.write(&"mock-plate-handler-01".into(), "pipette.volume_ul_target", MhsValue::Float(250.0)).unwrap_err();
    assert!(matches!(e, MhsError::SafetyViolation(..)), "expected SafetyViolation, got {e:?}");
}

#[test]
fn plate_handler_volume_under_grant_works() {
    let mut m = MockMHS::new();
    m.hold_grant(&"mock-plate-handler-01".into(), "pipette.volume_ul_target").unwrap();
    m.write(&"mock-plate-handler-01".into(), "pipette.volume_ul_target", MhsValue::Float(150.0)).unwrap();
    let after = m.read(&"mock-plate-handler-01".into(), "pipette.volume_ul").unwrap();
    assert_eq!(after.value.as_f64().unwrap(), 150.0);
}

#[test]
fn plate_handler_xyz_axes_advance_independently() {
    let mut m = MockMHS::new();
    m.write(&"mock-plate-handler-01".into(), "arm.x_target", MhsValue::Float(100.0)).unwrap();
    m.write(&"mock-plate-handler-01".into(), "arm.y_target", MhsValue::Float(50.0)).unwrap();
    m.tick(0.5).unwrap();
    let x = m.read(&"mock-plate-handler-01".into(), "arm.x").unwrap().value.as_f64().unwrap();
    let y = m.read(&"mock-plate-handler-01".into(), "arm.y").unwrap().value.as_f64().unwrap();
    assert!(x > 50.0 && y > 25.0, "both axes must approach their targets: x={x} y={y}");
}

// ── Test helpers exported (state constructors) are reachable ────────

#[test]
fn all_three_new_devices_expose_valid_state_maps() {
    // Just a smoke test that the state functions are reachable & well-formed
    let _ = mock_incubator_state();
    let _ = mock_microscope_state();
    let _ = mock_plate_handler_state();
}

// ── laser (QuEra-class Rydberg, F12) ─────────────────────────────────

use quilt_mhs::mhs::mock::{mock_laser, mock_laser_state};

#[test]
fn laser_press_citation_is_in_tags() {
    let m = mock_laser();
    let joined = m.tags.join(" ");
    assert!(joined.contains("99.3"), "F12 citation must be in tags: {joined}");
    assert!(joined.contains("QuEra"), "F12 partner name must be in tags: {joined}");
}

#[test]
fn laser_piezo_relaxes_fast() {
    let mut m = MockMHS::new();
    m.write(&"mock-laser-01".into(), "piezo.target", MhsValue::Float(120.0)).unwrap();
    m.tick(0.1).unwrap(); // 100 ms; τ=20ms so should be ~99% there
    let v = m.read(&"mock-laser-01".into(), "piezo.voltage").unwrap().value.as_f64().unwrap();
    assert!(v > 110.0, "after 100ms piezo should be very near 120V target: got {v}");
}

#[test]
fn laser_out_of_range_rejected() {
    let mut m = MockMHS::new();
    let e = m.write(&"mock-laser-01".into(), "piezo.target", MhsValue::Float(200.0)).unwrap_err();
    assert!(matches!(e, MhsError::SafetyViolation(..)), "expected SafetyViolation, got {e:?}");
}

#[test]
fn laser_state_helper_works() {
    let _ = mock_laser_state();
}
