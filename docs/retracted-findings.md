# Retracted findings

Claims this project once recorded as established, which later measurement
disproved. They are kept because each one cost real bench time and each is
easy to arrive at again. If you are about to conclude one of these, read the
entry first.

The pattern behind most of them is the same: **a difference measured between
two conditions that were not measured at the same time.** See
[bench-measurement.md](bench-measurement.md).

---

## "The card is running firmware 16.53" — WRONG until 2026-09-01

It was running the factory image `E320_PCB6.0_PWM_FPGA10.81_20230907`. The
notes recorded 16.53 because a restore had been performed and assumed to have
installed it; the restore put back the day-one dump, which is 10.81. The
gateware analysis caught it from the dumps
([fpga/flash-layout.md](fpga/flash-layout.md)) and `discover` confirms it
directly — **the card reports its own firmware version, so just ask it**:

```
e120 discover        # -> receiver card #186: id=0x64 firmware=16.53 ...
```

Consequence: much of the FPGA analysis targets 16.53 while the bench was
running 10.81. Check which image a claim refers to before acting on it.

## "The panel shows our content, scrambled" — WRONG

It was showing a buffer nothing was driving. On 10.81 the panel changed **with
no network traffic at all**: three photos five seconds apart, every streamer
killed, differed by a mean absolute 29–37 levels of 255, with mean brightness
wandering 226 → 200 → 235.

This one is the root of several others below, because it makes any single
before/after comparison meaningless. Installing 16.53 fixed it (the same test
now gives 1.6–1.8, camera noise) — see
[firmware-16.53-bench-result.md](firmware-16.53-bench-result.md).

**Always re-run the idle test after any change**: kill every streamer, take
three photos five seconds apart, and confirm the panel is static before
believing anything you measure.

## "White draws more current than black, so content reaches the panel" — WRONG

The reported gap was ~0.15 A (2.195/2.222 vs 2.039/2.053). Measured properly —
interleaved, repeated, spread reported — black and white differ by **0.001 A
against a within-condition spread of 0.033 A**. The original gap was drift
between two sequential measurements.

A related earlier version of this error compared white current from one config
against black from a *different* config.

## "The raw row layout shows content contrast" — WRONG

A same-colour control killed it: sending *identical* content twice alternated
the supply current by over an amp (3.14 → 4.57 → 3.14 → 4.60). The card has a
per-run state toggle. Any A/B test on this rig needs a same-colour control or
interleaving.

## "The seller's pixel mapping is an outlier against 34 vendor configs" — WRONG

It is the correct wiring for this module, and flashing the "consensus" table
instead is what scrambled every column. The module's two row-halves alternate
along the shift chain every 64 columns; the consensus table gives each half one
contiguous 128-slot run. See [panel-wiring.md](panel-wiring.md).

This was pinned by a test named
`the_sellers_outlier_mapping_is_not_what_the_knobs_produce`, which asserted our
own construction was right and the seller's file wrong. **Two test fixtures —
`tests/fixtures/p25-128x64-fixed.rcvbp` and the "consensus donor" — are our own
artefacts, not vendor ground truth.** Compare against
`third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`, which is the
file that shipped with the panel.

## "IsPWMChip(0x214) is false, so try Normal firmware" — WRONG

Flashing Normal 13.39 left the panel completely dead (0.44 A). The chips are
PWM-class. `0x214` is a dead id in the vendor's own code: every chip jump table
sends it to the default arm, so it gets no registers, no chip control and no
PWM classification. Use `0x014C`.

## "The card's built-in test generator does nothing" — WAS TRUE, no longer

On 10.81 all nine selectors gave flat current and indistinguishable output. On
16.53 the selectors produce visibly different displays. The generator works; it
just renders garbage, which is now a *useful* signal — it bypasses the host
entirely, so a fault visible in test mode is at or below the card's raster
stage.

## "Press the physical test button" — not useful on this card

The owner reports the button does nothing when pressed. Do not build a
diagnosis around it. `e120 test-mode <n>` reaches the same generator over the
wire.

## Camera traps that produced false structure

* **Auto-exposure clips every LED to white** at normal brightness, which reads
  as "the panel is white" when it is not. Shoot at brightness 6–20.
* **Auto-gain makes absolute brightness incomparable between shots.** A mostly
  dark panel gets boosted. Only compare structure, or difference two shots
  taken under the same conditions.
* **Thresholding a single frame to find the panel finds the window and the
  turntable lid instead.** Locate the panel by differencing lit against
  blanked: `scripts/locate.py`.
* The panel light reflects off nearby surfaces, so even the difference image
  includes reflections; take the brightest connected region, not the bounding
  box of everything above a threshold.
* **The panel is mounted rotated 90°** — it reads 64 wide × 128 tall in frame.
  A "vertical stripe" on camera is a constant-x band on the panel.
