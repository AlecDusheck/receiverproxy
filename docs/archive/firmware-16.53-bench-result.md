# Installing firmware 16.53: what it fixed, and what it did not

> Archived. Superseded by [rendering.md](../rendering.md) ("Firmware 16.53") for the install procedure and what changed, and by [retracted-findings.md](../retracted-findings.md) for the free-running finding. The "raster still wrong" section describes the state before `+0x02F = 1`, the frame order and booting from flash were found.

Bench, 2026-09-01. The card had been running
`E320_PCB6.0_PWM_FPGA10.81_20230907` — the factory image — even though the
project's notes recorded it as 16.53. `docs/fpga/flash-layout.md` established
that from the dumps; `discover` now confirms it directly, since the card
reports its own firmware version and that report changed from the flash.

`E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex` is the only image in
`third-party/firmware/` whose name carries a driver-chip list, and it names
**SM16269SH** — the family on this module.

## Installing it

```
e120 firmware write third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex \
     --backup <snapshot>/primary-region.bin --commit
```

Take `e120 flash snapshot` first; it captures the primary region and the golden bank,
and the golden bank at block 0x20 is never touched by this command.

Two things to expect:

* **The verify step reports ~4042 differing bytes and this is not a failure.**
  Every one of them lies in block 0x07 between `0x7F000` and `0x7FFFF`, the
  parameter tail the card writes for itself. Blocks 0x00–0x06 and 0x08–0x0A
  verify exactly. Dump the region back and bucket the differences by block
  before concluding anything from the warning.
* **Flashing firmware erases the parameter block.** Block 0x07 comes from the
  vendor image, so the config must be rewritten afterwards:
  `e120 flash restore-block build/<panel>-block7.bin --commit`, then power-cycle.

After the power-cycle `discover` reports `firmware=16.53`.

## What it fixed: the panel stopped free-running — HIGH

On 10.81 the panel changed **with no network traffic at all**. Three photos
taken five seconds apart, all streamers killed, differed by a mean absolute
29–37 levels (of 255), with mean brightness wandering 226 → 200 → 235.

On 16.53 the same measurement gives **1.6–1.8**, which is camera noise, and
identical mean brightness (189/189/189).

That mutating garbage had been read as "our data arriving scrambled" and it was
nothing of the sort — the card was rendering a buffer nothing was driving. Any
experiment run against it was measuring drift. This is why several earlier
content-dependence findings did not replicate.

The card's built-in test generator also came alive: on 10.81 all nine selectors
gave flat current and indistinguishable output, and on 16.53 the selectors
produce visibly different displays.

## What it did not fix: the raster is still wrong — HIGH

The panel still does not show sent content, and the current draw is now
*exactly* content-independent — interleaved and repeated (`scripts/bench.py run`,
3 reps), all-black and all-white differ by 0.001 A against a within-condition
spread of 0.033 A.

The decisive observation is that **the card's own test patterns render as
garbage too**. Those are generated inside the card and never touch the host, so
the fault is at or below the card's raster stage: how the card drives the hub,
not how we deliver pixels. Selectors 2 and 3 come out near-uniform white and
1, 4, 5, 6 as structured colour noise; none is the clean solid field a test
pattern should be.

This narrows the remaining fault to the panel-driving parameters — scan
addressing, the chip protocol and its timing — and rules out, for now, the
host-side raster layout, the row-base/screen-number field, and pixel ingest
generally. Those cannot be the cause of a defect that reproduces with the host
disconnected from the picture path.

## Measurement note

Do not trust a single supply reading on this bench, on either firmware. The
supply drifts over tens of seconds and readings taken right after a stream
starts run high. Compare conditions **interleaved and repeated**, and judge a
difference against the within-condition spread — `scripts/bench.py run` does
this and prints the verdict. Two false breakthroughs in this project came from
comparing one condition measured now against another measured a minute ago.
