//! Inter-quilt federation: two quilt runtimes operating each other
//! through MHS-shaped messaging. Runtime A's controller binds cells to
//! runtime B's device profile and vice versa — the same
//! [`QuiltDeviceProfile`] surface an external agent (Claude, another
//! fleet) would see, exercised by a quilt on the other end.
//!
//! [`FederationLink`] is a transport handle: it owns a share of the
//! remote runtime. In-process today; the day the real MHS transport
//! lands, `FederationLink` is the single file that changes (see
//! PORTING.md §Swapping the transport).

use crate::controller::QuiltMhsAdapter;
use crate::device::QuiltDeviceProfile;
use crate::mhs::client::MhsClient;
use crate::mhs::types::*;
use std::sync::{Arc, Mutex};

/// A transport handle to a remote quilt-as-MHS-device. All MHS messages
/// (discover/manifest/read/write/program/abort/grant/poll) cross this
/// link. Swap the body of these methods for a real MHS transport and the
/// federation test suite runs unchanged against real remote runtimes.
pub struct FederationLink {
    remote: Arc<Mutex<QuiltDeviceProfile>>,
}

impl FederationLink {
    pub fn to(remote: Arc<Mutex<QuiltDeviceProfile>>) -> Self {
        FederationLink { remote }
    }
}

impl MhsClient for FederationLink {
    fn discover(&mut self) -> MhsResult<Vec<DeviceId>> {
        self.remote.lock().unwrap().discover()
    }
    fn manifest(&mut self, device: &DeviceId) -> MhsResult<DeviceManifest> {
        self.remote.lock().unwrap().manifest(device)
    }
    fn read(&mut self, device: &DeviceId, channel: &str) -> MhsResult<Sample> {
        self.remote.lock().unwrap().read(device, channel)
    }
    fn write(&mut self, device: &DeviceId, channel: &str, value: MhsValue) -> MhsResult<Sample> {
        self.remote.lock().unwrap().write(device, channel, value)
    }
    fn run_program(&mut self, steps: Vec<Command>) -> MhsResult<ProgramReceipt> {
        // A program must apply atomically against the remote; hold the
        // lock across the whole sequence.
        let mut remote = self.remote.lock().unwrap();
        let mut done = 0;
        for s in steps {
            match remote.write(&s.device, &s.channel, s.value) {
                Ok(_) => done += 1,
                Err(e) => {
                    let _ = remote.abort(&s.device, "federated program step rejected");
                    return Ok(ProgramReceipt { accepted_steps: done, completed: false, abort_reason: Some(e.to_string()) });
                }
            }
        }
        Ok(ProgramReceipt { accepted_steps: done, completed: true, abort_reason: None })
    }
    fn abort(&mut self, device: &DeviceId, reason: &str) -> MhsResult<AbortReceipt> {
        self.remote.lock().unwrap().abort(device, reason)
    }
    fn hold_grant(&mut self, device: &DeviceId, channel: &str) -> MhsResult<()> {
        self.remote.lock().unwrap().hold_grant(device, channel)
    }
    fn release_grant(&mut self, device: &DeviceId, channel: &str) -> MhsResult<()> {
        self.remote.lock().unwrap().release_grant(device, channel)
    }
    fn poll(&mut self, dt: f64) -> MhsResult<f64> {
        self.remote.lock().unwrap().poll(dt)
    }
}

/// Two quilt runtimes, each exposing a sheet as an MHS device and each
/// holding a controller pointed at the other — the federation pair used
/// by `tests/federation.rs` to prove inter-quilt operation end to end.
pub struct FederationPair {
    pub a: QuiltMhsAdapter<FederationLink>,
    pub b: QuiltMhsAdapter<FederationLink>,
    sheet_a: Arc<Mutex<QuiltDeviceProfile>>,
    sheet_b: Arc<Mutex<QuiltDeviceProfile>>,
}

impl FederationPair {
    /// Wire two demo sheets together. `a` drives `b`'s sheet; `b` drives
    /// `a`'s sheet.
    pub fn demo() -> Self {
        let sheet_a = Arc::new(Mutex::new(QuiltDeviceProfile::demo("quilt-A")));
        let sheet_b = Arc::new(Mutex::new(QuiltDeviceProfile::demo("quilt-B")));
        FederationPair {
            a: QuiltMhsAdapter::new(FederationLink::to(sheet_b.clone())),
            b: QuiltMhsAdapter::new(FederationLink::to(sheet_a.clone())),
            sheet_a,
            sheet_b,
        }
    }

    /// Direct (operator-side) peek at a sheet — for assertions only.
    pub fn sheet(&self, which: PairSide) -> std::sync::MutexGuard<'_, QuiltDeviceProfile> {
        match which {
            PairSide::A => self.sheet_a.lock().unwrap(),
            PairSide::B => self.sheet_b.lock().unwrap(),
        }
    }
}

pub enum PairSide {
    A,
    B,
}
