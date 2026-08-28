//! Schema contract: the committed JSON Schemas in `schemas/` are exactly
//! what the Rust types generate (no drift), they parse as JSON, and the
//! serde types round-trip instances losslessly. Non-Rust ports validate
//! against these files; this test is what keeps them honest.

use quilt_mhs::mhs::types::*;
use serde_json::Value;
use std::path::PathBuf;

fn repo_schemas() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

#[test]
fn committed_schemas_match_generated() {
    let dir = repo_schemas();
    for (name, ty) in [
        ("mhs-device-manifest.schema.json", "DeviceManifest"),
        ("mhs-safety-envelope.schema.json", "SafetyEnvelope"),
        ("mhs-command.schema.json", "Command"),
        ("mhs-telemetry-sample.schema.json", "Sample"),
        ("mhs-program-receipt.schema.json", "ProgramReceipt"),
        ("mhs-abort-receipt.schema.json", "AbortReceipt"),
    ] {
        let committed: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))).unwrap();
        // regenerate from the live types via the same code path as
        // `cargo run --bin gen-schemas`
        let generated = match ty {
            "DeviceManifest" => serde_json::to_value(schemars::schema_for!(DeviceManifest)).unwrap(),
            "SafetyEnvelope" => serde_json::to_value(schemars::schema_for!(SafetyEnvelope)).unwrap(),
            "Command" => serde_json::to_value(schemars::schema_for!(Command)).unwrap(),
            "Sample" => serde_json::to_value(schemars::schema_for!(Sample)).unwrap(),
            "ProgramReceipt" => serde_json::to_value(schemars::schema_for!(ProgramReceipt)).unwrap(),
            _ => serde_json::to_value(schemars::schema_for!(AbortReceipt)).unwrap(),
        };
        assert_eq!(committed, generated, "{name} drifted from types — run `cargo run --bin gen-schemas`");
    }
}

#[test]
fn manifest_round_trips() {
    let m = quilt_mhs::mhs::mock::mock_arm();
    let json = serde_json::to_string(&m).unwrap();
    let back: DeviceManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(m, back);
    // the envelope and a sample too
    let s = Sample { device: "d".into(), channel: "c".into(), value: MhsValue::Float(1.5), t: 2.0 };
    assert_eq!(serde_json::from_str::<Sample>(&serde_json::to_string(&s).unwrap()).unwrap(), s);
}

#[test]
fn examples_parse_into_types() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // every example declares which type it exemplifies
        let ty = v["$type"].as_str().expect("examples carry a $type discriminator").to_string();
        match ty.as_str() {
            "DeviceManifest" => {
                let _: DeviceManifest = serde_json::from_value(v).unwrap();
            }
            "SafetyEnvelope" => {
                let _: SafetyEnvelope = serde_json::from_value(v).unwrap();
            }
            "Command" => {
                let _: Command = serde_json::from_value(v).unwrap();
            }
            "Sample" => {
                let _: Sample = serde_json::from_value(v).unwrap();
            }
            other => panic!("unknown $type {other}"),
        }
    }
}
