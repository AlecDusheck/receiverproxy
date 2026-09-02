# Measuring anything on this bench

Every false conclusion in [retracted-findings.md](retracted-findings.md) came
from a measurement, not from bad reasoning about a good measurement. This rig
has three independent sources of drift, and all of them look like signal.

## The three traps

1. **The supply drifts** over tens of seconds, and a reading taken right after
   a stream starts runs high — often by more than an amp.
2. **The card has a per-run state toggle.** Sending *identical* content twice
   has alternated the current 3.14 → 4.57 → 3.14 → 4.60. The content did not
   change; the run did.
3. **The camera auto-exposes and auto-gains.** Absolute brightness is not
   comparable between two shots, and at normal panel brightness every LED
   clips to white.

Consequence: **a difference between two conditions measured at different times
means nothing.** That single mistake produced two "breakthroughs", both wrong.

## The rule

Put the conditions in one run so drift is common to all of them, and judge any
gap against a same-content control. `scripts/bench.py run` does this: the
conditions become segments of one looping stream, each is metered and
photographed mid-segment, and the first condition is repeated at the end as the
control:

```
scripts/bench.py run --brightness 20 black white
```

A gap between conditions is not a finding unless it clearly exceeds what the
control shows against itself. Repeat the run before believing one; the measured
within-condition spread on this rig is ~0.033 A.

## Always run the idle test first

Before attributing anything on the panel to what you sent, confirm the panel is
static when you send nothing:

```
pkill -f 'e120 --brightness'
# three photos, five seconds apart, same crop
```

Mean absolute difference should be **1–2 levels** (camera noise). If it is
20–40, the card is rendering a buffer nothing is driving and every experiment
you run is measuring drift. That was the state on firmware 10.81.

## Include a positive control

When testing whether the panel resolves position along an axis, include an axis
you already know works — `bench.py run left right top bottom` pairs a left/right
split with a top/bottom one. If the control does not register, the measurement
is too insensitive to trust the result you care about, and you have learned that
instead of a false negative.

## Prefer discriminators that cannot be faked by exposure

The strongest single test found in this project is **send all-black**. A
correct path makes the panel go dark, and "dark" survives auto-exposure,
auto-gain and reflections in a way "looks like the right pattern" does not. A
panel that stays lit under all-black is conclusive regardless of photographic
conditions.

Ranked by how much they resist the traps above:

| instrument | good for | fails at |
|---|---|---|
| all-black frame | is the pixel path connected at all | nothing much — use this first |
| `bench.py run` current | quantitative, unattended | needs interleaving; blind to spatial structure |
| photo structure | geometry, mapping | exposure, gain, reflections, rotation |
| single current reading | nothing | everything |

## Tools

Everything is in one tool, `scripts/bench.py`, because every experiment has the
same shape and every past mistake was in one of its steps.

| command | what it does |
|---|---|
| `bench.py power on\|off\|cycle\|status` | supply with the dead-man timer (`psu.sh` underneath); `cycle` waits for discovery |
| `bench.py boot --spec S` | power-cycle, wait for the card, push the spec's packs, report the armed current |
| `bench.py locate` | find the panel in frame by differencing lit against blanked; remembers the crop |
| `bench.py capture NAME` | 90-frame primed average, cropped, clip fraction reported |
| `bench.py compare A B…` | structure correlation of captures against A |
| `bench.py run [--boot --spec S] label=pattern[@bright] …` | the experiment, see below |

`run` is the one to reach for. It shows each condition and prints supply
current, panel mean, clip fraction and correlation against the first
condition, then a tile. Two properties are built in and should not be turned
off without a reason:

* **the same-content control** — the first condition is repeated at the end;
  a difference between conditions only counts if it clears that number;
* **one continuous stream** — the conditions become segments of a looping
  video played by `e120 play`, captured mid-segment. Nothing restarts between
  conditions, so the card's per-restart state toggle cannot masquerade as a
  result. `--restart` and `--stream-flags` are experiment-only, for conditions
  that need different `image` flags.

Built-in patterns: `black white red green blue top bottom left right rgbrows
hbands vbands gray-N row-N col-N`, or any PNG path.

`bench.py flicker|bands|glitch` are experiment-only flicker probes (per-frame
brightness series, rolling-shutter band period, band events); the 30 fps camera
cannot resolve the panel's flicker ([rendering-recipe.md](rendering-recipe.md)).

```
scripts/bench.py run --boot --spec config/panels/p25-128x64-sm16269s.toml \
    --brightness 40 black white top left
```

Config-side tools stay separate: `flash-review.py`, `eeprom-restore.py`,
`mapdump.py`, `mapstruct.py`, `chipregs.py`.
