# Provisioning a receiver card

One command takes a card from whatever it holds to a working state:

```
e120 provision --spec config/panels/p25-128x64-sm16269s.toml \
    --firmware third-party/firmware/E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex \
    --position 0,0 --commit
```

Without `--commit` it prints the plan. Steps, and why each is the way it is:

| step | what | why |
|---|---|---|
| 1 snapshot | primary bank + golden bank to `build/snapshot-<time>/` | the only copy of what the card held; the recovery path for everything below |
| 2 firmware | compare the bank to the image; if it differs, SDRAM self-program (`upgrade install`) **then** host writes of any block still differing, then whole-bank verify; wait for the card to come back reporting the image's version | 16.53 write-protects blocks 0–2 and 8 from the host path and its self-program writes only those, so a complete install needs both ([rendering-recipe.md](rendering-recipe.md)) |
| 3 EEPROM read | the 256-byte record via the linear read | writing block 7 wipes the EEPROM mirror; the records must come back afterwards |
| 4 config | `gen-config` from the spec, `restore-flash` the block-7 image | the whole configuration, from TOML, no donor file ([building-a-config.md](building-a-config.md)); `arm_at_boot = true` so the card configures itself from flash — RAM pushes are unreliable |
| 5 EEPROM write | every record back at its own address and length, broadcast index, 500 ms apart; control area = `(x, y, x+w, y+h)`; save (0x87); reload (0x77); verify by reading back | records written across boundaries are ignored; index-0 writes are ignored while the cabinet record is corrupt; back-to-back writes are dropped ([eeprom-map.md](eeprom-map.md), [receiver-identity.md](receiver-identity.md)) |

Then power-cycle. The card arms from flash and renders whatever `e120 image` /
`e120 play` send. Verified on the bench: after provisioning and a power-cycle,
black 0.47 A (LEDs off), white full, every pattern intact.

## More than one panel

The **control area** is how a card knows its place in the wall: it keeps only
the pixels whose screen coordinates fall inside `(startX, startY)–(endX, endY)`.
Provision each card with its own `--position x,y`; the sender then streams the
whole screen (rows are screen rows, x offsets are screen x) and every card
picks its own rectangle. `e120 play --layout wall.json` describes the wall
(`e120 layout-example` prints one). Cards are addressed by MAC, so wire one at
a time while provisioning, or provision on a bench link.

## What a card must not be left with

* an erased EEPROM control area (`startX = 0xFFFF`) — it will report a healthy
  size and drop every pixel; `scripts/flash-review.py` checks it;
* a mixed firmware bank — always verify all eleven blocks after any write;
* parameters only in RAM.
