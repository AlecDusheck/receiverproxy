# Provisioning a receiver card

`rxp provision` takes a card from whatever it holds to a working state:
snapshot, firmware, configuration, EEPROM records, verify. It is the only
supported way to configure a card; RAM pushes (`rxp config send`) are for
experiments.

```
rxp provision --spec config/panels/p25-128x64-sm16269s.toml \
    --firmware E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex \
    --position 0,0 --commit
```

| option | meaning | default |
|---|---|---|
| `--spec` | panel spec TOML (`config/panels/*.toml`) | required |
| `--firmware` | the image to install: a name from `config/firmware.toml` (found under `third-party/firmware/` or the config directory's `firmware/` cache, sha256 checked before the write), a path (checked when its file name is in the manifest, otherwise used as is with a warning), or `auto` (below); the firmware step is skipped when absent | none |
| `--position x,y` | this card's origin in the whole screen, in pixels | `0,0` |
| `--index N` | this card's position in the Ethernet chain, the receiver index the EEPROM frames carry; without it they broadcast, and more than one card answering discovery is refused | broadcast |
| `--snapshot-dir` | where the pre-provisioning snapshot goes | `build/snapshot-<time>` |
| `--commit` | write; without it only the plan is printed | off |
| `--wait` | seconds to wait for each reply | 3 |

Output follows the CLI conventions: each step reports on stderr as `[n/5] …`
followed by `firmware:` / `flash:` / `eeprom:` lines; the snapshot paths are
the only stdout output.

## `--firmware auto`

`auto` ranks `config/firmware.toml` for the spec and the card instead of
naming an image, by the rules in [cards.md](cards.md) section 1: the model's
`[[tested]]` entry for this spec, then an image whose chip list names the
spec's driver chip, then a build kind that suits the chip, then version and
build date. It installs only what rule 1 or rule 2 decided; a chip no image
names is refused before anything is written, and the message is the ranking:

```
rxp: provision: no firmware chosen for ICN2053: 108 candidates
  E320_PCB6.0_PWM_FPGA25.50_20221229_invroute_ICND2076.hex 25.50 PWM: PWM suits an S-PWM chip
  ...
```

The plan then names the choice and why:

```
firmware: auto -> E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex (driven on the E120 with this spec; names SM16269SH, the SM16269S family; PWM suits an S-PWM chip)
```

`rxp firmware pick --spec SPEC [--card NAME]` prints the same ranking without
touching a card, and `--commit` is still what writes: without it the plan
stops at the line above. With it, a card already reporting the chosen
version skips the install. `--firmware NAME` and `--firmware PATH` are
unchanged; they install what they name.

## Steps

| step | action | reason |
|---|---|---|
| 1 snapshot | primary bank (blocks 0x00–0x0A) and golden bank (block 0x20) to the snapshot directory | the only copy of what the card held; the recovery path for every later step |
| 2 firmware | compare the bank to the image; if it differs, SDRAM self-program (`firmware install`), then host page writes of any block still differing (`firmware write`), then whole-bank verify; wait for the card to come back reporting the image's version | 16.53 write-protects blocks 0–2 and 8 from the host path, and its self-program path writes only those, so a complete install needs both paths (`memory.guarded` in the model file, [cards.md](cards.md)) |
| 3 EEPROM read | the 256-byte record set via the linear read at 0x7F000 | writing block 7 wipes the EEPROM mirror; the records are rewritten afterwards from this copy |
| 4 config | `config gen` from the spec, then `flash restore-block` of the block-7 image | the whole configuration comes from the TOML, no donor file ([building-a-config.md](building-a-config.md)); `arm_at_boot = true` makes the card configure itself from flash |
| 5 EEPROM write | every record back at its own address and length, to `--index` or broadcast, 500 ms apart; control area `(x, y, x+w, y+h)`; save (opcode 0x87); reload (opcode 0x77); verify by reading back | a write spanning record boundaries is ignored; an index-0 write is ignored while the cabinet record is corrupt, which is why the single-card default broadcasts; back-to-back writes are dropped ([eeprom-map.md](eeprom-map.md), [receiver-identity.md](receiver-identity.md)) |

Power-cycle after the command completes. The card arms from flash and renders
what `rxp show image` / `rxp show video` send.

## Timing constants

| constant | value | where |
|---|---|---|
| SDRAM staging chunk | 1024 bytes, 3 ms apart by default (`--chunk-delay-us 3000`) | `upgrade::CHUNK`, `rxp firmware install` |
| settle after firmware | 12 s after discovery reports the new version; pushes sent earlier are lost | `provision.rs` |
| block-7 write | erase, 3 s settle, 256-byte pages 8 ms apart, verify, repair | `flash restore-block` |
| EEPROM record spacing | 500 ms | `provision.rs`, `scripts/eeprom-restore.py` |

Page 0xF0 of block 7 refuses the write; it is the EEPROM mirror and the
refusal is expected.

Two more expected readings after a firmware install: the whole-bank verify
reports differing bytes confined to block 0x07 between `0x7F000` and
`0x7FFFF`, the parameter tail the card writes for itself, while every other
block verifies exactly; and `rxp discover` can report a nonsense detected
size until `rxp card set-layout` is re-sent.

## Control area and multi-panel walls

The control area is how a card knows its place in the wall. The card keeps
only the pixels whose screen coordinates fall inside
`(startX, startY)–(endX, endY)`, a 42-byte record at EEPROM address 0x02
([receiver-identity.md](receiver-identity.md)). `endX`/`endY` are end
coordinates, not sizes.

* Provision each card with its own `--position x,y`.
* The sender streams the whole screen: rows are screen rows, x offsets are
  screen x. Every card picks its own rectangle.
* The layout file for `rxp show video --layout wall.json` repeats the
  position: each `receivers` entry carries the card's `index`, its `x`,`y`
  (the numbers given to `--position`) and its size; each panel is placed
  inside its receiver by `receiver_x`,`receiver_y`. `rxp card
  layout-example` prints a two-card example.
* Every card on the link shares the vendor MAC pair; the type-0x1900 frame
  addresses one card by its receiver index, big-endian at payload bytes
  1..3, `0xFFFF` for every card ([eeprom-map.md](eeprom-map.md)). The
  index is the card's position in the Ethernet chain, counted from the
  sender: the order the Wall's chain settings (start corner, direction,
  serpentine) define, and the `index` of the layout's `receivers` entry.
* On a chain pass `--index N` per card; the EEPROM writes, the save and the
  reload then carry `N` and the other cards keep their windows. Without
  `--index` the frames broadcast: every card on the chain gets the same
  window, so the command refuses to write when more than one card answers
  discovery (`N cards answered discovery; pass --index`).
* One card on the link needs no `--index`. The broadcast default stays
  because an index-0 write is ignored while the cabinet record is corrupt
  ([receiver-identity.md](receiver-identity.md) section 2); `--index 0` is
  accepted for a card whose record is intact.

## Invariants

A card must not be left with:

* an erased EEPROM control area (`startX = 0xFFFF`): the card reports a
  healthy size to `rxp discover` and drops every pixel; `scripts/flash-review.py`
  checks it;
* a mixed firmware bank: verify all eleven blocks (0x00–0x0A) after any write;
* parameters only in RAM: the 34 `config send` packs are unacknowledged and
  do not all reliably land, and a power-cycle discards them.

## By hand

The same steps as separate commands:

```
rxp flash snapshot --dir build/snapshot-<time>
rxp firmware install <hex> --commit                                    # SDRAM path: blocks 0-2 and 8
rxp firmware write <hex> --backup <snapshot>/primary-region.bin --from-block 3 --to-block 7 --commit
rxp config gen --spec <spec>                                           # writes build/<panel>-block7.bin
rxp flash restore-block build/<panel>-block7.bin --commit
scripts/eeprom-restore.py --commit                                      # records from the factory dump
scripts/flash-review.py <block-7 dump>
```

then power-cycle. `rxp card reload --full` sends the vendor's post-save
reload frame (opcode 0x77) without a power-cycle.

## Limits

* The EEPROM flags at 0x41 and 0x42 (factory value `00`) do not take through
  opcode 0x85 and read back `0xFF`; the panel renders regardless.
* `rxp card screen-size --set` reads and writes all 256 bytes from EEPROM 0
  and refuses a record that reads as erased. It prints end coordinates as a
  size, which is right only while `startX = startY = 0`.
