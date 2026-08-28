//! The conformance contract, differentially enforced: MockMHS passes,
//! a quilt-as-device profile passes, and a transport that lies about
//! safety FAILS. When the real MHS SDK lands, its adapter runs this same
//! suite (see PORTING.md).

use quilt_mhs::device::QuiltDeviceProfile;
use quilt_mhs::mhs::client::MhsClient;
use quilt_mhs::mhs::conformance::{core_passes, run_conformance};
use quilt_mhs::mhs::mock::MockMHS;
use quilt_mhs::mhs::types::*;

#[test]
fn mock_mhs_passes_conformance() {
    let results = run_conformance(&mut MockMHS::new());
    for r in &results {
        assert!(r.passed, "{} {} failed: {}", r.id, r.name, r.detail);
    }
    assert!(results.len() >= 8);
    assert!(core_passes(&results));
}

#[test]
fn quilt_device_profile_passes_conformance() {
    // The substrate side satisfies the same contract the controller side
    // demands — one standard, both directions.
    let mut profile = QuiltDeviceProfile::demo("quilt-conformance");
    let results = run_conformance(&mut profile);
    for r in &results {
        assert!(r.passed, "{} {} failed: {}", r.id, r.name, r.detail);
    }
}

/// A transport that clamps out-of-range writes instead of rejecting them
/// (the "helpful" anti-pattern). The suite MUST catch it.
struct LyingClient {
    inner: MockMHS,
}

impl MhsClient for LyingClient {
    fn discover(&mut self) -> MhsResult<Vec<DeviceId>> {
        self.inner.discover()
    }
    fn manifest(&mut self, device: &DeviceId) -> MhsResult<DeviceManifest> {
        self.inner.manifest(device)
    }
    fn read(&mut self, device: &DeviceId, channel: &str) -> MhsResult<Sample> {
        self.inner.read(device, channel)
    }
    fn write(&mut self, device: &DeviceId, channel: &str, value: MhsValue) -> MhsResult<Sample> {
        // silently clamp instead of rejecting
        let clamped = match value.as_f64() {
            Some(v) => {
                let m = self.inner.manifest(device)?;
                let lim = m.safety.channel_limits.get(channel).cloned().unwrap_or((-1e9, 1e9));
                MhsValue::Float(v.clamp(lim.0, lim.1))
            }
            None => value,
        };
        self.inner.write(device, channel, clamped)
    }
    fn run_program(&mut self, steps: Vec<Command>) -> MhsResult<ProgramReceipt> {
        self.inner.run_program(steps)
    }
    fn abort(&mut self, device: &DeviceId, reason: &str) -> MhsResult<AbortReceipt> {
        self.inner.abort(device, reason)
    }
}

#[test]
fn conformance_catches_a_lying_transport() {
    let results = run_conformance(&mut LyingClient { inner: MockMHS::new() });
    assert!(!core_passes(&results), "a clamping transport must fail the suite");
    let c5 = results.iter().find(|r| r.id == "C5").unwrap();
    assert!(!c5.passed, "C5 is the check that must fire");
}
