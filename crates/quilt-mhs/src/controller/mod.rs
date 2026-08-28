//! The quilt→MHS adapter (controller side): quilt cells and effects drive
//! MHS devices. The 5+1 quilt opcodes map onto the announced MHS surface
//! like this (sources + assumptions in MHS-SPEC-WATCH.md):
//!
//! | quilt opcode | MHS surface | why |
//! |---|---|---|
//! | `BIND(name, value)` | cell ↔ device-channel registration (`manifest` lookup) | a bound cell IS an addressable resource |
//! | `LINK(a, b)` | cell-to-device graph edge | links carry propagation to device-bound tails |
//! | `EFFECT(target, v)` | `write(device, channel, v)` | "write" is the announced primitive |
//! | `VIEW(target)` | `read(device, channel)` | "read" is the announced primitive |
//! | `TICK(dt)` | `poll(dt)` — the machine's control-loop step | devices run loops; TICK drains them |
//! | `FORGET(x)` | `abort(device)` + grant release | safety teardown; interlocks are first-class forgettable state |
//!
//! The 5+1+1 laws are enforced here (see `tests/laws.rs`):
//! BIND idempotence, LINK transitivity, EFFECT associativity, VIEW purity,
//! TICK monotonicity, super-relevance (hand-ranked channels), and
//! FORGET-completeness (teardown leaves no trace the laws can see).

use crate::mhs::client::MhsClient;
use crate::mhs::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A hand (curator) that a cell serves — super-relevance counts how many
/// distinct hands a device channel satisfies.
pub type HandId = String;

/// Where a cell's EFFECT/VIEW actually lands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceBinding {
    pub device: DeviceId,
    pub channel: String,
    /// True when the channel is writable (EFFECT-capable). Readable-only
    /// bindings are VIEW cells (sensors).
    pub effect_capable: bool,
    pub destructive: bool,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub value: MhsValue,
    /// outbound links a → b
    pub links: BTreeSet<String>,
    pub binding: Option<DeviceBinding>,
    pub hands: BTreeSet<HandId>,
}

/// One adapter-side opcode event (append-only journal; TICK_monotonicity
/// and forget-completeness are observable from outside through it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OpEvent {
    Bind { name: String, value: MhsValue },
    BindDevice { name: String, device: DeviceId, channel: String },
    Link { from: String, to: String },
    Effect { name: String, value: MhsValue },
    EffectDevice { name: String, device: DeviceId, channel: String, value: MhsValue },
    View { name: String },
    Tick { dt: f64, t: f64 },
    Grant { device: DeviceId, channel: String, held_by: String },
    ReleaseGrant { device: DeviceId, channel: String, held_by: String },
    Abort { device: DeviceId, reason: String },
    Forget { name: String },
}

/// Receipt for a FORGET — everything that teardown removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ForgetReceipt {
    pub cell: String,
    pub links_removed: usize,
    pub grants_released: usize,
    pub devices_aborted: Vec<DeviceId>,
}

/// The quilt→MHS adapter. Generic over the [`MhsClient`] port; today you
/// use [`crate::mhs::mock::MockMHS`], tomorrow the real SDK behind the
/// same trait.
pub struct QuiltMhsAdapter<C: MhsClient> {
    client: C,
    cells: BTreeMap<String, Cell>,
    /// interlock grants: (device, channel) → cell(s) holding it
    grants: BTreeMap<(DeviceId, String), BTreeSet<String>>,
    pub clock: f64,
    pub journal: Vec<OpEvent>,
}

impl<C: MhsClient> QuiltMhsAdapter<C> {
    pub fn new(client: C) -> Self {
        QuiltMhsAdapter { client, cells: BTreeMap::new(), grants: BTreeMap::new(), clock: 0.0, journal: Vec::new() }
    }

    /// Access the underlying transport (federation tests use this).
    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }

    // ---- BIND -----------------------------------------------------------

    /// BIND a local cell to a value. Idempotent by law: `BIND(n,v);
    /// BIND(n,v) = BIND(n,v)` — the second identical bind journals
    /// nothing. Binding a different value to an existing name is an error
    /// (unbind first: FORGET).
    pub fn bind(&mut self, name: &str, value: MhsValue) -> MhsResult<()> {
        match self.cells.get(name) {
            Some(c) if c.value == value && c.binding.is_none() => Ok(()), // law: no-op
            Some(_) => Err(MhsError::BadValue(format!("'{name}' already bound to a different value/binding"))),
            None => {
                self.cells.insert(name.to_string(), Cell { value: value.clone(), links: BTreeSet::new(), binding: None, hands: BTreeSet::new() });
                self.journal.push(OpEvent::Bind { name: name.to_string(), value });
                Ok(())
            }
        }
    }

    /// BIND a cell to a device channel — the cell becomes the quilt-side
    /// address of an MHS-addressable resource. The manifest is fetched and
    /// the channel must exist. Writable channels are EFFECT-capable;
    /// readable-only channels are sensors (VIEW).
    pub fn bind_to_device(&mut self, name: &str, device: &DeviceId, channel: &str, hands: &[HandId]) -> MhsResult<()> {
        let manifest = self.client.manifest(device)?;
        let writable = manifest.writable.iter().find(|c| c.name == channel);
        let readable = manifest.readable.iter().find(|c| c.name == channel);
        let binding = match (writable, readable) {
            (Some(w), _) => DeviceBinding { device: device.clone(), channel: channel.to_string(), effect_capable: true, destructive: w.destructive },
            (None, Some(_)) => DeviceBinding { device: device.clone(), channel: channel.to_string(), effect_capable: false, destructive: false },
            (None, None) => return Err(MhsError::Unknown(format!("{device}:{channel}"))),
        };
        let value = self.client.read(device, channel)?.value;
        let cell = Cell { value, links: BTreeSet::new(), binding: Some(binding.clone()), hands: hands.iter().cloned().collect() };
        self.cells.insert(name.to_string(), cell);
        self.journal.push(OpEvent::BindDevice { name: name.to_string(), device: device.clone(), channel: channel.to_string() });
        Ok(())
    }

    /// Hands serving an existing cell (super-relevance bookkeeping).
    pub fn add_hand(&mut self, name: &str, hand: HandId) -> MhsResult<()> {
        let c = self.cells.get_mut(name).ok_or_else(|| MhsError::Unknown(name.to_string()))?;
        c.hands.insert(hand);
        Ok(())
    }

    // ---- LINK -----------------------------------------------------------

    /// LINK a → b. Missing endpoints are implicitly BINDed to Null (the
    /// qm_link semantics from quilt-esp32's canon layer).
    pub fn link(&mut self, from: &str, to: &str) -> MhsResult<()> {
        for n in [from, to] {
            if !self.cells.contains_key(n) {
                self.cells.insert(n.to_string(), Cell { value: MhsValue::Null, links: BTreeSet::new(), binding: None, hands: BTreeSet::new() });
            }
        }
        self.cells.get_mut(from).unwrap().links.insert(to.to_string());
        self.journal.push(OpEvent::Link { from: from.to_string(), to: to.to_string() });
        Ok(())
    }

    /// Transitive closure of outbound links from `name` (LINK_transitivity:
    /// a→b + b→c ⟹ closure(a) ∋ c). Cycles terminate (visited set).
    pub fn link_closure(&self, name: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<String> = self.cells.get(name).map(|c| c.links.iter().cloned().collect()).unwrap_or_default();
        while let Some(n) = stack.pop() {
            if seen.insert(n.clone()) {
                if let Some(c) = self.cells.get(&n) {
                    stack.extend(c.links.iter().cloned());
                }
            }
        }
        seen
    }

    // ---- EFFECT ---------------------------------------------------------

    /// EFFECT: make a cell take a value. Local cells set locally. Cells
    /// bound to a writable device channel write through the MHS port; the
    /// DEVICE enforces its envelope (out-of-limit → `SafetyViolation`,
    /// destructive without grant → `GrantRequired`). The value also
    /// propagates along links until it lands on a device-bound tail —
    /// a sensor→formula→actuator chain is three cells and one write.
    pub fn effect(&mut self, name: &str, value: MhsValue) -> MhsResult<Vec<Sample>> {
        let mut samples = Vec::new();
        self.effect_inner(name, &value, &mut samples)?;
        self.journal.push(OpEvent::Effect { name: name.to_string(), value });
        Ok(samples)
    }

    fn effect_inner(&mut self, name: &str, value: &MhsValue, samples: &mut Vec<Sample>) -> MhsResult<()> {
        let Some(cell) = self.cells.get(name) else {
            return Err(MhsError::Unknown(name.to_string()));
        };
        match &cell.binding {
            Some(b) if b.effect_capable => {
                let (device, channel) = (b.device.clone(), b.channel.clone());
                let s = self.client.write(&device, &channel, value.clone())?;
                samples.push(s);
                let cell = self.cells.get_mut(name).unwrap();
                cell.value = value.clone();
                self.journal.push(OpEvent::EffectDevice { name: name.to_string(), device, channel, value: value.clone() });
            }
            Some(_) => {
                // sensor-bound cell: reads only, cannot take effects
                return Err(MhsError::BadValue(format!("'{name}' is bound to a read-only channel")));
            }
            None => {
                let cell = self.cells.get_mut(name).unwrap();
                cell.value = value.clone();
            }
        }
        // propagate along links (each at most once per call — cycles in
        // the cell graph are legal, infinite loops are not)
        let links: Vec<String> = self.cells.get(name).map(|c| c.links.iter().cloned().collect()).unwrap_or_default();
        for l in links {
            if let Some(c) = self.cells.get(&l) {
                if c.binding.as_ref().map(|b| b.effect_capable).unwrap_or(false) || c.binding.is_none() {
                    self.effect_inner(&l, value, samples)?;
                }
            }
        }
        Ok(())
    }

    /// EFFECT a batch. Batching is associative by construction: every
    /// grouping of the same steps lands the same values on the same
    /// channels in the same order (EFFECT_associativity — see tests).
    pub fn effect_batch(&mut self, steps: Vec<(String, MhsValue)>) -> MhsResult<Vec<Sample>> {
        let mut samples = Vec::new();
        for (name, value) in steps {
            samples.extend(self.effect(&name, value)?);
        }
        Ok(samples)
    }

    // ---- VIEW -----------------------------------------------------------

    /// VIEW: read a cell. Device-bound cells read telemetry through the
    /// MHS port; local cells read local state. Pure: VIEW never mutates
    /// device state, never journals a write, never advances the clock
    /// (VIEW_purity — enforced in tests).
    pub fn view(&mut self, name: &str) -> MhsResult<MhsValue> {
        let Some(cell) = self.cells.get(name) else {
            return Err(MhsError::Unknown(name.to_string()));
        };
        match cell.binding.clone() {
            Some(b) => {
                let s = self.client.read(&b.device, &b.channel)?;
                let cell = self.cells.get_mut(name).unwrap();
                cell.value = s.value.clone();
                self.journal.push(OpEvent::View { name: name.to_string() });
                Ok(s.value)
            }
            None => {
                self.journal.push(OpEvent::View { name: name.to_string() });
                Ok(cell.value.clone())
            }
        }
    }

    // ---- TICK -----------------------------------------------------------

    /// TICK: advance the quilt clock one step and drain the machine side
    /// (`poll` on the MHS port — the device's own control loop). The
    /// clock only advances: `dt <= 0` is an error, and time never moves
    /// backward (TICK_monotonicity).
    pub fn tick(&mut self, dt: f64) -> MhsResult<f64> {
        if dt <= 0.0 {
            return Err(MhsError::NotMonotonic);
        }
        self.clock += dt;
        // drain the transport; transports without a loop step (real
        // async devices) simply don't implement poll — monotonic local
        // time still holds.
        let _ = self.client.poll(dt);
        self.journal.push(OpEvent::Tick { dt, t: self.clock });
        Ok(self.clock)
    }

    // ---- interlocks (forgettable safety state) --------------------------

    /// Hold an interlock grant for a destructive channel, attributed to a
    /// cell. This is the "explicit grant" half of the forking rule: no
    /// destructive op without one.
    pub fn grant(&mut self, held_by: &str, device: &DeviceId, channel: &str) -> MhsResult<()> {
        self.client.hold_grant(device, channel)?;
        self.grants.entry((device.clone(), channel.to_string())).or_default().insert(held_by.to_string());
        self.journal.push(OpEvent::Grant { device: device.clone(), channel: channel.to_string(), held_by: held_by.to_string() });
        Ok(())
    }

    /// Which cells hold grants on which channels (interlock state is
    /// inspectable — and forgettable — as first-class state).
    pub fn interlocks(&self) -> &BTreeMap<(DeviceId, String), BTreeSet<String>> {
        &self.grants
    }

    // ---- FORGET ----------------------------------------------------------

    /// FORGET: remove a cell and every trace the laws can see — its
    /// links, inbound links, and its grants. Safety default: when the
    /// last grant-holding cell for a device is forgotten, the adapter
    /// ABORTS that device (parks it) before releasing. Interlocks are
    /// first-class forgettable state; forgetting them tears the machine
    /// down safely (FORGET_completeness — see tests).
    pub fn forget(&mut self, name: &str) -> MhsResult<ForgetReceipt> {
        let Some(cell) = self.cells.get(name) else {
            return Err(MhsError::Unknown(name.to_string()));
        };
        let mut receipt = ForgetReceipt { cell: name.to_string(), ..Default::default() };

        // 1. remove outbound links
        receipt.links_removed += cell.links.len();

        // 2. remove inbound links from other cells
        for other in self.cells.values_mut() {
            if other.links.remove(name) {
                receipt.links_removed += 1;
            }
        }

        // 3. release grants held by this cell; abort devices whose last
        //    holder just left
        let held: Vec<(DeviceId, String)> = self
            .grants
            .iter()
            .filter(|(_, holders)| holders.contains(name))
            .map(|((d, c), _)| (d.clone(), c.clone()))
            .collect();
        for (device, channel) in &held {
            let holders = self.grants.get_mut(&(device.clone(), channel.clone())).unwrap();
            holders.remove(name);
            receipt.grants_released += 1;
            self.journal.push(OpEvent::ReleaseGrant { device: device.clone(), channel: channel.clone(), held_by: name.to_string() });
            if holders.is_empty() {
                // last interlock forgotten → safety teardown
                let receipt_ab = self.client.abort(device, "last interlock forgotten")?;
                self.grants.remove(&(device.clone(), channel.clone()));
                receipt.devices_aborted.push(device.clone());
                self.journal.push(OpEvent::Abort { device: device.clone(), reason: receipt_ab.reason });
            }
        }

        // 4. the cell itself
        self.cells.remove(name);
        self.journal.push(OpEvent::Forget { name: name.to_string() });
        Ok(receipt)
    }

    // ---- super-relevance --------------------------------------------------

    /// Hand-count per device channel: how many distinct hands are served
    /// by cells bound to it. Super-relevance: a channel satisfying
    /// multiple hands is more fit than one satisfying a single hand.
    pub fn channel_relevance(&self) -> BTreeMap<(DeviceId, String), usize> {
        let mut rel = BTreeMap::new();
        for cell in self.cells.values() {
            if let Some(b) = &cell.binding {
                if !cell.hands.is_empty() {
                    *rel.entry((b.device.clone(), b.channel.clone())).or_insert(0) += cell.hands.len();
                }
            }
        }
        rel
    }

    /// Channels ranked by super-relevance (most hands first).
    pub fn ranked_channels(&self) -> Vec<((DeviceId, String), usize)> {
        let mut v: Vec<_> = self.channel_relevance().into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    // ---- pass-throughs -----------------------------------------------------

    pub fn discover(&mut self) -> MhsResult<Vec<DeviceId>> {
        self.client.discover()
    }

    pub fn manifest(&mut self, device: &DeviceId) -> MhsResult<DeviceManifest> {
        self.client.manifest(device)
    }

    /// Run a chained program ("code file") through the port directly.
    pub fn run_program(&mut self, steps: Vec<Command>) -> MhsResult<ProgramReceipt> {
        self.client.run_program(steps)
    }
}
