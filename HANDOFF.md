# Handoff: getting the panel to display

Updated after the 2026-09-01 session. The frame protocol is now correct and
verified, the configuration story is fully decoded, and the remaining fault is
isolated **below the raster**: even the card's own test-pattern generator
renders noise on this panel, on two different firmware builds. The prime
suspect is the driver-chip identity/registers; the user was asked to read the
IC markings off the module.

Read [`firmware/README.md`](firmware/README.md),
[`docs/config-protocol.md`](docs/config-protocol.md), and
[`docs/compiled-image-format.md`](docs/compiled-image-format.md) alongside this.

---

## 1. The rig

| Thing | Detail |
|---|---|
| Host | Mac, USB-Ethernet **AX88179B on `en24`**, raw layer-2 via `/dev/bpf` |
| Card | Colorlight **E120**, id=0x64, now running **firmware 10.81** (factory build, downgraded back from 16.53 this session — neither renders) |
| Panel | One **P2.5, 128x64**, config says SM16269S, module spec says 1/16 duty, 14-bit gray (`docs/P2.5_Outdoor_SMD1415_320x160mm_Module_Specification.pdf`, model P2.5-O16S-SMD1415-128x64-E). **Actual IC marking unconfirmed — this is the open question.** On hub J1 only |
| PSU | KA3005P at 5 V, limit 5.1 A — read/power-cycle only, never change settings |
| Camera | Front webcam. `scripts/snap-avg.sh` for stills, **`scripts/strobe.sh` for anything temporal** — averaged stills hide strobing entirely |

`sudo chmod o+rw /dev/bpf*` after every Mac reboot.

**Keep brightness low (≤40) while the panel is in the unmodulated all-on
state** — at full it rails the 5.1 A limit and sags the supply to 4.6 V (it
did this session, during an FPGA reprogram: CC state, results invalid).

Vendor SDK / ChipData inspection goes to an **Opus 5 subagent**
([`docs/vendor-sdk-analysis.md`](docs/vendor-sdk-analysis.md)). Never execute
vendor software.

---

## 2. Solved this session (do not re-litigate)

1. **The wire frames were off by one byte, and are now fixed and committed.**
   FPP's real layout (`CL_PACKET_DATA_OFFSET 13`) puts the first data byte in
   the second EtherType byte: sync = `[0x01,0x07,...]` with brightness at
   frame offset 35, brightness = `[0x0a,b,b,b,0xff]`, pixel row =
   `[0x55,rowMSB,rowLSB,...]`, 497 px/packet. The previous "FPP layout" in
   the tree was the shifted variant; the card **misparsed the shifted sync
   frame and degraded into a metronomic ~5 Hz strobe** (persists after
   streaming stops; cleared by power cycle). With correct frames: no strobe,
   and content/brightness move the panel current. The shifted variant is kept
   as `--pixel-layout shifted` for A/B only.

2. **The compiled parameter image at flash 0x70000 is fully decoded.**
   Spec: `docs/compiled-image-format.md`; generator: `scripts/compile_rcvbp.py`
   (round-trips the factory block byte-exact). It is a fixed-offset scatter of
   pack *bodies* (no framing/checksums). Page 0 = the vendor's fully-computed
   basic pack; extracted ready-to-send packs live in `firmware/derived/`.

3. **The card's flash config was ALWAYS correct for this panel.** The stored
   .rcvbp is record-identical to `firmware/P2.5-32S-128X64-SM16269S-256X384I.rcvbp`.
   Do not synthesize/flash a "fixed" compiled block — it reproduces the same
   bytes. Page 0x09 (chip-register pack) is erased in the factory image, which
   is why drivers never arm at boot; arming via RAM chip pack works.

4. **The save-to-flash path is fully resolved** (vendor-SDK agent, high
   confidence): erase block 7 (op 0x23, flag 0, 3 s settle) → 256-byte page
   writes (op 0x85) of the 14 regions → **reload-from-flash frame: type 0x0600,
   opcode 0x77, payload[0x0a..0x0e] = 01 01 01 00 00** (no power cycle needed).
   The host always writes the compiled image itself; there is no card-side
   recompile. Sub-index correction: **basic pack `[3]=0x00`, data-swap
   `[3]=0x02`, chip `[3]=0x01`** — §21.2's `[3]=2-for-basic` was wrong, which
   is what broke arming when tried.

5. **The vendor real-time pack trio sends cleanly** (chip → data-swap →
   basic, 5 ms apart): payload hexes in `firmware/derived/` (`raw-send --type
   0500 --payload "$(cat …hex)" --pad 258`). It arms and runs but does not fix
   rendering.

6. **EEPROM screen record** (0x7F000): was blank (all FF); `screen-size --set
   128x64 --commit` persists across power cycles — but the 1024x512 boot
   geometry **does not come from it** (still reported with the record set).
   It's a gateware default. `set-layout` per session remains required.

7. **Test-mode selector**: effectively 5-bit (selector mod 32; 20/52 alias).
   Full 256-value sweep on 16.53: zero clean patterns, current flat
   (`scripts/sweep-test-modes.sh`, `scripts/analyze-testmode-sweep.py`).
   Re-tested clean-state on both firmwares: **the card's own test generator
   renders the same noise as our content.**

---

## 3. Where the fault is now isolated

With correct frames, on a clean boot (no strobe state):

* chip pack arms drivers: 0.62 A → 4.5 A, unmodulated white field
* streaming content changes current (≈3.0 A) and visibly perturbs the panel
  (structured black regions with a sawtooth boundary, shifting noise bands)
* brightness scales current (b8 ≈ 0.97 A, b40 ≈ 1.9 A)
* **but every content source — our frames AND the internal test generator —
  renders per-pixel noise**, on firmware 10.81 and 16.53 alike

So: link good, raster running, chips powered — the failure is in the
gateware ↔ driver-chip serial protocol or the chip register values. If the
ICs are not actually SM16269S (or are a variant), everything observed is
explained.

## 4. Next steps, in order

1. **Get the physical IC marking from the user** (asked; they are at the
   bench). Then match firmware + registers to the real chip. If it's a
   conventional (non-PWM) chip, try `E320_PCB6.0_Normal_FPGA13.39` — note its
   different image format (`firmware/README.md`).
2. **ChipData agent results** (running when session ended): vendor register
   defaults for SM16269*, the chip-id→firmware-variant mapping, and a §21.2
   scan-field re-check (compiled pack says "8" where panel/record say 1/16).
3. If registers differ from ChipData defaults: patch record 0x84, resend the
   RAM trio, re-test (`strobe.sh` + current, brightness ≤40).
4. Once anything renders recognizably: A/B geometry (128x64 vs 256x32 fold),
   color order, then `test rgb` → `image` → `play`.
5. Optional persistence once rendering works: write chip pack into compiled
   page 0x09 + reload-0x77 so the panel arms from flash at boot
   (`firmware/derived/sm16269s-block7.bin` is the factory-identical base).

## 5. Traps (updated)

1. Never pass a firmware image as `--base-image` to `write-config`, and never
   `restore-screen-record --from-image` a firmware file (see git history).
   Recovery: `firmware/card-dumps/primary-region.bin`.
2. Camera: averaged stills for spatial content, `strobe.sh` for temporal —
   the user caught real strobing that snap-avg completely hid.
3. `CH1: Cc` in `ka3005p status` = rail sagging = stop and power-cycle;
   keep brightness low in the all-on state.
4. RAM params die on reboot; chips latch at arm time — re-arm after changes.
5. The upgrade path works on both directions (16.53 ⇄ 10.81) but the
   completion report can outlast the 120 s wait — card answered discovery,
   power-cycle then was fine. Do not power off while unresponsive.

## 6. Assets

* `firmware/card-dumps/primary-region.bin` — day-one dump, source of truth.
* `firmware/derived/` — extracted vendor packs (ready payload hexes), the
  factory-identical block-7 image.
* `docs/compiled-image-format.md` + `scripts/compile_rcvbp.py` — the 0x70000
  format and generator.
* LEDVISION extracts + `libCLTDevice.asm` (44 MB disassembly, all doc
  addresses) in the OLD session scratchpad:
  `/private/tmp/claude-501/-Users-amd-e120/261c3dad-ba97-45d2-8ea3-ab7a950a8ff9/scratchpad/`
  (re-extract per `docs/vendor-sdk-analysis.md` if purged).
* Trial photos/currents: `/tmp/e120-trials/`.
