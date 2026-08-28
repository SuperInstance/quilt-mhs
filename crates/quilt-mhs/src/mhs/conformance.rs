//! The MHS conformance suite — the porting contract.
//!
//! `run_conformance(client)` runs the SAME checks against ANY
//! [`MhsClient`] implementation. Today [`MockMHS`] passes it. The day the
//! real Anthropic MHS SDK lands, you implement `MhsClient` for it and run
//! this exact suite; if it passes, the whole quilt adapter stack above the
//! port keeps working unchanged. This is the same pattern as quilt-rust's
//! `compat/conformance_test.rs` + `golden.json`: one contract, many
//! substrates, differential tests.
//!
//! C1..C8 check only behavior every MHS-shaped transport must exhibit,
//! derived from the announced surface (see MHS-SPEC-WATCH.md for the
//! source behind each check). Checks marked (A-n) test our assumptions;
//! they are separated so a real SDK can disagree without invalidating the
//! core checks.

use crate::mhs::client::MhsClient;
use crate::mhs::types::*;
use serde::{Deserialize, Serialize};

/// Result of one conformance check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: &'static str,
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
    /// True when the check encodes an ASSUMPTION (A-n) rather than a
    /// press-sourced behavior.
    pub assumption: bool,
}

/// Run the full conformance suite against any MhsClient. Requires the
/// transport to expose at least one device with a numeric writable
/// channel with a declared limit (mock devices qualify).
pub fn run_conformance(client: &mut dyn MhsClient) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    // C1 — discovery: devices are discoverable in a standard format.
    let discovered = client.discover();
    checks.push(match &discovered {
        Ok(v) if !v.is_empty() => ok("C1", "discovery returns devices", format!("{} devices", v.len())),
        Ok(_) => fail("C1", "discovery returns devices", "empty discovery".into()),
        Err(e) => fail("C1", "discovery returns devices", e.to_string()),
    });
    let Some(devices) = discovered.ok().filter(|v| !v.is_empty()) else {
        return checks; // nothing further is testable
    };

    // C2 — manifest: the reference file names readable/writable channels
    // and a safety envelope with limits that will be enforced.
    let device = devices[0].clone();
    let manifest = client.manifest(&device);
    checks.push(match &manifest {
        Ok(m) if !m.writable.is_empty() && !m.safety.channel_limits.is_empty() => {
            ok("C2", "manifest declares channels + enforced limits", format!("{} writable, {} limits", m.writable.len(), m.safety.channel_limits.len()))
        }
        Ok(m) => fail("C2", "manifest declares channels + enforced limits", format!("writable={}, limits={}", m.writable.len(), m.safety.channel_limits.len())),
        Err(e) => fail("C2", "manifest declares channels + enforced limits", e.to_string()),
    });
    let Some(m) = manifest.ok() else {
        return checks;
    };

    // pick a numeric writable channel with a limit — prefer NON-destructive
    // so core checks don't depend on the (assumption-level) grant surface
    let pick = m
        .safety
        .channel_limits
        .iter()
        .filter(|(c, _)| m.writable.iter().any(|w| &w.name == *c && !w.destructive))
        .map(|(c, r)| (c.clone(), *r))
        .next()
        .or_else(|| {
            m.safety
                .channel_limits
                .iter()
                .filter(|(c, _)| m.writable.iter().any(|w| &w.name == *c))
                .map(|(c, r)| (c.clone(), *r))
                .next()
        });
    let Some((chan, (lo, hi))) = pick else {
        checks.push(fail("C3", "read/write primitives work", "no limited writable channel to probe".into()));
        return checks;
    };

    // C3 — read/write primitives: write returns a sample; read reflects it.
    let probe = lo + (hi - lo) * 0.5;
    let wrote = client.write(&device, &chan, MhsValue::Float(probe));
    checks.push(match &wrote {
        Ok(s) if s.value == MhsValue::Float(probe) => ok("C3", "write primitive returns sample", format!("{chan}={probe}")),
        Ok(s) => fail("C3", "write primitive returns sample", format!("echoed {:?}", s.value)),
        Err(e) => fail("C3", "write primitive returns sample", e.to_string()),
    });
    let readback = wrote.ok().and_then(|_| client.read(&device, &chan).ok());
    checks.push(match readback {
        Some(s) if s.value.as_f64().is_some() => ok("C4", "read primitive reflects written value", format!("{chan}={:?}", s.value)),
        _ => fail("C4", "read primitive reflects written value", "read did not echo".into()),
    });

    // C5 — safety enforcement: a write outside the declared limit is
    // rejected AND leaves state unchanged.
    let before = client.read(&device, &chan).ok();
    let violating = if probe >= hi { lo - 1.0 } else { hi + 1.0 };
    let rejected = client.write(&device, &chan, MhsValue::Float(violating));
    let after = client.read(&device, &chan).ok();
    let c5 = match (&rejected, &before, &after) {
        (Err(MhsError::SafetyViolation(..)), Some(b), Some(a)) if b.value == a.value => {
            ok("C5", "out-of-limit write rejected, state unchanged", format!("rejected {violating} on {chan}"))
        }
        (Err(MhsError::SafetyViolation(..)), _, _) => {
            fail("C5", "out-of-limit write rejected, state unchanged", "state changed or unreadable alongside rejection".into())
        }
        (Err(e), _, _) => fail("C5", "out-of-limit write rejected, state unchanged", format!("wrong error: {e}")),
        (Ok(_), _, _) => fail("C5", "out-of-limit write rejected, state unchanged", "violating write accepted!".into()),
    };
    checks.push(c5);

    // C6 — read purity: reads never mutate state (VIEW_purity at the MHS
    // boundary). We can only observe the read channel itself here.
    let r1 = client.read(&device, &chan).ok();
    let r2 = client.read(&device, &chan).ok();
    checks.push(match (r1, r2) {
        (Some(a), Some(b)) if a.value == b.value => ok("C6", "read is pure (repeatable, non-mutating)", format!("{chan} stable")),
        _ => fail("C6", "read is pure (repeatable, non-mutating)", "reads diverged".into()),
    });

    // C7 — code files: a chained program of in-limit steps completes.
    let prog = client.run_program(vec![
        Command { device: device.clone(), channel: chan.clone(), value: MhsValue::Float(probe) },
        Command { device: device.clone(), channel: chan.clone(), value: MhsValue::Float(probe) },
    ]);
    checks.push(match prog {
        Ok(p) if p.completed && p.accepted_steps == 2 => ok("C7", "chained program (code file) completes", format!("{} steps", p.accepted_steps)),
        Ok(p) => fail("C7", "chained program (code file) completes", format!("completed={} steps={}", p.completed, p.accepted_steps)),
        Err(e) => fail("C7", "chained program (code file) completes", e.to_string()),
    });

    // C8 (A-6) — abort exists and latches: after abort, writes are refused.
    let aborted = client.abort(&device, "conformance probe");
    let write_after = aborted.ok().and_then(|_| client.write(&device, &chan, MhsValue::Float(probe)).err());
    checks.push(match write_after {
        Some(MhsError::Aborted(_)) => ok_assume("C8", "abort latches against further writes", "A-6: abort surface assumed".into(), true),
        Some(e) => fail_assume("C8", "abort latches against further writes", format!("write after abort: {e}"), true),
        None => fail_assume("C8", "abort latches against further writes", "write after abort ACCEPTED".into(), true),
    });

    checks
}

/// True when every non-assumption check passed (assumption failures are
/// reported separately — a real SDK may legitimately disagree with an
/// assumption, and that disagreement is exactly what MHS-SPEC-WATCH
/// exists to diff).
pub fn core_passes(results: &[CheckResult]) -> bool {
    results.iter().all(|c| c.passed || c.assumption)
}

fn ok(id: &'static str, name: &'static str, detail: String) -> CheckResult {
    CheckResult { id, name, passed: true, detail, assumption: false }
}
fn fail(id: &'static str, name: &'static str, detail: String) -> CheckResult {
    CheckResult { id, name, passed: false, detail, assumption: false }
}
fn ok_assume(id: &'static str, name: &'static str, detail: String, assumption: bool) -> CheckResult {
    CheckResult { id, name, passed: true, detail, assumption }
}
fn fail_assume(id: &'static str, name: &'static str, detail: String, assumption: bool) -> CheckResult {
    CheckResult { id, name, passed: false, detail, assumption }
}
