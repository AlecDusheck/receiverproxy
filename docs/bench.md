# The bench: measurement method, meters and limits

How a measurement on this rig is taken, what each meter reads, where each
meter fails, and the `scripts/bench.py` commands that implement the method.
Claims disproved by this method are in
[retracted-findings.md](retracted-findings.md).

## Rig

| part | detail |
|---|---|
| host | Mac with an AX88179B adapter on `en24`, raw Ethernet through `/dev/bpf` (`sudo chmod o+rw /dev/bpf*` after every reboot) |
| card | Colorlight E120, firmware 16.53. `rxp discover` reports the running version |
| panel | one Eager P2.5-O16S-SMD1415-128x64-E module, 1/16 duty, SM16269S drivers (read off the silicon), on hub J1. Mounted rotated 90 degrees: 64 wide by 128 tall on camera, so a vertical stripe in a photo is a constant-x band on the panel |
| supply | Korad KA3005P at 5 V, 5.1 A current limit, read over USB by `ka3005p` |
| camera | 30 fps webcam on the panel, 1920x1080, avfoundation device `RXP_CAMERA` (default `0`) |

`bench.py` keeps its state in `/tmp/rxp-trials/`: captures (`cap-NAME.jpg`,
`cap-NAME.rgb`), per-LED crops (`hi-NAME.png`), generated patterns (`pat/`)
and the panel crop (`crop.txt`; `RXP_CROP` overrides it, built-in default
`420:750:1060:250`).

Supply rules: the supply is read and power-cycled, never re-set. `scripts/psu.sh`
only toggles the output and arms an automatic off on every power-on (default
and maximum 10 minutes); `psu.sh extend` restarts the timer without a power
cycle; `psu.sh status` prints the reading and the time left. An armed panel
showing unmodulated content draws about 4.5 A; at full brightness it rails the
5.1 A limit and browns out. Brightness stays at or below 40 until content is
right. Never flash while `ka3005p status` shows `CH1: Cc`.

Vendor software is inspected, never executed.

## Drift sources

1. The supply drifts over tens of seconds. A reading taken right after a
   stream starts runs high, often by more than an amp.
2. The card has a per-run state toggle. Measured: identical content sent
   twice gave 3.14, 4.57, 3.14, 4.60 A.
3. The camera auto-exposes and auto-gains. Absolute brightness is not
   comparable between two shots, and at normal panel brightness every LED
   clips to white.

A difference between two conditions measured at different times is not a
result.

## Method

### One run, one control

All conditions go into one run so drift is common to them, and any gap is
judged against a same-content control. `scripts/bench.py run` does this: the
conditions become segments of one looping stream through `rxp show video
--loop`, each segment is metered and photographed mid-segment, and the first
condition is repeated at the end as the control.

```
scripts/bench.py run --spec config/panels/p25-128x64-sm16269s.toml --brightness 20 black white
```

A gap is a finding only when it clearly exceeds what the control shows
against itself, and only after the run is repeated. The within-condition
spread on this rig is 0.033 A.

### Idle test

Before attributing anything on the panel to what was sent, confirm the panel
is static when nothing is sent:

```
pkill -f 'rxp --brightness'
# three photos, five seconds apart, same crop
```

Mean absolute difference of 1 to 2 levels is camera noise. 20 to 40 levels
means the card is rendering a buffer nothing is driving and every experiment
measures drift. Measured: 29 to 37 levels on firmware 10.81; 1.6 to 1.8 on
16.53.

### Positive control

A test of whether the panel resolves position along an axis includes an axis
already known to work: `bench.py run left right top bottom` pairs a left/right
split with a top/bottom one. If the control does not register, the
measurement is too insensitive to trust.

### Discriminators exposure cannot fake

The strongest single test is an all-black frame. A correct path makes the
panel go dark, and dark survives auto-exposure, auto-gain and reflections. A
panel that stays lit under all-black is conclusive regardless of photographic
conditions.

| instrument | good for | fails at |
|---|---|---|
| all-black frame | whether the pixel path is connected at all | little; use it first |
| `bench.py run` current | quantitative, unattended | needs interleaving; blind to spatial structure |
| photo structure | geometry, mapping | exposure, gain, reflections, rotation |
| single current reading | nothing | everything |

## Meters and limits

| meter | reads | limit |
|---|---|---|
| KA3005P current | total panel plus card draw. Measured at the bench brightness, configured from flash: black 0.466 A, boot 0.41 A, white 2.64 A | drifts over tens of seconds; the first seconds of a stream run high; 0.033 A within-condition spread; blind to where on the panel the current goes |
| averaged webcam capture | structure, geometry, mapping; clip fraction; correlation between captures | the panel multiplexes 1/16, so a single 1/30 s frame is scan phase, not content. `bench.py capture` averages a primed 90-frame window (`tmix=frames=N,select='gte(n,N)'`). Auto-exposure clips LEDs to white above brightness about 20; auto-gain boosts a dark panel; reflections off nearby surfaces enter even a difference image |
| `bench.py flicker`, `bands`, `glitch` | per-frame brightness series, rolling-shutter band period, band events | 30 fps cannot resolve the panel's flicker: 2.4 % frame to frame against an 8 to 14 % camera reference. Flicker is judged by eye |
| `rxp discover` | firmware version, control area end coordinates | reports a healthy size while the control area is erased (`startX = 0xFFFF`); the EEPROM is checked with `scripts/flash-review.py` |

Camera handling: shoot at brightness 6 to 20; locate the panel by differencing
lit against blanked (`bench.py locate`), taking the brightest connected region
rather than everything above a threshold; compare structure or same-condition
differences, never absolute brightness. `capture` counts a pixel as clipped at
level 250 or above and warns when the clipped fraction exceeds 3 %.

## `scripts/bench.py` commands

| command | what it does |
|---|---|
| `bench.py power on\|off\|cycle\|status [--minutes N]` | supply through `psu.sh` with the dead-man timer (default 10, maximum 10); `cycle` waits for discovery |
| `bench.py boot --spec S` | kill streamers, power-cycle (off, 4 s, on), wait for discovery (25 s timeout), settle 12 s (discovery answers before boot parameters have loaded; packs pushed earlier are lost), `rxp config send --spec S`, report the armed current |
| `bench.py locate` | find the panel in frame by differencing lit against blanked (30 frames each); saves the crop |
| `bench.py capture NAME [--frames 90]` | primed averaged still, cropped, sampled 64x128; prints mean, clipped fraction, outlier count |
| `bench.py compare A B ...` | structure correlation of captures against A |
| `bench.py tile NAME ... [--out PATH]` | side-by-side strip of captures |
| `bench.py run [--boot] --spec S [--brightness 40] [--frames 60] [--segment 8] label=pattern[@bright] ...` | each condition becomes a `--segment`-second 30 fps clip; the clips and the `-ctl` control are concatenated and streamed once with `rxp --brightness B show video run.mp4 --loop --fps 30`; current read and capture taken 30 % into each segment; prints current, mean, clip fraction and correlation per condition, same-content control at the end. `--restart` streams each condition through `rxp show image --hold` instead, which allows a different brightness per condition and reintroduces the per-restart toggle |
| `bench.py flicker NAME [--seconds 3]` | per-frame brightness series |
| `bench.py bands NAME` | rolling-shutter band period of one frame |
| `bench.py glitch NAME [--seconds 4]` | frames whose row profile departs from the median |

Built-in patterns: `black white red green blue top bottom left right rgbrows
hbands vbands gray-N row-N col-N`, or any PNG path. `--boot` before any
configuration change starts the card from a known state:

```
scripts/bench.py run --boot --spec config/panels/p25-128x64-sm16269s.toml \
    --brightness 40 black white top left
```

Config-side scripts are separate and read-only unless `--commit` is given:
`flash-review.py` (diff block 7 against the factory dump and check the
control area; run after every flash operation), `eeprom-restore.py` (rewrite
records from the factory dump, one at a time, `--commit`), `mapdump.py`,
`mapstruct.py`, `chipregs.py`.

## Flash and configuration rules

* Flashing firmware erases the parameter block (block 0x07). The config is
  rewritten afterwards: `rxp flash restore-block build/<panel>-block7.bin
  --commit`, then `scripts/eeprom-restore.py --commit`, then a power-cycle;
  `rxp provision` does all of it.
* `rxp flash snapshot` first. It captures the primary region and the golden
  bank; the golden bank at block 0x20 is never written by `firmware write`.
* `rxp config send` pushes to RAM only and lands on about one boot in three.
  The mapping is read from flash at boot, so a mapping change needs
  `flash restore-block` plus a power-cycle.
* EEPROM records are written one at a time, at the record's own address and
  length from [eeprom-map.md](eeprom-map.md); the card silently ignores a
  write spanning record boundaries. Records `0x041`, `0x042` and `0x092` did
  not take through opcode `0x85` and remain `0xFF`.
* `rxp card screen-size --set` refuses a record that reads as erased. Writing
  an erased record back is what persisted `0xFF` across the control area.
