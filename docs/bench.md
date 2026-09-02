# The bench: measurement method, meters and limits

How a measurement is taken when bringing up a card, what each meter reads,
where each meter fails, and the `scripts/bench.py` commands that implement
the method.

## Rig

A rig needs a host with a raw-Ethernet path to the card, a current-reading
bench supply, and a camera on the panel.

| part | requirement |
|---|---|
| host | raw Ethernet to the card: `/dev/bpf` on macOS (`sudo chmod o+rw /dev/bpf*` after every reboot), `AF_PACKET` on Linux. `--iface` names the adapter |
| card | its firmware version read with `rxp discover`; the card is the only authority for it |
| panel | one module on a hub port. Record how it is mounted: a panel rotated 90 degrees turns a vertical stripe in a photo into a constant-x band on the panel |
| supply | a supply whose current can be read and logged over USB (`ka3005p` drives a Korad KA3005P) and whose output can be toggled |
| camera | a webcam on the panel; `avfoundation` device index in `RXP_CAMERA` (default `0`) |

`bench.py` keeps its state in `/tmp/rxp-trials/`: captures (`cap-NAME.jpg`,
`cap-NAME.rgb`), per-LED crops (`hi-NAME.png`), generated patterns (`pat/`)
and the panel crop (`crop.txt`; `RXP_CROP` overrides it).

Supply rules: the supply is read and power-cycled, never re-set.
`scripts/psu.sh` only toggles the output and arms an automatic off on every
power-on (default and maximum 10 minutes); `psu.sh extend` restarts the timer
without a power cycle; `psu.sh status` prints the reading and the time left.
Brightness stays at or below 40 until content is right: an armed panel
showing unmodulated content draws several amps and at full brightness will
rail a small supply's current limit and brown the card out. Never flash while
the supply reads constant-current.

Vendor software is inspected, never executed.

## Drift sources

1. The supply drifts over tens of seconds. A reading taken right after a
   stream starts runs high, often by more than an amp.
2. The card has a per-run state toggle: identical content sent twice can
   differ by more than an amp between the two runs.
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
scripts/bench.py run --spec config/panels/<panel>.toml --brightness 20 black white
```

A gap is a finding only when it clearly exceeds what the control shows
against itself, and only after the run is repeated. Establish the rig's own
within-condition spread from the control before reading anything into a
difference.

### Idle test

Before attributing anything on the panel to what was sent, confirm the panel
is static when nothing is sent:

```
pkill -f 'rxp --brightness'
# three photos, five seconds apart, same crop
```

A mean absolute difference of a level or two between the photos is camera
noise. Tens of levels means the card is rendering a buffer nothing is
driving, and every experiment on it measures drift rather than content. Some
firmware builds free-run this way; re-run the idle test after any firmware
change before trusting a measurement.

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
| supply current | total panel plus card draw | drifts over tens of seconds; the first seconds of a stream run high; blind to where on the panel the current goes |
| averaged webcam capture | structure, geometry, mapping; clip fraction; correlation between captures | a multiplexed panel shows scan phase, not content, in a single 1/30 s frame. `bench.py capture` averages a primed 90-frame window (`tmix=frames=N,select='gte(n,N)'`); an unprimed average is a single frame and reads as scrambled content. Auto-exposure clips LEDs to white above brightness about 20; auto-gain boosts a dark panel; reflections off nearby surfaces enter even a difference image |
| `bench.py flicker`, `bands`, `glitch` | per-frame brightness series, rolling-shutter band period, band events | a 30 fps camera cannot resolve panel flicker against its own frame-to-frame reference; flicker is judged by eye |
| `rxp discover` | firmware version, control area end coordinates | reports a healthy size while the control area is erased (`startX = 0xFFFF`); the EEPROM is checked with `scripts/flash-review.py` |

Camera handling: shoot at brightness 6 to 20; locate the panel by differencing
lit against blanked (`bench.py locate`), taking the brightest connected region
rather than everything above a threshold, which also finds windows and other
reflectors; compare structure or same-condition differences, never absolute
brightness. `capture` counts a pixel as clipped at level 250 or above and
warns when the clipped fraction exceeds 3 %.

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
scripts/bench.py run --boot --spec config/panels/<panel>.toml \
    --brightness 40 black white top left
```

Config-side scripts are separate and read-only unless `--commit` is given:
`flash-review.py` (diff block 7 against a reference dump and check the
control area; run after every flash operation), `eeprom-restore.py` (rewrite
records from a dump, one at a time, `--commit`), `mapdump.py`,
`mapstruct.py`, `chipregs.py`.

## Flash and configuration rules

* Flashing firmware erases the parameter block (block 0x07). The config is
  rewritten afterwards: `rxp flash restore-block build/<panel>-block7.bin
  --commit`, then `scripts/eeprom-restore.py --commit`, then a power-cycle;
  `rxp provision` does all of it.
* `rxp flash snapshot` first. It captures the primary region and the golden
  bank; the golden bank at block 0x20 is never written by `firmware write`.
* `rxp config send` pushes to RAM only and does not reliably land: the 34
  packs are unacknowledged. The mapping is read from flash at boot, so a
  mapping change needs `flash restore-block` plus a power-cycle.
* EEPROM records are written one at a time, at the record's own address and
  length from [eeprom-map.md](eeprom-map.md); the card silently ignores a
  write spanning record boundaries.
* `rxp card screen-size --set` refuses a record that reads as erased.
  Writing an erased record back persists `0xFF` across the control area and
  the card then drops every pixel ([receiver-identity.md](receiver-identity.md)).
