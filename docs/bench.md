# The bench: how measurements are taken, the meters, their limits

Every claim in [retracted-findings.md](retracted-findings.md) came from a
measurement, not from bad reasoning about a good one. The rig has three
sources of drift and all of them look like signal.

## The rig

* Mac with an AX88179B adapter on `en24`, raw Ethernet through `/dev/bpf`
  (`sudo chmod o+rw /dev/bpf*` after every reboot).
* Colorlight E120, firmware 16.53. `e120 discover` reports the version;
  confirm it, do not assume it.
* One Eager P2.5-O16S-SMD1415-128x64-E module (1/16 duty, SM16269S drivers
  read off the silicon) on hub J1, mounted rotated 90°: it reads 64 wide by
  128 tall on camera, so a vertical stripe in a photo is a constant-x band on
  the panel.
* Korad KA3005P supply at 5 V, 5.1 A current limit, read over USB by
  `ka3005p`.
* A 30 fps webcam on the panel.

Supply rules: read it and power-cycle it, never change its voltage or current
settings. `scripts/psu.sh` arms an automatic off (default and maximum 10
minutes) on every power-on; `psu.sh extend` pushes it out. An armed panel
showing unmodulated content draws ~4.5 A and at full brightness rails the
limit and browns out: keep brightness ≤ 40 until content is right, and never
flash while `ka3005p status` shows `CH1: Cc`.

Vendor software is inspected, never executed.

## The three traps

1. The supply drifts over tens of seconds, and a reading taken right after a
   stream starts runs high, often by more than an amp.
2. The card has a per-run state toggle. Identical content sent twice has
   alternated the current 3.14 → 4.57 → 3.14 → 4.60 A. The content did not
   change; the run did.
3. The camera auto-exposes and auto-gains. Absolute brightness is not
   comparable between two shots, and at normal panel brightness every LED
   clips to white.

A difference between two conditions measured at different times means
nothing. That single mistake produced two false breakthroughs.

## The rule

Put the conditions in one run so drift is common to all of them, and judge any
gap against a same-content control. `scripts/bench.py run` does this: the
conditions become segments of one looping stream, each is metered and
photographed mid-segment, and the first condition is repeated at the end as
the control:

```
scripts/bench.py run --spec config/panels/p25-128x64-sm16269s.toml --brightness 20 black white
```

A gap is not a finding unless it clearly exceeds what the control shows
against itself. Repeat the run before believing one. The within-condition
spread on this rig is about 0.033 A.

## Run the idle test first

Before attributing anything on the panel to what you sent, confirm the panel
is static when you send nothing:

```
pkill -f 'e120 --brightness'
# three photos, five seconds apart, same crop
```

Mean absolute difference should be 1–2 levels (camera noise). 20–40 means the
card is rendering a buffer nothing is driving and every experiment is
measuring drift. That was the state on firmware 10.81 (29–37 levels; 1.6–1.8
on 16.53).

## Include a positive control

When testing whether the panel resolves position along an axis, include an
axis already known to work: `bench.py run left right top bottom` pairs a
left/right split with a top/bottom one. If the control does not register, the
measurement is too insensitive to trust the result you care about.

## Prefer discriminators that exposure cannot fake

The strongest single test is an all-black frame. A correct path makes the
panel go dark, and dark survives auto-exposure, auto-gain and reflections in a
way "looks like the right pattern" does not. A panel that stays lit under
all-black is conclusive regardless of photographic conditions.

| instrument | good for | fails at |
|---|---|---|
| all-black frame | is the pixel path connected at all | little; use it first |
| `bench.py run` current | quantitative, unattended | needs interleaving; blind to spatial structure |
| photo structure | geometry, mapping | exposure, gain, reflections, rotation |
| single current reading | nothing | everything |

## Meters and their limits

| meter | reads | limit |
|---|---|---|
| KA3005P current | total panel + card draw; black 0.466 A, boot 0.41 A, white 2.64 A at the bench brightness | drifts over tens of seconds; the first seconds of a stream run high; 0.033 A within-condition spread; blind to where on the panel the current goes |
| averaged webcam capture | structure, geometry, mapping; clip fraction; correlation between captures | the panel multiplexes 1/16, so a single 1/30 s frame is scan phase, not content; every photo before 2026-09-01 was one frame. `bench.py capture` primes a 90-frame average; keep it that way. Auto-exposure clips LEDs to white above brightness ~20; auto-gain boosts a dark panel; reflections off nearby surfaces enter even a difference image |
| `bench.py flicker / bands / glitch` | per-frame brightness series, rolling-shutter band period, band events | 30 fps cannot resolve the panel's flicker: 2.4 % frame to frame against an 8–14 % camera reference. Flicker is judged by eye |
| `e120 discover` | firmware version, control area end coordinates | reports a healthy size while the control area is erased (`startX = 0xFFFF`); check the EEPROM with `scripts/flash-review.py` |

Camera handling that avoids false structure: shoot at brightness 6–20; locate
the panel by differencing lit against blanked (`bench.py locate`), taking the
brightest connected region rather than everything above a threshold; compare
structure or same-condition differences, never absolute brightness.

## Tools

Everything is in `scripts/bench.py` because every experiment has the same
shape and every past mistake was in one of its steps.

| command | what it does |
|---|---|
| `bench.py power on\|off\|cycle\|status` | supply with the dead-man timer (`psu.sh` underneath); `cycle` waits for discovery |
| `bench.py boot --spec S` | power-cycle, wait for the card, push the spec's packs, report the armed current |
| `bench.py locate` | find the panel in frame by differencing lit against blanked; remembers the crop |
| `bench.py capture NAME` | 90-frame primed average, cropped, clip fraction reported |
| `bench.py compare A B…` | structure correlation of captures against A |
| `bench.py tile NAME…` | side-by-side strip of captures |
| `bench.py run [--boot] --spec S label=pattern[@bright] …` | the experiment: one looping stream through `e120 show video`, captured mid-segment, current, mean, clip fraction and correlation per condition, same-content control at the end. `--restart` streams each condition through `e120 show image --hold` instead, which is what allows a different brightness per condition; it reintroduces the per-restart toggle, so use it only when needed |
| `bench.py flicker\|bands\|glitch NAME` | the flicker probes above |

Built-in patterns: `black white red green blue top bottom left right rgbrows
hbands vbands gray-N row-N col-N`, or any PNG path. Use `--boot` before any
configuration change so the card starts from a known state:

```
scripts/bench.py run --boot --spec config/panels/p25-128x64-sm16269s.toml \
    --brightness 40 black white top left
```

Config-side tools stay separate and read-only: `flash-review.py` (diff block 7
against the day-one dump, check the control area; run after every flash
operation), `eeprom-restore.py` (rewrite records from the day-one dump, one
at a time, `--commit`), `mapdump.py`, `mapstruct.py`, `chipregs.py`.

## Procedures worth not rediscovering

* Flashing firmware erases the parameter block; rewrite the config after
  (`e120 flash restore-block build/<panel>-block7.bin --commit`, then
  `scripts/eeprom-restore.py --commit`, then power-cycle), or use
  `e120 provision`.
* `e120 flash snapshot` first. It captures the primary region and the golden
  bank; the golden bank at block 0x20 is never written by `firmware write`.
* `e120 config send` pushes to RAM only and lands on about one boot in three.
  The mapping is read from flash at boot, so a mapping change needs
  `flash restore-block` plus a power-cycle.
* EEPROM records must be written one at a time, at the record's own address
  and length from [eeprom-map.md](eeprom-map.md); the card silently ignores a
  write spanning record boundaries. Records `0x041`, `0x042` and `0x092` did
  not take through opcode `0x85` and are still `0xFF`.
* `e120 card screen-size --set` refuses a record that reads as erased; writing
  one back is what persisted `0xFF` across the control area once.
