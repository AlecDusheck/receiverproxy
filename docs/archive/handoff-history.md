# Handoff history

The narrative the top-level `HANDOFF.md` carried until 2026-09-02, kept as
the record of how the bring-up went. The current state is in `HANDOFF.md`;
the measurements are in [rendering.md](../rendering.md) and the withdrawn
claims in [retracted-findings.md](../retracted-findings.md).

## 2026-09-01, evening

The panel renders sent patterns. Four row bands and four column bands come up
as coherent colour stripes, half-lit patterns land as solid rectangles in the
right places, white draws current in proportion, and the same-content control
returns. The recipe is in `config/panels/p25-128x64-sm16269s.toml`:

* chip id `0x14C` (the vendor's "SM16169SH" SH stream), the only identity
  firmware 16.53 brings the SM16269S outputs up for;
* grey depth 12 (`[module] gray_bits`); the card arrived saying 14. At the
  time this was recorded as "at 14 and 16 pixel data never reaches the chips'
  SRAM"; that was measured through RAM pushes and later retracted (12–16
  render alike from flash);
* `+0x02F = 1`, the vendor Reset() default; inherited cleared;
* `rows` raster, brightness then rows then three latches per frame (one latch
  never starts the display, two decays on a ~10 s period, three holds);
* mapping `block = 64`, firmware 16.53, EEPROM control area intact.

Black is black (0.466 A, LEDs off) since the void-line column table displaces
the phantom line positions `width..2·width` the card was driving with a fixed
pattern (`mapping.gate_phantom_positions`). Grey response is monotonic and
every test pattern renders. Remaining: row band order reads reversed on the
rotated panel; a faint perceived flicker not measurable with the camera.

Provisioning a card is one command:

```
e120 provision --spec config/panels/p25-128x64-sm16269s.toml \
    --firmware third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex \
    --position 0,0 --commit
```

## 2026-09-01, afternoon: the empty control window

The card had been left with `startX = startY = 0xFFFF` in its EEPROM control
area after a block-0x07 erase and a 256-byte `card screen-size --set`
read-modify-write that persisted the erased bytes. Discovery reported a
healthy 128x64 while every pixel was dropped. Found by static analysis of
the vendor library's `WriteEepromCtrlAreaOffset` and confirmed from the flash
mirror at `0x7F000`. Fixed by writing the records back one at a time.

## 2026-09-01, morning: firmware 16.53

The card had been running the factory 10.81 image although the notes said
16.53. On 10.81 the panel changed with no traffic at all (mean absolute
difference 29–37 levels between photos five seconds apart); on 16.53 it is
static (1.6–1.8). After the install the panel still did not render and black
and white drew the same current; the card's own test patterns also rendered
as garbage, which put the fault at or below the card's raster stage. Open
item at that point: derive the panel-driving parameters (scan addressing,
chip protocol, timing) from the hardware instead of inheriting the seller's.
Two analyses were in progress: the gateware's 0x55 receive path and what
gates a pixel write (`docs/fpga/pixel-write-path.md`), and the vendor SDK's
Screen Connection step (`screen-connection-wire.md`, `receiver-identity.md`).

## Earlier

* Wire protocol recovered byte-exact from CLTNic.dll ([pixel-protocol.md](../pixel-protocol.md)).
* Config generation from TOML alone with no donor file; the seller's shipped
  config regenerates record for record under test.
* Gateware analysis of the ECP5 bitstream (`docs/fpga/`), including a
  negative-results file so dead searches are not repeated.
* The pixel mapping fixed: the module's row-halves interleave every 64
  columns, and the "consensus" table that had been flashed was wrong
  ([panel-wiring.md](../panel-wiring.md)).
* `scripts/bench.py capture` silently took single frames until 2026-09-01;
  the panel multiplexes 1/16, so one exposure is scan phase, not content.

## Assets

`card-dumps/primary-region.bin` (day-one dump, ground truth; this is 10.81),
`build/snapshot-*/` (pre-firmware-flash snapshots: primary region, golden
bank, live config), `third-party/firmware/` (five vendor images; 16.53 is the
only one naming SM16269SH),
`third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp` (the reference
file; its records match the card's day-one flash), `analysis/record01-fieldmine/` (corpus statistics),
`analysis/fpga/`.

`crates/e120-rcvbp/tests/fixtures/p25-128x64-fixed.rcvbp` and the "consensus
donor" are our own constructions, not vendor ground truth; two tests once
pinned them as such and were wrong.
