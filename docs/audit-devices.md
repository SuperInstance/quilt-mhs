# MHS Device Audit — `quilt-mhs`

**Date:** 2026-08-27 · **Scope:** what `MockMHS` simulates today vs. what the F16 partner list implies the seam should cover.

## 1. What exists today

Two devices in `mhs/mock.rs`, both first-order-physics mock-ups of canonical press examples.

| DeviceId | Model | Readable channels | Writable channels | Tags (natural-language) |
|---|---|---|---|---|
| `mock-arm-01` | "MockArm S1" | `joint1.angle` (deg, −90..90), `load.cell` (kg, 0..5) | `joint1.target` (deg, ±90, 10 Hz rate-limit), `gripper.cmd` (frac 0..1, **destructive**) | Arm mass 18.5 kg, mount to 400 kg slab; 30 deg/s mech slew; gripper 1.0 crushes samples |
| `mock-thermal-01` | "MockBath T1" | `bath.temperature` (degC, 0..100) | `bath.setpoint` (degC, 0..100), `pump.duty` (%, 0..100) | 2 L water bath; boils dry above 100 degC; never command >95 unattended |

Dynamics: first-order relaxation, `cur + (goal - cur)·(1 - exp(-dt/τ))`, with `τ=0.6s` for the arm joint and `τ=20s` for the bath. Both devices have `destructive_requires_grant=true` and `abort_supported=true`; abort parks every writable channel at its range floor. The substrate profile (`device/mod.rs`) ships a third "device" — `QuiltDeviceProfile::demo` (a boat sheet) — but that's a *substrate-as-device*, not a hardware mock, so it's out of scope for the device roster.

## 2. What should exist — five more candidates, drawn from F16

For each, channels use the manifest vocabulary (`name`, `unit`, `range`, `writable`, `destructive`). Tags are paraphrased from vendor manuals, not invented.

**A. `mock-incubator-01` — Thermo Fisher Heracell VIOS 160i CO₂ incubator.** 165 L, IR CO₂ sensor, 37 °C setpoint, 5 % CO₂. Tags: *"For adherent HEK293 / iPSC cultures; do not open the door >2×/hr or CO₂ recovery exceeds 10 min."* Channels: `chamber.temperature` R 5..50 °C; `chamber.co2` R 0..20 %; `door.state` R bool; `chamber.temp_setpoint` W 5..50 °C (destructive-ish); `chamber.co2_setpoint` W 0..20 %. Dynamics: `T_{t+1} = T_t + dt/τ_T·(T_sp − T_t) − K_door·door_open`, τ_T ≈ 240 s; `CO2_{t+1} = CO2_t + dt/τ_C·(CO2_sp − CO2_t) − K_door·door_open`, τ_C ≈ 180 s.

**B. `mock-microscope-01` — Zeiss Axio Observer 7 + Hamamatsu ORCA-Flash 4.0.** Inverted, 5-objective turret, sCMOS. Tags: *"Objective 40×/1.2 W Corr is water-immersion; refill reservoir every 2 hr to avoid drift."* Channels: `stage.x/y/z` R µm, R 0..100000; `objective.mag` R/W 1,2.5,5,10,20,40,63,100×; `camera.exposure_ms` W 1..60000; `camera.gain` W 0..1000; `illumination.intensity` W 0..100 %; `shutter.cmd` W open/close (**destructive** — photobleaching). Dynamics: stage first-order with τ=80 ms; `exp → SNR` linear; bleaching proxy `B_{t+1} = B_t + α·I·exp·dt`.

**C. `mock-pipette-01` — Tecan Fluent 1080 + MCA 96 head.** Air-displacement, 1–1000 µL. Tags: *"1000 µL tip aspirate at speed 8 = ~1.2 s; never aspirate below tip cone or you shear cells."* Channels: `head.z` R/W mm; `tip.volume` R/W µL 0..1000; `tip.speed` W 1..10; `aspirate.cmd` W trigger (**destructive**); `dispense.cmd` W trigger (**destructive**); `well.plate_index` R/W int. Dynamics: `z_{t+1} = z_t + dt/τ_z·(z_sp − z_t)`, τ_z ≈ 30 ms; `V_{t+1} = V_t + flow·dt`, flow = ±(speed/10)·V_max.

**D. `mock-laser-01` — QuEra-style 689 nm Rydberg laser + reference cavity.** The press's *only* quantitative case (F12: 99.3 % lock recovery). Tags: *"Lock holds ±200 kHz on the Rb-87 D2 line; mechanical disturbance >0.3 g breaks lock for ~6 s."* Channels: `lock.error_hz` R; `piezo.voltage` R/W 0..150 V; `current.ma` W 0..500; `lock.state` R enum {unlocked, locking, locked}; `shutter.cmd` W open/close (**destructive**). Dynamics: `err_{t+1} = err_t + K·(V_piezo − V_pid)`, V_pid from PI; lock transition Markov, P(locked) = σ(−|err|/σ_lock).

**E. `mock-pipettor-plate-01` — MBF Bioscience ScanImage + Prior H117 stage.** Already named in F16 as a building partner. Tags: *"Frame rate 30 fps at 2048×2048; Z-stack 50 µm step, 0.5 s settle per plane."* Channels: same as C plus `stack.z_start`, `stack.z_step`, `stack.num_planes`, `scan.channel` (1..4 fluorescence).

## 3. Effort ranking — top 3 for Phase 215

1. **`mock-incubator-01`** — highest leverage. Every cell-biology workflow in F16 (Genentech, HHMI Janelia, MBF) assumes one. Two channels with door-state coupling exercises the *full* safety story: range, rate, abort, grant, plus a *disturbance* the controller can choose to read vs. ignore. Maps directly to Phase-215 item #4.
2. **`mock-microscope-01`** — exercises the most diverse channel types: enum (`objective.mag`), bool (shutter), bounded continuous (gain), and a `destructive` write. Lock-step stage + camera dynamics is the same shape as the QuEra laser lock — *one* mock validates two F16 partners.
3. **`mock-laser-01`** — closes the loop on the only press-sourced number (F12). The `lock.state` enum is the *first* non-numeric MhsValue in flight, which pressure-tests `MhsValue::Str`/`Bool` paths the current mocks never touch.

Plate handler (#6) and pipette (#C) are also Phase-215 candidates but pipette dynamics are largely deterministic kinematic — less interesting than the incubator's door-disturbance coupling.

## 4. The "lab of the future" pick

**`mock-incubator-01`** (Heracell VIOS 160i). If a quilt cell could command chamber temperature, CO₂ setpoint, and door state — and read them back under the MHS safety envelope — then a single TICK-loop could drive a real mammalian-cell experiment end-to-end: thaw cells → seed in plate → `chamber.temp_setpoint=37, chamber.co2_setpoint=5, door.state=closed` → hand the plate off to a downstream microscope mock. No other single device closes the loop from *"biology"* to *"MHS-shaped commands"*. It's the cell-culture equivalent of the press's USB-C analogy (F14): one device, one driver, the whole incubator vocabulary of a working lab in three writes.

## 5. The top-3, concretely (for the next PR)

### Top 3: incubator + microscope + laser

#### Device 1 — `mock-incubator-01`
- **Model:** Thermo Fisher Heracell VIOS 160i CO₂ incubator
- **Channels (6):**
  - `chamber.temperature` — degC, 5..50, R
  - `chamber.co2` — %, 0..20, R
  - `door.state` — bool, {open,closed}, R
  - `chamber.temp_setpoint` — degC, 5..50, **W**
  - `chamber.co2_setpoint` — %, 0..20, **W**
  - `chamber.scram` — bool, **W destructive** (estop → heater off, CO₂ valve closed)
- **Tags (3):** *"For adherent HEK293/iPSC; 37 °C / 5 % CO₂ nominal."* · *"Do not command >40 °C: HEPA gasket softens above 45 °C — irreversible."* · *"Door opens drop CO₂ ~0.8 %/s; recovery τ ≈ 180 s closed."*
- **Dynamics:** `T_{t+1} = T_t + dt/240·(T_sp − T_t) − 0.8·door_open`; `CO2_{t+1} = CO2_t + dt/180·(C_sp − CO2_t) − 0.008·door_open`; τ_T=240s, τ_C=180s.

#### Device 2 — `mock-microscope-01`
- **Model:** Zeiss Axio Observer 7 + Hamamatsu ORCA-Flash 4.0 sCMOS
- **Channels (5):**
  - `stage.z` — µm, 0..100000, R
  - `objective.mag` — ×, {1,2.5,5,10,20,40,63,100}, **W**
  - `camera.exposure_ms` — ms, 1..60000, **W**
  - `illumination.intensity` — %, 0..100, **W**
  - `shutter.cmd` — bool, **W destructive** (open → bleaching begins)
- **Tags (4):** *"40×/1.2 W Corr is water-immersion; refill reservoir every 2 hr."* · *"Full-frame 2048×2048 @ 30 fps; ROI halves linear."* · *"Illumination >50 % at 100× photobleaches GFP in <30 s."* · *"Stage backlash 0.4 µm — approach z from above."*
- **Dynamics:** `z_{t+1} = z_t + dt/0.08·(z_sp − z_t)`, τ_z=80 ms; bleaching `B_{t+1} = B_t + 1e-4·I·exp·dt` while `shutter=open`.

#### Device 3 — `mock-laser-01`
- **Model:** QuEra-style 689 nm Rydberg reference-cavity laser (MHS-built per F12)
- **Channels (5):**
  - `lock.error_hz` — Hz, −500..500, R
  - `lock.state` — str, {`unlocked`,`locking`,`locked`}, R
  - `piezo.voltage` — V, 0..150, **W**
  - `current.ma` — mA, 0..500, **W**
  - `shutter.cmd` — bool, **W destructive**
- **Tags (4):** *"Rb-87 D2 line, 689 nm; lock holds ±200 kHz cold."* · *"Mechanical disturbance >0.3 g breaks lock ~6 s."* · *"100 % current at 25 °C ambient → 2 W intracavity — no eyewear in beam path."* · *"99.3 % autonomous relock (F12): the controller's job is *not* to relock, but to detect and shed load."*
- **Dynamics:** `err_{t+1} = K·(V_piezo − V_pid(0.4·err_t))`; `lock.state` Markov with `P(relock) = 1/(1+exp(|err|/σ_lock))`, σ_lock=200 Hz.

All three share the same `MockDevice` skeleton: first-order relaxation in `MockMHS::tick`, range+rate enforcement in `MockDevice::enforce`, and a destructive `shutter.cmd`/`chamber.scram` channel that exercises the interlock grant path (A-7) without any new transport work.
