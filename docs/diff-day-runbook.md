# Diff-Day Runbook — when the real MHS spec lands

**Audience:** the on-call engineer the day Anthropic publishes the
MHS spec / SDK / schema. This is a checklist, not a procedure. The
procedure lives in `MHS-SPEC-WATCH.md` §3; the runbook is what you
*do*, in order, in the first six hours.

## T+0 — freeze the repo

```bash
git tag -a pre-spec -m "Last commit before the real MHS spec"
git push origin pre-spec
```

Don't merge anything new until the diff lands. The blast radius is
small (`mhs/types.rs` + the new `MhsClient` impl), but the new
*types* may rename fields the controller side already uses.

## T+15m — walk F1..F19 in MHS-SPEC-WATCH.md

Open `MHS-SPEC-WATCH.md` §1 (the F-n table). For each row, mark
`confirmed` / `corrected` / `new`. A `corrected` row means the press
got it wrong; a `new` row is a fact we missed entirely.

Pay special attention to:
- **F8 (reference file / manifest)** — the field set may differ.
- **F9 (3 control mechanisms)** — the names of MCP/CLI/code-files
  may shift.
- **F12 (QuEra 99.3%)** — note whether the real spec defines a
  benchmark or just cites the partner.
- **F19 (EU regulation)** — if real, `destructive_requires_grant`
  may be mandatory, not optional.

## T+1h — walk A-1..A-10

Each assumption in `MHS-SPEC-WATCH.md` §2 names the **exact code
that changes** when the guess is wrong. Walk them in order. For each:

1. Read the assumption.
2. Open the named file.
3. If the real spec contradicts the guess, edit the file.
4. Add a one-line comment citing the spec section.
5. Remove the `A-n` tag if the guess is now confirmed.

The A-1..A-10 column is the **whole blast radius** of assumption
changes. If you touch any other file, you're doing more than
diff-day.

## T+3h — write the new MhsClient impl

Create `crates/quilt-mhs/src/mhs/official.rs` (feature-gated as
`mhs-official`). It should be ~200 lines: one `impl MhsClient for
OfficialClient`. Behind a Cargo feature so the build is unchanged for
everyone who doesn't enable it.

```rust
#[cfg(feature = "mhs-official")]
pub struct OfficialClient { /* whatever the SDK needs */ }

#[cfg(feature = "mhs-official")]
impl MhsClient for OfficialClient { /* 9 methods, delegates to SDK */ }
```

The trait is the seam. The controller, the device profile, the
conformance suite, the laws, the federation — all unchanged.

## T+3h30m — run the conformance suite

```bash
cargo test --features mhs-official --test conformance
```

Green = real spec and our types agree. Red = real disagreements.
Compare against `MockMHS`:

```bash
cargo test --test conformance  # without the feature
```

`core_passes` should be identical. Assumption-flagged checks may
differ — that's expected; update the `A-n` tag to "confirmed by
real spec" or "wrong, fixed in commit XYZ".

## T+4h — regenerate the JSON schemas

```bash
cargo run --bin gen-schemas --features mhs-official
git diff schemas/
```

If the diff is empty, the field names match. If not, the **schema is
the public artifact**; the Rust types are the implementation. Update
the Rust types to match the schema, not vice versa.

## T+4h30m — run the laws test

```bash
cargo test --features mhs-official --test laws
```

The 5+1+1 laws must pass against the real spec too. They are
properties of the *adapter* (the BIND journal, the LINK closure,
the VIEW purity, the TICK monotonicity, the FORGET receipt), not of
the device. If a law fails, the adapter is wrong, not the device.

## T+5h — run the federation test

```bash
cargo test --features mhs-official --test federation
```

The federation test runs two `QuiltDeviceProfile`s against each other
through `FederationLink`. If the real spec changes the device
profile, federation is where the breakage shows up first.

## T+6h — update the spec-watch + bump the crate

```bash
# Update each F-n row in MHS-SPEC-WATCH.md with the new fact
# Update each A-n row with confirmed/wrong status
# Bump the crate version (0.1.0 -> 0.2.0 if any A-n was wrong;
#                         0.1.0 -> 0.1.1 if only additive changes)
git add -A
git commit -m "Phase 215 diff-day: real MHS spec landed

F1..F19 walked, A1..A10 walked, mhs-official feature added,
all 22 + 9 new tests green, schemas regenerated, crate bumped."
git tag -a v0.2.0
git push origin main --tags
```

## T+6h+1 — write a one-paragraph CHANGELOG entry

For each F-n that was corrected, one sentence. For each A-n that
was wrong, one sentence. For the new MhsClient impl, one sentence
pointing at the diff.

## Rollback plan

If anything goes wrong at any step:

```bash
git checkout pre-spec
```

The `pre-spec` tag is the rollback point. The repo runs unchanged on
`MockMHS` and the old types. The `mhs-official` feature is opt-in;
turning it off leaves the build working.

## The polyformalism promise

The seam is the trait. The day the real spec lands, **one file
changes** (`mhs/official.rs` + a feature flag). Every other
component — the controller, the device profile, the federation, the
conformance suite, the laws, the schemas — runs unchanged.

That's what "real on every component" means: not that we wrote more
code, but that the existing code absorbed the real spec with one
file's worth of change.
