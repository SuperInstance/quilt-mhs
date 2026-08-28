//! MockMHS — a complete in-memory MHS implementation so the entire quilt
//! bridge runs TODAY: no hardware, no real SDK, no network. Two devices:
//!
//! - `mock-arm-01` — a robot arm joint (the announcement's canonical
//!   example: speed and angle limits are exactly what a vendor declares).
//! - `mock-thermal-01` — a first-order thermal bath (read temperature /
//!   write setpoint: literally the announcement's read/write examples).
//!
//! MockMHS *enforces* its declared envelope (violations reject and leave
//! state untouched), keeps an append-only journal (TICK_monotonicity,
//! VIEW_purity observable from outside), latches after abort, and steps
//! its own control loop via [`MockMHS::tick`] — the machine side of
//! quilt's TICK.

use crate::mhs::client::MhsClient;
use crate::mhs::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One journaled device event; append-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JournalEntry {
    Write { device: DeviceId, channel: String, value: MhsValue, t: f64 },
    Read { device: DeviceId, channel: String, t: f64 },
    Abort { device: DeviceId, reason: String, t: f64 },
    ClearAbort { device: DeviceId, t: f64 },
    Tick { dt: f64, t: f64 },
}

#[derive(Debug, Clone)]
struct MockDevice {
    manifest: DeviceManifest,
    /// channel -> current value
    state: BTreeMap<String, MhsValue>,
    /// channel -> last write time (rate limiting)
    last_write: BTreeMap<String, f64>,
    /// first-order dynamics: channel -> (tau, toward-channel)
    lag: Vec<(String, String, f64)>,
    aborted: Option<String>,
}

impl MockDevice {
    fn enforce(&self, channel: &str, value: &MhsValue, now: f64, has_grant: bool) -> MhsResult<()> {
        if let Some(reason) = &self.aborted {
            return Err(MhsError::Aborted(reason.clone()));
        }
        let ch = self
            .manifest
            .writable
            .iter()
            .find(|c| c.name == channel)
            .ok_or_else(|| MhsError::Unknown(format!("{}:{}", self.manifest.device_id, channel)))?;
        if ch.destructive && self.manifest.safety.destructive_requires_grant && !has_grant {
            // The mock resolves grants at transport level (A-7): the
            // controller's interlock hold is what unlocks the channel.
            return Err(MhsError::GrantRequired(format!("{}:{} is destructive", self.manifest.device_id, channel)));
        }
        if let (Some(v), Some(range)) = (value.as_f64(), ch.range) {
            if !(range.0..=range.1).contains(&v) {
                return Err(MhsError::SafetyViolation(
                    format!("{}:{}", self.manifest.device_id, channel),
                    format!("value {v} outside declared range {range:?}"),
                ));
            }
        }
        if let (Some(v), Some(limit)) = (value.as_f64(), self.manifest.safety.channel_limits.get(channel)) {
            if !(limit.0..=limit.1).contains(&v) {
                return Err(MhsError::SafetyViolation(
                    format!("{}:{}", self.manifest.device_id, channel),
                    format!("value {v} outside enforced limit {limit:?}"),
                ));
            }
        }
        if let (Some(prev), Some(&hz)) = (self.last_write.get(channel), self.manifest.safety.max_write_rate_hz.get(channel)) {
            if hz > 0.0 && now > *prev && (now - *prev) < 1.0 / hz {
                return Err(MhsError::SafetyViolation(
                    format!("{}:{}", self.manifest.device_id, channel),
                    format!("write rate exceeds {hz} Hz"),
                ));
            }
        }
        Ok(())
    }

    fn sample(&self, channel: &str, t: f64) -> MhsResult<Sample> {
        let known = self.manifest.readable.iter().any(|c| c.name == channel)
            || self.manifest.writable.iter().any(|c| c.name == channel);
        if !known {
            return Err(MhsError::Unknown(format!("{}:{}", self.manifest.device_id, channel)));
        }
        Ok(Sample {
            device: self.manifest.device_id.clone(),
            channel: channel.to_string(),
            value: self.state.get(channel).cloned().unwrap_or(MhsValue::Null),
            t,
        })
    }
}

/// In-memory MHS transport with two mock devices and full safety
/// enforcement. `clock` advances only via [`MockMHS::tick`].
#[derive(Debug, Clone)]
pub struct MockMHS {
    devices: BTreeMap<DeviceId, MockDevice>,
    /// Held interlock grants: (device, channel). Destructive writes are
    /// refused without one (A-7).
    grants: BTreeSet<(DeviceId, String)>,
    pub clock: f64,
    pub journal: Vec<JournalEntry>,
}

impl MockMHS {
    pub fn new() -> Self {
        let mut m = MockMHS { devices: BTreeMap::new(), grants: BTreeSet::new(), clock: 0.0, journal: Vec::new() };
        m.add_device(mock_arm(), mock_arm_state(), vec![("joint1.angle".into(), "joint1.target".into(), 0.6)]);
        m.add_device(mock_thermal(), mock_thermal_state(), vec![("bath.temperature".into(), "bath.setpoint".into(), 20.0)]);
        m
    }

    fn add_device(
        &mut self,
        manifest: DeviceManifest,
        state: BTreeMap<String, MhsValue>,
        lag: Vec<(String, String, f64)>,
    ) {
        self.devices.insert(
            manifest.device_id.clone(),
            MockDevice { manifest, state, last_write: BTreeMap::new(), lag, aborted: None },
        );
    }

    /// Advance the machine control loop by `dt` seconds. The device clock
    /// only advances (TICK_monotonicity). First-order channels relax toward
    /// their target channels.
    pub fn tick(&mut self, dt: f64) -> MhsResult<f64> {
        if dt <= 0.0 {
            return Err(MhsError::NotMonotonic);
        }
        self.clock += dt;
        for dev in self.devices.values_mut() {
            // first-order relaxation: each lag pair relaxes toward its source
            for (target_ch, src, tau) in dev.lag.clone() {
                let cur = dev.state.get(&target_ch).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let goal = dev.state.get(&src).and_then(|v| v.as_f64()).unwrap_or(cur);
                let relaxed = cur + (goal - cur) * (1.0 - (-dt / tau).exp());
                dev.state.insert(target_ch, MhsValue::Float(relaxed));
            }
        }
        self.journal.push(JournalEntry::Tick { dt, t: self.clock });
        Ok(self.clock)
    }

    /// Operator-side action (NOT on the MhsClient trait on purpose: an
    /// agent does not unlatch its own abort).
    pub fn clear_abort(&mut self, device: &DeviceId) -> MhsResult<()> {
        let dev = self.devices.get_mut(device).ok_or_else(|| MhsError::Unknown(device.clone()))?;
        dev.aborted = None;
        self.journal.push(JournalEntry::ClearAbort { device: device.clone(), t: self.clock });
        Ok(())
    }

    /// Number of writes journaled (VIEW_purity probes read this before and
    /// after reads).
    pub fn write_count(&self) -> usize {
        self.journal.iter().filter(|e| matches!(e, JournalEntry::Write { .. })).count()
    }

    pub fn read_count(&self) -> usize {
        self.journal.iter().filter(|e| matches!(e, JournalEntry::Read { .. })).count()
    }
}

impl Default for MockMHS {
    fn default() -> Self {
        Self::new()
    }
}

impl MhsClient for MockMHS {
    fn discover(&mut self) -> MhsResult<Vec<DeviceId>> {
        Ok(self.devices.keys().cloned().collect())
    }

    fn manifest(&mut self, device: &DeviceId) -> MhsResult<DeviceManifest> {
        Ok(self.devices.get(device).ok_or_else(|| MhsError::Unknown(device.clone()))?.manifest.clone())
    }

    fn read(&mut self, device: &DeviceId, channel: &str) -> MhsResult<Sample> {
        let t = self.clock;
        let dev = self.devices.get(device).ok_or_else(|| MhsError::Unknown(device.clone()))?;
        let s = dev.sample(channel, t)?;
        self.journal.push(JournalEntry::Read { device: device.clone(), channel: channel.to_string(), t });
        Ok(s)
    }

    fn write(&mut self, device: &DeviceId, channel: &str, value: MhsValue) -> MhsResult<Sample> {
        let t = self.clock;
        let has_grant = self.grants.contains(&(device.clone(), channel.to_string()));
        // enforce + apply atomically
        {
            let dev = self.devices.get(device).ok_or_else(|| MhsError::Unknown(device.clone()))?;
            dev.enforce(channel, &value, t, has_grant)?;
        }
        let dev = self.devices.get_mut(device).unwrap();
        dev.state.insert(channel.to_string(), value.clone());
        dev.last_write.insert(channel.to_string(), t);
        self.journal.push(JournalEntry::Write { device: device.clone(), channel: channel.to_string(), value, t });
        dev.sample(channel, t)
    }

    fn run_program(&mut self, steps: Vec<Command>) -> MhsResult<ProgramReceipt> {
        let mut done = 0;
        for step in steps.clone() {
            match self.write(&step.device, &step.channel, step.value) {
                Ok(_) => done += 1,
                Err(e) => {
                    // All-or-abort: latch every device this program touched.
                    let touched: Vec<DeviceId> = steps.iter().map(|s| s.device.clone()).collect();
                    for d in touched {
                        let _ = self.abort(&d, "program step rejected");
                    }
                    return Ok(ProgramReceipt { accepted_steps: done, completed: false, abort_reason: Some(e.to_string()) });
                }
            }
        }
        Ok(ProgramReceipt { accepted_steps: done, completed: true, abort_reason: None })
    }

    fn abort(&mut self, device: &DeviceId, reason: &str) -> MhsResult<AbortReceipt> {
        let dev = self.devices.get_mut(device).ok_or_else(|| MhsError::Unknown(device.clone()))?;
        dev.aborted = Some(reason.to_string());
        // Safety teardown: relax every writable numeric channel toward the
        // safe end of its range (e.g. arm to angle 0, duty to 0).
        for ch in &dev.manifest.writable {
            if let (Some(_), Some((lo, _))) = (dev.state.get(&ch.name).and_then(|v| v.as_f64()), ch.range) {
                // Safety teardown: park every writable channel at the safe
                // end of its range (arm to 0-anchored limit, duty to floor).
                dev.state.insert(ch.name.clone(), MhsValue::Float(lo));
            }
        }
        let receipt = AbortReceipt { device: device.clone(), reason: reason.to_string(), latched: true };
        self.journal.push(JournalEntry::Abort { device: device.clone(), reason: reason.to_string(), t: self.clock });
        Ok(receipt)
    }

    fn hold_grant(&mut self, device: &DeviceId, channel: &str) -> MhsResult<()> {
        if !self.devices.contains_key(device) {
            return Err(MhsError::Unknown(device.clone()));
        }
        self.grants.insert((device.clone(), channel.to_string()));
        Ok(())
    }

    fn release_grant(&mut self, device: &DeviceId, channel: &str) -> MhsResult<()> {
        self.grants.remove(&(device.clone(), channel.to_string()));
        Ok(())
    }

    fn poll(&mut self, dt: f64) -> MhsResult<f64> {
        self.tick(dt)
    }
}

/// Initial state for the mock arm (see `mock_arm`).
pub fn mock_arm_state() -> BTreeMap<String, MhsValue> {
    [
        ("joint1.angle", 0.0),
        ("joint1.target", 0.0),
        ("load.cell", 0.0),
        ("gripper.cmd", 0.0),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), MhsValue::Float(v)))
    .collect()
}

/// The announcement's canonical robot-arm example: a vendor declares how an
/// AI may move a heavy arm safely, "limiting the speeds and angles it is
/// allowed to use" (TNW, 2026-08-27). Weight is the announcement's own
/// example of tag knowledge that lives in paper manuals today.
pub fn mock_arm() -> DeviceManifest {
    let mut channel_limits = BTreeMap::new();
    channel_limits.insert("joint1.target".to_string(), (-90.0, 90.0));
    channel_limits.insert("gripper.cmd".to_string(), (0.0, 1.0));
    let mut max_write_rate_hz = BTreeMap::new();
    max_write_rate_hz.insert("joint1.target".to_string(), 10.0);
    DeviceManifest {
        device_id: "mock-arm-01".into(),
        model: "MockArm S1".into(),
        firmware: "0.1.0-mock".into(),
        tags: vec![
            "Arm mass 18.5 kg; mount to a 400 kg granite slab before commanding joint1.".into(),
            "joint1 slew is 30 deg/s mechanical; the envelope enforces 10 Hz command rate to leave headroom.".into(),
            "gripper.cmd 1.0 is a hard close: samples may be crushed. Treat as destructive.".into(),
        ],
        readable: vec![
            Channel { name: "joint1.angle".into(), unit: Some("deg".into()), writable: false, range: Some((-90.0, 90.0)), destructive: false },
            Channel { name: "load.cell".into(), unit: Some("kg".into()), writable: false, range: Some((0.0, 5.0)), destructive: false },
        ],
        writable: vec![
            Channel { name: "joint1.target".into(), unit: Some("deg".into()), writable: true, range: Some((-90.0, 90.0)), destructive: false },
            Channel { name: "gripper.cmd".into(), unit: Some("frac".into()), writable: true, range: Some((0.0, 1.0)), destructive: true },
        ],
        safety: SafetyEnvelope { channel_limits, max_write_rate_hz, destructive_requires_grant: true, abort_supported: true },
        transport: "mock".into(),
    }
}

/// Initial state for the mock thermal bath (see `mock_thermal`).
pub fn mock_thermal_state() -> BTreeMap<String, MhsValue> {
    [
        ("bath.temperature", 22.0),
        ("bath.setpoint", 22.0),
        ("pump.duty", 0.0),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), MhsValue::Float(v)))
    .collect()
}

/// The announcement's read/write examples verbatim: "get temperature" /
/// "set temperature", on a first-order thermal bath.
pub fn mock_thermal() -> DeviceManifest {
    let mut channel_limits = BTreeMap::new();
    channel_limits.insert("bath.setpoint".to_string(), (0.0, 100.0));
    channel_limits.insert("pump.duty".to_string(), (0.0, 100.0));
    DeviceManifest {
        device_id: "mock-thermal-01".into(),
        model: "MockBath T1".into(),
        firmware: "0.1.0-mock".into(),
        tags: vec![
            "Water bath, 2 L. Boils dry above 100 degC setpoint — never command above 95 for unattended runs.".into(),
        ],
        readable: vec![
            Channel { name: "bath.temperature".into(), unit: Some("degC".into()), writable: false, range: Some((0.0, 100.0)), destructive: false },
        ],
        writable: vec![
            Channel { name: "bath.setpoint".into(), unit: Some("degC".into()), writable: true, range: Some((0.0, 100.0)), destructive: false },
            Channel { name: "pump.duty".into(), unit: Some("%".into()), writable: true, range: Some((0.0, 100.0)), destructive: false },
        ],
        safety: SafetyEnvelope { channel_limits, max_write_rate_hz: BTreeMap::new(), destructive_requires_grant: true, abort_supported: true },
        transport: "mock".into(),
    }
}
