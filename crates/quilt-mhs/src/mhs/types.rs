//! Types for the Model Hardware Standard (MHS) surface, as announced
//! 2026-08-27. Every field here is either (a) named in press coverage
//! (sourced in `MHS-SPEC-WATCH.md`) or (b) a **clearly-marked assumption**
//! (`A-n` in `MHS-SPEC-WATCH.md`). When the real spec lands, these types are
//! the ONLY thing that changes; the ports (`client::MhsClient`,
//! `device::QuiltDeviceProfile`) are stable.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A scalar value crossing the MHS boundary. Deliberately tiny: the press
/// surface is "read" (get temperature) / "write" (set temperature) style
/// primitives, so scalars plus null cover the announced shape without
/// inventing container semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum MhsValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl MhsValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            MhsValue::Int(i) => Some(*i as f64),
            MhsValue::Float(f) => Some(*f),
            _ => None,
        }
    }
}

/// A stable device identifier, e.g. `mock-arm-01`.
pub type DeviceId = String;

/// A channel on a device — the addressable read/write surface.
/// Press names read ("get temperature") and write ("set temperature") as the
/// core primitives; a device offers named channels for each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Channel {
    /// Channel name, e.g. `bath.setpoint`, `joint1.target`.
    pub name: String,
    /// Physical unit if numeric, e.g. "degC", "deg", "kg".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Whether the agent may write this channel (readable channels are
    /// listed in the manifest's `readable` set).
    pub writable: bool,
    /// Declared numeric range, if any. The MHS driver "enforces" safety
    /// limits (press, 2026-08-27); we model enforcement per channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<(f64, f64)>,
    /// True if writing this channel is destructive / irreversible.
    /// Destructive channels require an explicit grant (our forking rule:
    /// no destructive ops without explicit grant).
    #[serde(default)]
    pub destructive: bool,
}

/// Safety envelope a device declares and enforces. The announcement says the
/// MHS driver produces a reference file describing "what it can measure,
/// what can be adjusted, and what safety limits will be enforced" — this is
/// our concretization of that sentence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SafetyEnvelope {
    /// Hard per-channel numeric limits; writes outside are rejected by the
    /// device, not by agent politeness.
    pub channel_limits: BTreeMap<String, (f64, f64)>,
    /// Maximum accepted write rate per channel, Hz. Assumption A-2.
    pub max_write_rate_hz: BTreeMap<String, f64>,
    /// Destructive channels may only be written while an interlock grant
    /// is held (explicit, revocable, forgettable).
    pub destructive_requires_grant: bool,
    /// Device supports an abort/estop path. Assumption A-6 (press implies
    /// enforcement and error recovery; exact surface unknown).
    pub abort_supported: bool,
}

/// The "reference file" concept from the announcement: what the device is,
/// what it can measure, what can be adjusted, which limits are enforced,
/// plus the natural-language `tags` the announcement describes (e.g. the
/// weight of a robot arm, stored as text rather than code).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeviceManifest {
    pub device_id: DeviceId,
    pub model: String,
    pub firmware: String,
    /// Natural-language knowledge the announcement says users (or an
    /// interviewing agent) write into the driver. Preserved verbatim.
    pub tags: Vec<String>,
    pub readable: Vec<Channel>,
    pub writable: Vec<Channel>,
    pub safety: SafetyEnvelope,
    /// Transport identifier for the current session, e.g. "mock", "mcp",
    /// "cli", "code-files". The announcement names MCP, CLI, and code files
    /// as the three control mechanisms.
    pub transport: String,
}

/// One telemetry sample: a channel value with the device's clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Sample {
    pub device: DeviceId,
    pub channel: String,
    pub value: MhsValue,
    /// Device-local monotonic time (seconds). TICK_monotonicity holds here:
    /// the device clock only advances.
    pub t: f64,
}

/// A single write step, used both for direct writes and for chained
/// programs ("code files" in the announcement: deterministic sequences the
/// device runs without the agent reasoning at every step).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Command {
    pub device: DeviceId,
    pub channel: String,
    pub value: MhsValue,
}

/// Receipt for a chained program ("code file"). All steps run or the
/// device aborts — partial application is reported, never hidden.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProgramReceipt {
    pub accepted_steps: usize,
    pub completed: bool,
    pub abort_reason: Option<String>,
}

/// Receipt for an abort/teardown. Assumption A-6: exact MHS abort shape
/// unknown; we record the reason and the device's post-abort state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbortReceipt {
    pub device: DeviceId,
    pub reason: String,
    /// True when writes are refused until an operator clears the abort.
    pub latched: bool,
}

/// Errors crossing the MHS boundary. Safety violations are first-class:
/// a rejected write never changes device state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MhsError {
    /// Device or channel does not exist.
    Unknown(String),
    /// Write rejected by the device's safety envelope. Device state is
    /// unchanged. The second element is the human-readable limit that fired.
    SafetyViolation(String, String),
    /// Channel is destructive and no interlock grant is held.
    GrantRequired(String),
    /// Device is latched after an abort; refuses writes until cleared by
    /// an operator.
    Aborted(String),
    /// Malformed value for the channel.
    BadValue(String),
    /// Clock may only advance.
    NotMonotonic,
    /// Transport-level failure (reserved for real transports).
    Transport(String),
}

impl std::fmt::Display for MhsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MhsError::Unknown(s) => write!(f, "unknown device or channel: {s}"),
            MhsError::SafetyViolation(dev, why) => write!(f, "safety violation on {dev}: {why}"),
            MhsError::GrantRequired(s) => write!(f, "interlock grant required: {s}"),
            MhsError::Aborted(s) => write!(f, "device latched after abort: {s}"),
            MhsError::BadValue(s) => write!(f, "bad value: {s}"),
            MhsError::NotMonotonic => write!(f, "clock only advances"),
            MhsError::Transport(s) => write!(f, "transport failure: {s}"),
        }
    }
}

impl std::error::Error for MhsError {}

pub type MhsResult<T> = Result<T, MhsError>;
