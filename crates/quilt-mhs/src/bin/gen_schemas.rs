//! Generate the JSON Schemas (repo-root `schemas/`) from the Rust types,
//! so type and schema can never drift. Run: `cargo run --bin gen-schemas`.
//! CI (tests/schemas.rs) fails if the committed files differ.

use quilt_mhs::mhs::types::{AbortReceipt, Command, DeviceManifest, ProgramReceipt, Sample, SafetyEnvelope};

fn write_schema<T: schemars::JsonSchema>(dir: &std::path::Path, name: &str) -> std::io::Result<()> {
    let schema = schemars::schema_for!(T);
    let json = serde_json::to_string_pretty(&schema).unwrap();
    std::fs::write(dir.join(name), json + "\n")
}

fn main() -> std::io::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    std::fs::create_dir_all(&root)?;
    write_schema::<DeviceManifest>(&root, "mhs-device-manifest.schema.json")?;
    write_schema::<SafetyEnvelope>(&root, "mhs-safety-envelope.schema.json")?;
    write_schema::<Command>(&root, "mhs-command.schema.json")?;
    write_schema::<Sample>(&root, "mhs-telemetry-sample.schema.json")?;
    write_schema::<ProgramReceipt>(&root, "mhs-program-receipt.schema.json")?;
    write_schema::<AbortReceipt>(&root, "mhs-abort-receipt.schema.json")?;
    println!("schemas written to {}", root.display());
    Ok(())
}
