//! The quilt-as-MHS-device adapter (substrate side): expose a quilt
//! runtime as an MHS-addressable machine. Cells/sheets become the
//! addressable resources; other agents — Claude, other fleets, other
//! quilt runtimes — operate the substrate through the same MHS-shaped
//! surface defined in `mhs::types`.
//!
//! Two deployment modes fall out for free:
//!
//! - **Intra-quilt**: cells in one sheet drive cells in another sheet
//!   through MHS-shaped messages instead of direct LINKs — useful when the
//!   sheets live in different processes and you want the safety envelope
//!   enforced at the seam.
//! - **Inter-quilt**: two quilt runtimes federation-test each other; see
//!   `federation`.
//!
//! The sheet here is a deliberately small reference runtime. A real
//! deployment swaps `SheetRuntime` for quilt-rust's `QuiltEngine` (same
//! opcode vocabulary, `Arc<QuiltEngine>` at the boundary) — the profile
//! only needs `define/set/get/tick`, which is exactly what the engine's
//! public API already offers.

pub mod federation;
use crate::mhs::client::MhsClient;
use crate::mhs::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Cell kinds exposed by the device (the quilt-rust vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetCellKind {
    /// Static value: readable.
    Value,
    /// Reactive expression: readable.
    Formula,
    /// Polled input: readable (telemetry source).
    Sensor,
    /// Physical port: readable AND writable within contract.
    Io,
}

#[derive(Debug, Clone)]
pub struct SheetCell {
    pub kind: SheetCellKind,
    pub value: MhsValue,
    /// Contract range for numeric cells — becomes the enforced channel
    /// limit in the safety envelope.
    pub range: Option<(f64, f64)>,
    /// Destructive writes need an interlock grant (the forking rule).
    pub destructive: bool,
}

/// A minimal reference quilt sheet. The device side of the bridge only
/// needs this surface; quilt-rust's `QuiltEngine` is a drop-in.
#[derive(Debug, Clone, Default)]
pub struct SheetRuntime {
    pub cells: BTreeMap<String, SheetCell>,
    pub clock: f64,
}

impl SheetRuntime {
    pub fn new() -> Self {
        SheetRuntime::default()
    }

    pub fn define(&mut self, name: &str, kind: SheetCellKind, value: MhsValue, range: Option<(f64, f64)>, destructive: bool) {
        self.cells.insert(name.to_string(), SheetCell { kind, value, range, destructive });
    }

    pub fn get(&self, name: &str) -> Option<&MhsValue> {
        self.cells.get(name).map(|c| &c.value)
    }

    pub fn set(&mut self, name: &str, value: MhsValue) -> MhsResult<()> {
        let c = self.cells.get_mut(name).ok_or_else(|| MhsError::Unknown(name.to_string()))?;
        if let (Some(v), Some((lo, hi))) = (value.as_f64(), c.range) {
            if !(lo..=hi).contains(&v) {
                return Err(MhsError::SafetyViolation(name.to_string(), format!("{v} outside contract {lo}..{hi}")));
            }
        }
        c.value = value;
        Ok(())
    }

    /// The sheet's own TICK: monotonic, drains nothing by itself (a real
    /// engine drains queued effects here).
    pub fn tick(&mut self, dt: f64) -> MhsResult<f64> {
        if dt <= 0.0 {
            return Err(MhsError::NotMonotonic);
        }
        self.clock += dt;
        Ok(self.clock)
    }
}

/// A quilt runtime exposed AS an MHS device. Implements [`MhsClient`]
/// over itself (in-process transport): the same trait the controller side
/// consumes, so an adapter can drive a device profile with zero extra
/// plumbing — that IS the intra-quilt loop.
pub struct QuiltDeviceProfile {
    pub device_id: DeviceId,
    pub model: String,
    pub sheet: SheetRuntime,
    grants: BTreeSet<String>,
    aborted: Option<String>,
    /// Human/operator notes carried into the manifest `tags` (the MHS
    /// natural-language knowledge channel).
    pub tags: Vec<String>,
}

impl QuiltDeviceProfile {
    /// A ready-made demo substrate: a small boat-controller sheet —
    /// bilge sensor, pump IO, engine throttle, log cell.
    pub fn demo(device_id: &str) -> Self {
        let mut sheet = SheetRuntime::new();
        sheet.define("bilge.depth", SheetCellKind::Sensor, MhsValue::Float(0.12), Some((0.0, 2.0)), false);
        sheet.define("pump.duty", SheetCellKind::Io, MhsValue::Float(0.0), Some((0.0, 100.0)), false);
        sheet.define("engine.throttle", SheetCellKind::Io, MhsValue::Float(0.0), Some((0.0, 100.0)), false);
        sheet.define("engine.scram", SheetCellKind::Io, MhsValue::Bool(false), None, true);
        sheet.define("log.line", SheetCellKind::Formula, MhsValue::Str("boot".into()), None, false);
        QuiltDeviceProfile {
            device_id: device_id.to_string(),
            model: "QuiltSheet 0.1".into(),
            sheet,
            grants: BTreeSet::new(),
            aborted: None,
            tags: vec![
                "This substrate is a live quilt sheet; every channel is a cell. VIEW cells with SENSOR kind, EFFECT cells with IO kind.".into(),
                "engine.scram is destructive: it kills the engine and needs an interlock grant to command.".into(),
            ],
        }
    }

    pub fn new(device_id: &str, model: &str, sheet: SheetRuntime, tags: Vec<String>) -> Self {
        QuiltDeviceProfile { device_id: device_id.to_string(), model: model.to_string(), sheet, grants: BTreeSet::new(), aborted: None, tags }
    }

    /// Build the MHS manifest ("reference file") for this sheet: sensor
    /// and formula cells are readable; IO cells are writable within their
    /// contracts; contracts + destructive flags become the enforced
    /// safety envelope.
    pub fn build_manifest(&self) -> DeviceManifest {
        let mut readable = Vec::new();
        let mut writable = Vec::new();
        let mut limits = BTreeMap::new();
        for (name, c) in &self.sheet.cells {
            let channel = Channel {
                name: name.clone(),
                unit: None,
                writable: c.kind == SheetCellKind::Io,
                range: c.range,
                destructive: c.destructive,
            };
            if c.range.is_some() {
                limits.insert(name.clone(), c.range.unwrap());
            }
            match c.kind {
                SheetCellKind::Io => {
                    readable.push(channel.clone());
                    writable.push(channel);
                }
                _ => readable.push(channel),
            }
        }
        DeviceManifest {
            device_id: self.device_id.clone(),
            model: self.model.clone(),
            firmware: "quilt-sheet-0.1".into(),
            tags: self.tags.clone(),
            readable,
            writable,
            safety: SafetyEnvelope {
                channel_limits: limits,
                max_write_rate_hz: BTreeMap::new(),
                destructive_requires_grant: true,
                abort_supported: true,
            },
            transport: "quilt-sheet-inprocess".into(),
        }
    }

    fn ensure_not_aborted(&self) -> MhsResult<()> {
        match &self.aborted {
            Some(r) => Err(MhsError::Aborted(r.clone())),
            None => Ok(()),
        }
    }

    /// Operator-side unlatch (agents do not clear their own aborts).
    pub fn clear_abort(&mut self) {
        self.aborted = None;
    }
}

impl MhsClient for QuiltDeviceProfile {
    fn discover(&mut self) -> MhsResult<Vec<DeviceId>> {
        Ok(vec![self.device_id.clone()])
    }

    fn manifest(&mut self, device: &DeviceId) -> MhsResult<DeviceManifest> {
        if device != &self.device_id {
            return Err(MhsError::Unknown(device.clone()));
        }
        Ok(self.build_manifest())
    }

    fn read(&mut self, device: &DeviceId, channel: &str) -> MhsResult<Sample> {
        if device != &self.device_id {
            return Err(MhsError::Unknown(device.clone()));
        }
        let v = self.sheet.get(channel).cloned().ok_or_else(|| MhsError::Unknown(format!("{device}:{channel}")))?;
        Ok(Sample { device: device.clone(), channel: channel.to_string(), value: v, t: self.sheet.clock })
    }

    fn write(&mut self, device: &DeviceId, channel: &str, value: MhsValue) -> MhsResult<Sample> {
        if device != &self.device_id {
            return Err(MhsError::Unknown(device.clone()));
        }
        self.ensure_not_aborted()?;
        let cell = self.sheet.cells.get(channel).ok_or_else(|| MhsError::Unknown(format!("{device}:{channel}")))?;
        if cell.kind != SheetCellKind::Io {
            return Err(MhsError::BadValue(format!("{channel} is not an IO cell")));
        }
        if cell.destructive && !self.grants.contains(channel) {
            return Err(MhsError::GrantRequired(format!("{device}:{channel} is destructive")));
        }
        self.sheet.set(channel, value.clone())?;
        Ok(Sample { device: device.clone(), channel: channel.to_string(), value, t: self.sheet.clock })
    }

    fn run_program(&mut self, steps: Vec<Command>) -> MhsResult<ProgramReceipt> {
        let mut done = 0;
        for s in steps {
            if s.device != self.device_id {
                return Ok(ProgramReceipt { accepted_steps: done, completed: false, abort_reason: Some(format!("foreign device {}", s.device)) });
            }
            match self.write(&s.device, &s.channel, s.value) {
                Ok(_) => done += 1,
                Err(e) => return Ok(ProgramReceipt { accepted_steps: done, completed: false, abort_reason: Some(e.to_string()) }),
            }
        }
        Ok(ProgramReceipt { accepted_steps: done, completed: true, abort_reason: None })
    }

    fn abort(&mut self, device: &DeviceId, reason: &str) -> MhsResult<AbortReceipt> {
        if device != &self.device_id {
            return Err(MhsError::Unknown(device.clone()));
        }
        self.aborted = Some(reason.to_string());
        // FORGET-shaped teardown: park every IO cell at its contract floor.
        let names: Vec<String> = self.sheet.cells.iter().filter(|(_, c)| c.kind == SheetCellKind::Io).map(|(n, _)| n.clone()).collect();
        for n in names {
            if let Some((lo, _)) = self.sheet.cells.get(&n).and_then(|c| c.range) {
                self.sheet.set(&n, MhsValue::Float(lo)).ok();
            }
        }
        self.grants.clear();
        Ok(AbortReceipt { device: device.clone(), reason: reason.to_string(), latched: true })
    }

    fn hold_grant(&mut self, device: &DeviceId, channel: &str) -> MhsResult<()> {
        if device != &self.device_id {
            return Err(MhsError::Unknown(device.clone()));
        }
        self.grants.insert(channel.to_string());
        Ok(())
    }

    fn release_grant(&mut self, device: &DeviceId, channel: &str) -> MhsResult<()> {
        if device != &self.device_id {
            return Err(MhsError::Unknown(device.clone()));
        }
        self.grants.remove(channel);
        Ok(())
    }

    fn poll(&mut self, dt: f64) -> MhsResult<f64> {
        self.sheet.tick(dt)
    }
}
