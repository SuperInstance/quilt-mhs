//! Inter-quilt federation: two quilt runtimes operating each other
//! through MHS-shaped messaging — runtime A's controller drives runtime
//! B's sheet, and symmetrically. This is the same surface an external
//! agent (Claude, another fleet) would use against a quilt substrate.

use quilt_mhs::device::federation::{FederationPair, PairSide};
use quilt_mhs::mhs::types::*;

#[test]
fn a_discovers_and_drives_b() {
    let mut pair = FederationPair::demo();
    // A discovers B's runtime as an MHS device
    let devices = pair.a.discover().unwrap();
    assert_eq!(devices, vec!["quilt-B".to_string()]);

    // A reads B's manifest — cells appear as channels, contracts as limits
    let m = pair.a.manifest(&"quilt-B".to_string()).unwrap();
    assert!(m.writable.iter().any(|c| c.name == "pump.duty"), "IO cell exposed writable");
    assert!(m.safety.channel_limits.contains_key("pump.duty"), "contract becomes enforced limit");

    // A binds a cell to B's pump and effects it
    pair.a.bind_to_device("b.pump", &"quilt-B".to_string(), "pump.duty", &["captain.a".to_string()]).unwrap();
    let samples = pair.a.effect("b.pump", MhsValue::Float(60.0)).unwrap();
    assert_eq!(samples.len(), 1);
    // B's sheet actually changed — the write crossed the federation link
    assert_eq!(pair.sheet(PairSide::B).sheet.get("pump.duty"), Some(&MhsValue::Float(60.0)));

    // A views B's sensor telemetry
    pair.a.bind_to_device("b.bilge", &"quilt-B".to_string(), "bilge.depth", &[]).unwrap();
    let v = pair.a.view("b.bilge").unwrap();
    assert_eq!(v, MhsValue::Float(0.12));
}

#[test]
fn envelope_enforced_across_the_link() {
    let mut pair = FederationPair::demo();
    pair.a.bind_to_device("b.pump", &"quilt-B".to_string(), "pump.duty", &[]).unwrap();
    // out-of-contract write rejected BY B, not by A's politeness
    let e = pair.a.effect("b.pump", MhsValue::Float(150.0)).unwrap_err(); // contract 0..100
    assert!(matches!(e, MhsError::SafetyViolation(..)));
    assert_eq!(pair.sheet(PairSide::B).sheet.get("pump.duty"), Some(&MhsValue::Float(0.0)), "B unchanged");
}

#[test]
fn destructive_cells_need_interlock_across_the_link() {
    let mut pair = FederationPair::demo();
    pair.a.bind_to_device("b.scram", &"quilt-B".to_string(), "engine.scram", &[]).unwrap();
    let e = pair.a.effect("b.scram", MhsValue::Bool(true)).unwrap_err();
    assert!(matches!(e, MhsError::GrantRequired(_)));
    pair.a.grant("b.scram", &"quilt-B".to_string(), "engine.scram").unwrap();
    pair.a.effect("b.scram", MhsValue::Bool(true)).unwrap();
    assert_eq!(pair.sheet(PairSide::B).sheet.get("engine.scram"), Some(&MhsValue::Bool(true)));
}

#[test]
fn forget_parks_the_federated_runtime() {
    let mut pair = FederationPair::demo();
    pair.a.bind_to_device("b.scram", &"quilt-B".to_string(), "engine.scram", &["hand.a".to_string()]).unwrap();
    pair.a.grant("b.scram", &"quilt-B".to_string(), "engine.scram").unwrap();

    // drive something first so we can see the park
    pair.a.bind_to_device("b.throttle", &"quilt-B".to_string(), "engine.throttle", &[]).unwrap();
    pair.a.effect("b.throttle", MhsValue::Float(40.0)).unwrap();

    // FORGET the interlock-holding cell → last grant → B aborts and parks
    let receipt = pair.a.forget("b.scram").unwrap();
    assert_eq!(receipt.devices_aborted, vec!["quilt-B".to_string()]);
    // parked: ranged IO cells driven to contract floor
    assert_eq!(pair.sheet(PairSide::B).sheet.get("engine.throttle"), Some(&MhsValue::Float(0.0)));
    // latched: further writes refused
    let e = pair.a.effect("b.throttle", MhsValue::Float(10.0)).unwrap_err();
    assert!(matches!(e, MhsError::Aborted(_)));
}

#[test]
fn federation_is_symmetric() {
    let mut pair = FederationPair::demo();
    // A → B ...
    pair.a.bind_to_device("b.pump", &"quilt-B".to_string(), "pump.duty", &[]).unwrap();
    pair.a.effect("b.pump", MhsValue::Float(25.0)).unwrap();
    // ... and B → A in the same session
    pair.b.bind_to_device("a.throttle", &"quilt-A".to_string(), "engine.throttle", &[]).unwrap();
    pair.b.effect("a.throttle", MhsValue::Float(55.0)).unwrap();
    assert_eq!(pair.sheet(PairSide::B).sheet.get("pump.duty"), Some(&MhsValue::Float(25.0)));
    assert_eq!(pair.sheet(PairSide::A).sheet.get("engine.throttle"), Some(&MhsValue::Float(55.0)));
}
