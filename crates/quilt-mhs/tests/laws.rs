//! The 5+1+1 laws of the quilt cellular architecture, enforced through
//! the quilt→MHS adapter against the MockMHS transport. Law numbering
//! follows quilt-cellular-arch INDEX.md (FRAMEWORK.md numbers
//! super-relevance as the 6th law and treats FORGET as the +1; same
//! seven facts, two orderings).

use quilt_mhs::controller::QuiltMhsAdapter;
use quilt_mhs::mhs::mock::MockMHS;
use quilt_mhs::mhs::types::*;
use quilt_mhs::MhsClient;

fn adapter() -> QuiltMhsAdapter<MockMHS> {
    QuiltMhsAdapter::new(MockMHS::new())
}

// Law 1 — BIND_idempotence: BIND(n,v); BIND(n,v) = BIND(n,v).
#[test]
fn bind_is_idempotent() {
    let mut a = adapter();
    a.bind("headroom", MhsValue::Float(1.0)).unwrap();
    a.bind("headroom", MhsValue::Float(1.0)).unwrap(); // law: no-op
    let binds = a.journal.iter().filter(|e| matches!(e, quilt_mhs::controller::OpEvent::Bind { .. })).count();
    assert_eq!(binds, 1, "second identical BIND must journal nothing");
    // a DIFFERENT value on the same name is a new binding → refused
    assert!(a.bind("headroom", MhsValue::Float(2.0)).is_err());
}

// Law 2 — LINK_transitivity: a→b + b→c ⟹ a reaches c.
#[test]
fn links_are_transitive() {
    let mut a = adapter();
    a.bind("a", MhsValue::Float(0.0)).unwrap();
    a.bind("b", MhsValue::Null).unwrap();
    a.bind("c", MhsValue::Null).unwrap();
    a.link("a", "b").unwrap();
    a.link("b", "c").unwrap();
    let closure = a.link_closure("a");
    assert!(closure.contains("b") && closure.contains("c"), "closure(a) must reach c: {closure:?}");
}

// Law 2, operational form — a linked chain propagates an effect to the
// device-bound tail: sensor→formula→actuator is three cells, one write.
#[test]
fn effect_propagates_across_links_to_device() {
    let mut a = adapter();
    a.bind("target.deg", MhsValue::Float(0.0)).unwrap();
    a.bind_to_device("arm.target", &"mock-arm-01".to_string(), "joint1.target", &["captain".to_string()]).unwrap();
    a.link("target.deg", "mid").unwrap();
    a.link("mid", "arm.target").unwrap();
    let samples = a.effect("target.deg", MhsValue::Float(12.5)).unwrap();
    assert_eq!(samples.len(), 1, "exactly one device write from the chain");
    assert_eq!(samples[0].value, MhsValue::Float(12.5));
    assert_eq!(samples[0].channel, "joint1.target");
    // transitive closure says head reaches the device tail
    assert!(a.link_closure("target.deg").contains("arm.target"));
}

// Law 3 — EFFECT_associativity: grouping invariance. Applying the same
// effect set with different groupings lands identical values on identical
// channels in identical order.
#[test]
fn effect_grouping_is_invariant() {
    let mk = |grouping: usize| {
        let mut a = adapter();
        a.bind_to_device("arm", &"mock-arm-01".to_string(), "joint1.target", &[]).unwrap();
        a.bind_to_device("bath", &"mock-thermal-01".to_string(), "bath.setpoint", &[]).unwrap();
        match grouping {
            0 => {
                a.effect_batch(vec![
                    ("arm".into(), MhsValue::Float(5.0)),
                    ("bath".into(), MhsValue::Float(30.0)),
                    ("arm".into(), MhsValue::Float(6.0)),
                ])
                .unwrap();
            }
            _ => {
                a.effect_batch(vec![("arm".into(), MhsValue::Float(5.0))]).unwrap();
                a.effect_batch(vec![
                    ("bath".into(), MhsValue::Float(30.0)),
                    ("arm".into(), MhsValue::Float(6.0)),
                ])
                .unwrap();
            }
        }
        // collect the device write sequence
        a.journal
            .iter()
            .filter_map(|e| match e {
                quilt_mhs::controller::OpEvent::EffectDevice { channel, value, .. } => {
                    Some((channel.clone(), value.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(mk(0), mk(1), "(f∘g)∘h and f∘(g∘h) must land the same writes");
}

// Law 4 — VIEW_purity: VIEW never mutates device state, never journals a
// write, never advances the clock.
#[test]
fn view_is_pure() {
    let mut a = adapter();
    a.bind_to_device("temp", &"mock-thermal-01".to_string(), "bath.temperature", &[]).unwrap();
    let writes_before = a.client_mut().write_count();
    let clock_before = a.clock;
    let v1 = a.view("temp").unwrap();
    let v2 = a.view("temp").unwrap();
    let v3 = a.view("temp").unwrap();
    assert_eq!(v1, v2);
    assert_eq!(v2, v3);
    assert_eq!(a.client_mut().write_count(), writes_before, "VIEW must not write");
    assert_eq!(a.clock, clock_before, "VIEW must not advance time");
}

// Law 5 — TICK_monotonicity: the clock only advances.
#[test]
fn tick_is_monotonic() {
    let mut a = adapter();
    assert_eq!(a.tick(1.0).unwrap(), 1.0);
    assert_eq!(a.tick(2.5).unwrap(), 3.5);
    assert!(matches!(a.tick(0.0), Err(MhsError::NotMonotonic)));
    assert!(matches!(a.tick(-1.0), Err(MhsError::NotMonotonic)));
    assert_eq!(a.clock, 3.5);
    // TICK also drives the machine's own loop (mock control step):
    // write a setpoint, tick, the bath relaxes toward it.
    a.bind_to_device("sp", &"mock-thermal-01".to_string(), "bath.setpoint", &[]).unwrap();
    a.bind_to_device("temp", &"mock-thermal-01".to_string(), "bath.temperature", &[]).unwrap();
    a.effect("sp", MhsValue::Float(90.0)).unwrap();
    let t0 = a.view("temp").unwrap().as_f64().unwrap();
    a.tick(10.0).unwrap();
    let t1 = a.view("temp").unwrap().as_f64().unwrap();
    assert!(t1 > t0, "bath must relax toward setpoint after TICK ({t0} -> {t1})");
}

// Law 6 — Super-relevance: a channel satisfying multiple hands is more
// fit (ranks above) one satisfying a single hand.
#[test]
fn super_relevance_ranks_multi_hand_channels() {
    let mut a = adapter();
    a.bind_to_device("arm.a", &"mock-arm-01".to_string(), "joint1.target", &["captain".to_string(), "curator".to_string()]).unwrap();
    a.bind_to_device("bath.a", &"mock-thermal-01".to_string(), "bath.setpoint", &["captain".to_string()]).unwrap();
    let ranked = a.ranked_channels();
    assert_eq!(ranked[0].0 .1, "joint1.target", "two hands beat one");
    assert_eq!(ranked[0].1, 2);
    // forgetting the multi-hand cell removes its relevance contribution
    a.forget("arm.a").unwrap();
    let ranked = a.ranked_channels();
    assert_eq!(ranked[0].0 .1, "bath.setpoint", "relevance follows living cells only");
}

// Law 7 — FORGET_completeness: FORGET(x) removes all traces of x within
// the laws — and because interlocks are first-class forgettable state,
// forgetting the last grant-holder parks the machine.
#[test]
fn forget_is_complete() {
    let mut a = adapter();
    a.bind("upstream", MhsValue::Float(1.0)).unwrap();
    a.bind_to_device("grip", &"mock-arm-01".to_string(), "gripper.cmd", &["captain".to_string()]).unwrap();
    a.link("upstream", "grip").unwrap();
    a.grant("grip", &"mock-arm-01".to_string(), "gripper.cmd").unwrap();

    // destructive write now allowed
    a.effect("grip", MhsValue::Float(1.0)).unwrap();

    let receipt = a.forget("grip").unwrap();
    assert!(receipt.links_removed >= 1, "inbound link removed");
    assert_eq!(receipt.grants_released, 1, "grant released");
    assert_eq!(receipt.devices_aborted, vec!["mock-arm-01".to_string()], "last interlock forgotten -> abort");

    // no trace the laws can see:
    assert!(a.view("grip").is_err(), "cell gone");
    assert!(a.effect("grip", MhsValue::Float(0.0)).is_err(), "effect gone");
    assert!(a.interlocks().is_empty(), "interlocks empty");
    assert!(!a.link_closure("upstream").contains("grip"), "links gone");
    // the machine itself is latched — writes refused until an operator clears
    let e = a.client_mut().write(&"mock-arm-01".to_string(), "joint1.target", MhsValue::Float(0.0)).unwrap_err();
    assert!(matches!(e, MhsError::Aborted(_)), "device must be latched, got {e:?}");
}

// The safety envelope holds at the adapter seam: out-of-limit effects are
// rejected by the DEVICE and the cell is unchanged.
#[test]
fn device_enforces_limits_through_adapter() {
    let mut a = adapter();
    a.bind_to_device("arm", &"mock-arm-01".to_string(), "joint1.target", &[]).unwrap();
    let e = a.effect("arm", MhsValue::Float(120.0)).unwrap_err(); // limit is ±90
    assert!(matches!(e, MhsError::SafetyViolation(..)));
    let v = a.view("arm").unwrap();
    assert_eq!(v, MhsValue::Float(0.0), "cell value unchanged after rejection");
}

// Destructive channels are interlock-gated end to end.
#[test]
fn destructive_writes_require_grant() {
    let mut a = adapter();
    a.bind_to_device("grip", &"mock-arm-01".to_string(), "gripper.cmd", &[]).unwrap();
    let e = a.effect("grip", MhsValue::Float(1.0)).unwrap_err();
    assert!(matches!(e, MhsError::GrantRequired(_)));
    a.grant("grip", &"mock-arm-01".to_string(), "gripper.cmd").unwrap();
    a.effect("grip", MhsValue::Float(1.0)).unwrap();
}
