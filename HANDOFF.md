# Handoff

Bring-up of a P2.5 128x64 SM16269S panel on a Colorlight E120, driven over raw
Ethernet from the Rust CLI in this repo.

**Read first:** [`docs/retracted-findings.md`](docs/retracted-findings.md) —
claims this project once recorded as established and later disproved. Several
are things you would otherwise conclude again. Then
[`docs/bench-measurement.md`](docs/bench-measurement.md), because most of them
came from measuring badly rather than reasoning badly.

## Where it stands (2026-09-01)

**The panel does not yet show sent content.** What is known about why:

* The card's **own built-in test patterns also render as garbage**. They are
  generated inside the card and never touch the host, so the fault is at or
  below the card's raster stage — in how the card drives the hub, not in what
  we transmit.
* Content-independence is now exact, not approximate: interleaved and
  repeated, all-black and all-white differ by **0.001 A** against a spread of
  0.033 A.
* Therefore the host-side raster packing and the row-base/screen-number field
  are ruled out for now. Both were swept (`image --raster`, `--row-base`) and
  neither moves the panel, which is consistent with a fault the host cannot
  reach.
* Since our config now matches the seller's file exactly and the panel still
  misrenders, **the seller's configuration is itself wrong for this module.**
  Their file is labelled `P2.5-32S`; the panel is `P2.5-O16S`.

**Check the EEPROM before anything else.** The card discards every pixel that
falls outside its control area, and ours had been erased to an empty window by
our own tooling — with `discover` still reporting a healthy 128x64 throughout.
After any flash operation:

```
e120 dump-flash --block 7 --out now.bin
python3 scripts/flash-review.py now.bin          # names every damaged record
python3 scripts/eeprom-restore.py --live now.bin # dry run; --commit to write
```

Three things were fixed today and all were real:

* **Firmware.** The card was running the factory `10.81`, not the `16.53` the
  notes claimed. On 10.81 the panel changed *with no network traffic at all*
  (29–37 levels between photos five seconds apart). On 16.53 it is static
  (1.6–1.8, camera noise) and the built-in test generator works. See
  [`docs/firmware-16.53-bench-result.md`](docs/firmware-16.53-bench-result.md).
* **Pixel mapping and driver registers.** The module's row-halves interleave
  every 64 columns; we had flashed the contiguous wiring, scrambling every
  column. See [`docs/panel-wiring.md`](docs/panel-wiring.md).
* **The control area.** Restored from the day-one dump and verified across a
  power cycle. See [`docs/receiver-identity.md`](docs/receiver-identity.md),
  [`docs/eeprom-map.md`](docs/eeprom-map.md) — the vendor device library names
  essentially every EEPROM field, so that layout is known, not guesswork.

## The rig

Mac + AX88179B on `en24` (raw Ethernet via `/dev/bpf`; `sudo chmod o+rw
/dev/bpf*` after every reboot). Card: E120, **firmware 16.53** — confirm with
`e120 discover`, which reports it; do not take it on trust. Panel: one Eager
P2.5-O16S-SMD1415-128x64-E module (1/16 duty, SM16269S read off the silicon) on
hub J1, **mounted rotated 90°** so it reads 64 wide x 128 tall on camera. PSU:
KA3005P 5 V / 5.1 A limit. Camera on the panel.

**PSU rule: read it and power-cycle it, never change voltage or current
settings.** Use `scripts/psu.sh`, which arms an automatic shut-off (default and
maximum 10 minutes) on every power-on; `psu.sh extend` pushes it out.

**Power rule:** an armed panel showing unmodulated content draws ~4.5 A and at
full brightness rails the limit and browns out. Keep brightness ≤ 40 until
content is right, and never flash while `ka3005p status` shows `CH1: Cc`.

**Vendor software: inspect, never execute.** Static analysis only, and delegate
vendor SDK file inspection to an Opus subagent.

## What is established

* **Wire protocol** — byte-exact against the vendor's own sender DLL
  ([`docs/pixel-protocol.md`](docs/pixel-protocol.md), recovered by static
  reading of CLTNic.dll): 21-byte header, `0x55` at offset 12, row/x-offset/
  count as big-endian u16 at 13/15/17, `08 88` at 19–20, pixels from 21, max
  497 per packet. Per frame: latch, brightness, then rows.
* **Config generation** — `e120 gen-config --spec config/panels/<panel>.toml`
  produces the `.rcvbp`, the basic pack and the complete boot image from erased
  flash, with a provenance line per byte, **from TOML alone with no donor
  file**. It reproduces the seller's shipped config record-for-record under
  test. Record 0x01 is decoded byte-for-byte
  ([`docs/record-0x01-fields.md`](docs/record-0x01-fields.md)), the boot image
  region by region ([`docs/compiled-image-format.md`](docs/compiled-image-format.md)).
* **Gateware** — [`docs/fpga-gateware.md`](docs/fpga-gateware.md) and
  `docs/fpga/` (bitstream format, pinout, output stage, block RAM, flash
  layout), including `negative_results_and_method.txt` so dead searches are not
  repeated.

## Open items

1. **Derive the panel-driving parameters from the hardware**, rather than
   inheriting the seller's. This is the live problem: scan addressing, the chip
   protocol and its timing. The card's own test patterns failing is the handle —
   it lets you iterate without the host in the loop.
2. Two agents were mid-flight at handoff: one on the gateware's 0x55 receive
   path and what gates a pixel write, one on the vendor SDK's screen-assignment
   / "Screen Connection" step. Check `docs/fpga/` and `docs/` for their output.
3. Once content renders: `image` / `play` are already wired; then enable
   `boot.arm_at_boot` so the panel comes up from flash.

## Procedures worth not rediscovering

* **Flashing firmware erases the parameter block.** Rewrite the config after:
  `e120 restore-flash build/<panel>-block7.bin --commit`, then power-cycle.
* The firmware verify step's **~4042-byte warning is benign** — all of it is
  the `0x7F000` parameter tail the card writes itself. Dump the region back and
  bucket differences by block before believing a failure.
* `e120 snapshot` first: it captures the primary region and the golden bank.
  The golden bank at block 0x20 is never touched by `flash-firmware`.
* `e120 send-params` pushes to **RAM only**. The mapping is read from flash at
  boot, so a mapping change needs `restore-flash` plus a power-cycle.
* `e120 set-layout` is needed after a firmware change: `discover` reported a
  nonsense "detected size 1544x128" until it was re-sent.
* **EEPROM records must be written one at a time**, at the record's own address
  and length from [`docs/eeprom-map.md`](docs/eeprom-map.md). The card silently
  ignores a write spanning record boundaries. A few records (`0x041`, `0x042`,
  `0x092`) did not take via opcode `0x85` and the vendor library reaches them
  by other paths; they are still `0xFF`.
* **Photos:** `scripts/snap-avg.sh` silently took single frames until
  2026-09-01. The panel multiplexes 1/16, so one exposure is scan phase, not
  content. It now primes the average; keep it that way.

## Assets

`card-dumps/primary-region.bin` (day-one dump, ground truth — this is 10.81),
`build/snapshot-*/` (pre-firmware-flash snapshot: primary region, golden bank,
live config), `third-party/firmware/` (five vendor images; 16.53 is the only
one naming SM16269SH), `third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`
(**the file that shipped with the panel — the real ground truth**),
`analysis/record01-fieldmine/` (corpus statistics), `analysis/fpga/`.

Note that `crates/e120-rcvbp/tests/fixtures/p25-128x64-fixed.rcvbp` and the
"consensus donor" are **our own constructions, not vendor ground truth**; two
tests once pinned them as such and were wrong.
