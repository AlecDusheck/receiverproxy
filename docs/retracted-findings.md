# Verified negatives

Claims about this card and panel that measurement disproves. Each is easy to
arrive at from a single reading, so check the table before concluding one of
them. Most rest on the same error: a difference between two conditions that
were not measured at the same time. The measurement method is in
[bench.md](bench.md).

## Card and firmware

| claim | why it is false | measurement or test |
|---|---|---|
| A `flash restore` of the factory snapshot leaves the card on firmware 16.53 | The factory image is `E320_PCB6.0_PWM_FPGA10.81_20230907`; restoring the factory dump reinstalls 10.81. The card reports its own version and is the only authority for it | `rxp discover` → `receiver card #186: id=0x64 firmware=16.53 ...`; the flash dumps ([fpga/flash-layout.md](fpga/flash-layout.md)). Any claim about card behaviour must name the firmware it was measured on |
| On firmware 10.81 the panel shows host content, scrambled | On 10.81 the panel displays a buffer nothing drives and changes with no network traffic. Any single before/after comparison on 10.81 is meaningless | Idle test: every streamer killed, three photos five seconds apart. 10.81: mean absolute difference 29–37 levels of 255, mean brightness 226 / 200 / 235. 16.53: 1.6–1.8 (camera noise), 189 / 189 / 189. Re-run the idle test after any change before trusting a measurement |
| `IsPWMChip(0x214)` is false, so the drivers need Normal-class firmware | The chips are PWM-class. `0x214` is a dead id in the vendor code: every chip jump table sends it to the default arm, so it gets no registers, no chip control and no PWM classification. The working id is `0x014C` | Normal 13.39 flashed: panel dead at 0.44 A |
| The card's built-in test generator does nothing | It is inert only on 10.81. On 16.53 the nine selectors give visibly different displays. The generator bypasses the host, so a fault visible in test mode is at or below the card's raster stage | 10.81: all nine selectors give flat current and indistinguishable output. 16.53: visibly different output per selector. `rxp card test-mode <n>` |
| The physical test button is a diagnostic | The button on this card does nothing when pressed | Operator observation. `rxp card test-mode <n>` reaches the same generator over the wire |
| A blank panel with `discover` reporting 128x64 is a panel fault | Erasing flash block 0x07 clears the EEPROM mirror. `rxp card screen-size --set` is a read-modify-write over all 256 bytes, spanning every record in [eeprom-map.md](eeprom-map.md); run on an erased mirror it persists `0xFF` across the control area, the calibration flags, the card name and the seam settings. The receiver keeps only pixels inside its control area; at `startX = startY = 0xFFFF` the window is empty, every pixel is dropped, and `discover` still reports 128x64. Frames are accepted, the packet counter advances, the current changes, nothing displays | `scripts/flash-review.py` diffs block 0x07 against the factory dump and names every differing run. `card screen-size --set` refuses to write a record that reads as erased. `scripts/eeprom-restore.py` rewrites damaged records; each record must be written at its own address and length, because the card silently ignores a write spanning record boundaries (a 16-byte write at `0x040` does nothing; the 42-byte write at `0x002` takes). Flashing firmware also erases block 0x07 |

## Configuration

| claim | why it is false | measurement or test |
|---|---|---|
| The reference file's pixel mapping is an outlier against 34 vendor configs and should be replaced by the consensus table | The reference mapping is the correct wiring for this module. The module's two row-halves alternate along the shift chain every 64 columns; the consensus table gives each half one contiguous 128-slot run and scrambles every column | Consensus table flashed: every column scrambled ([panel-wiring.md](panel-wiring.md)). Pinned by `the_reference_mapping_is_reproduced_by_the_block_knob` in `crates/rcvbp/tests/factory.rs`. A former test, `the_sellers_outlier_mapping_is_not_what_the_knobs_produce`, asserted the opposite and is removed. `crates/rcvbp/tests/fixtures/p25-128x64-fixed.rcvbp` and `third-party/configs/donor-P2.5-320x160-2153-consensus.rcvbp` are this project's own artefacts, not vendor ground truth; the reference is `third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`, whose records match the card's factory flash under test |
| Only 12-bit grey renders; 14 and 16 never reach the chips | Configured from flash, grey 12, 13, 14, 15 and 16 render identically with identical currents. The requirements are `+0x02F = 1`, rows then latches, and booting from flash | Measured from flash on 16.53 ([rendering.md](rendering.md)). The 12-only result came from `config send` RAM pushes, which land on about one boot in three. A result obtained through a RAM push and not reproduced from flash counts as unmeasured |

## Current readings

| claim | why it is false | measurement or test |
|---|---|---|
| White draws about 0.15 A more than black, so content reaches the panel (10.81, original config) | The 0.15 A (2.195/2.222 A white vs 2.039/2.053 A black) is drift between two sequential readings. Measured interleaved and repeated, black and white differ by 0.001 A against a within-condition spread of 0.033 A | Interleaved runs with spread reported (`scripts/bench.py run`). A variant of the same error compares white from one config against black from a different config |
| The raw row layout shows content contrast | Sending identical content twice alternates the supply current by over an amp: the card has a per-run state toggle. An A/B difference without a same-content control measures the toggle | Same-content control: 3.14 → 4.57 → 3.14 → 4.60 A for identical frames. Every A/B test needs a same-content control or interleaving |

## Camera readings

| claim | why it is false | measurement or test |
|---|---|---|
| `scripts/bench.py capture` photos are 24-frame averages | `tmix=frames=N` emits one output frame per input frame, and its early outputs average only the frames seen so far; the first output is a single frame. Taking `-frames:v 1` selects that first output. The panel multiplexes 1/16, so a single 1/30 s exposure catches one phase of the scan and shows horizontal banding, which reads as scrambled content | Fixed by priming the filter: capture 2N frames and keep one from after the window is full. The primed capture visibly changes what the panel appears to show. Every capture taken before the primed filter (before 2026-09-01) is a single 1/30 s exposure |
| The panel is white | Auto-exposure clips every LED to white at normal brightness | Shoot at brightness 6–20 |
| Two shots differ in absolute brightness | Auto-gain boosts a mostly dark panel; absolute brightness is not comparable between shots | Compare structure only, or difference two shots taken under the same conditions |
| Thresholding one frame locates the panel | The threshold finds the window and the turntable lid | Locate by differencing lit against blanked: `scripts/bench.py locate`. Panel light reflects off nearby surfaces, so take the brightest connected region, not the bounding box of everything above threshold |
| A vertical stripe on camera is a vertical stripe on the panel | The panel is mounted rotated 90° and reads 64 wide by 128 tall in frame; a vertical stripe on camera is a constant-x band on the panel | Rig geometry ([bench.md](bench.md)) |
