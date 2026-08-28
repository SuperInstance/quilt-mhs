//! The controller-side port: everything quilt needs FROM a Model Hardware
//! Standard implementation, behind one trait. This is the seam that
//! isolates the announced-but-unreleased MHS spec from the quilt core.
//!
//! Design rule (ports-and-adapters): the adapter code in `controller`
//! knows ONLY this trait and the types in `mhs::types`. When Anthropic
//! ships the real MHS SDK, you write one new `MhsClient` impl and run the
//! same conformance suite (`mhs::conformance`) against it. Nothing else
//! changes.

use crate::mhs::types::*;

/// The MHS client port, modeled on the announced surface (2026-08-27):
///
/// - **Discovery**: "each device discoverable in a standard format, so that
///   devices and agents can find each other and communicate across
///   networks" → [`MhsClient::discover`] + [`MhsClient::manifest`].
/// - **Primitives**: read ("get temperature") / write ("set temperature") →
///   [`MhsClient::read`] / [`MhsClient::write`].
/// - **Code files**: chained driver commands the device runs without the
///   agent reasoning at every step → [`MhsClient::run_program`].
/// - **Safety**: limits the driver "will enforce", and hardware-error
///   recovery → [`MhsClient::abort`] (surface assumed, A-6).
///
/// Intentionally synchronous — quilt's engine is sync at the core and
/// async happens at the boundary (quilt-rust architecture note). Real
/// transports wrap this trait and block, spawn, or await as they please.
pub trait MhsClient {
    /// List devices visible on this transport.
    fn discover(&mut self) -> MhsResult<Vec<DeviceId>>;

    /// Fetch a device's reference-file manifest (characteristics,
    /// channels, tags, enforced safety limits).
    fn manifest(&mut self, device: &DeviceId) -> MhsResult<DeviceManifest>;

    /// Read one channel; returns a telemetry sample. Reads must be pure:
    /// they never mutate device state.
    fn read(&mut self, device: &DeviceId, channel: &str) -> MhsResult<Sample>;

    /// Write one channel. The DEVICE enforces its safety envelope; a
    /// rejected write leaves state unchanged and returns
    /// [`MhsError::SafetyViolation`] or [`MhsError::GrantRequired`].
    fn write(&mut self, device: &DeviceId, channel: &str, value: MhsValue) -> MhsResult<Sample>;

    /// Run a chained program (a "code file"): a deterministic sequence of
    /// writes the device executes itself. All-or-abort.
    fn run_program(&mut self, steps: Vec<Command>) -> MhsResult<ProgramReceipt>;

    /// Abort / emergency teardown for one device. Post-abort the device
    /// refuses writes until an operator clears it (A-6).
    fn abort(&mut self, device: &DeviceId, reason: &str) -> MhsResult<AbortReceipt>;

    /// Hold an interlock grant for a destructive channel (A-7). Default:
    /// unsupported — simple transports may have no grant surface and
    /// simply reject destructive writes outright.
    fn hold_grant(&mut self, _device: &DeviceId, _channel: &str) -> MhsResult<()> {
        Err(MhsError::Transport("grants unsupported on this transport".into()))
    }

    /// Release a previously held grant (A-7).
    fn release_grant(&mut self, _device: &DeviceId, _channel: &str) -> MhsResult<()> {
        Err(MhsError::Transport("grants unsupported on this transport".into()))
    }

    /// Advance transport/device time and drain pending work — the
    /// machine's own control-loop step (A-8). Real devices run their loops
    /// asynchronously; this is the sync seam the adapter's TICK calls.
    /// Default: unsupported (device time advances elsewhere).
    fn poll(&mut self, _dt: f64) -> MhsResult<f64> {
        Err(MhsError::Transport("poll unsupported on this transport".into()))
    }
}
