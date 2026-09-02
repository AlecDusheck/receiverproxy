# scripts

## Bench harness

| script | what it does |
|---|---|
| `bench.py` | power, boot a spec, stream conditions, averaged camera captures, flicker/bands/glitch meters |
| `psu.sh` | ka3005p supply on/off/status behind a 10-minute dead-man timer |
| `mirror.sh` | pipe a screen capture into `e120 show stream` |

## Analysis

| script | what it does |
|---|---|
| `flash-review.py` | diff a block-7 dump against the day-one dump run by run; checks the EEPROM control area |
| `flash_review_map.py` | EEPROM record map shared by flash-review.py and eeprom-restore.py |
| `eeprom-restore.py` | rewrite EEPROM records from the day-one dump, one record at a time (dry run without `--commit`) |
| `mapdump.py` | print a .rcvbp pixel map (record 0x0a03) entry by entry |
| `mapstruct.py` | collapse a .rcvbp pixel map into monotonic runs |
| `chipregs.py` | decode and compare driver-chip register tables (record 0x0a84) |
| `corpus-mine.py` | mine the vendor .rcvbp corpus into `config/chips/mined` and `config/panels/mined` |

## Bench workflow

1. `scripts/psu.sh on` powers the panel with the auto-off timer armed.
2. `scripts/bench.py boot --spec config/panels/NAME.toml` power-cycles, waits for discovery and pushes the spec.
3. `scripts/bench.py run --spec config/panels/NAME.toml label=pattern[@brightness] ...` streams the conditions and the same-content control.
4. `scripts/bench.py capture NAME` takes one averaged, cropped still; `locate` first on a new rig.
5. `scripts/bench.py flicker|bands|glitch NAME` reads the per-frame series, rolling-shutter bands and band events.
