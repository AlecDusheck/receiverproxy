# Handoff: getting the panel to display

Written at the end of a long session. The card runs correct firmware and the
panel lights and draws real current, but **it does not yet display what we
send**. This is everything the next agent needs.

Read [`firmware/README.md`](firmware/README.md) and
[`docs/config-protocol.md`](docs/config-protocol.md) alongside this.

---

## 1. The rig

| Thing | Detail |
|---|---|
| Host | Mac, USB-Ethernet **AX88179B on `en24`**, raw layer-2 via `/dev/bpf` |
| Card | Colorlight **E120**, reports `id=0x64 firmware=16.53` |
| Panel | One **P2.5, 128x64, SM16269S** driver ICs, on hub **J1 only** |
| PSU | KA3005P at 5 V, **current limit now 5.1 A** — read/power-cycle it with `ka3005p` |
| Camera | Front webcam pointed at the panel |

`sudo chmod o+rw /dev/bpf*` after every reboot of the Mac, or nothing talks.

**PSU rules from the user: read it and power-cycle it, never change voltage or
current settings.**

```sh
ka3005p status          # Voltage: 5.00 (5.00), Current: 4.477 (5.100), ...
ka3005p power off       # power-cycling the card is expected and encouraged
ka3005p power on
```

**Vendor SDK file inspection must be delegated to an Opus 5 subagent**
(`Agent` with `model: "opus"`), not done in the main loop — the user was
explicit about this. The procedure, the file map, and a prompt template are in
[`docs/vendor-sdk-analysis.md`](docs/vendor-sdk-analysis.md). Never execute the
vendor software.

---

## 2. Where things actually stand

### Works, verified on hardware

* Discovery, flash read/write, config read/write, EEPROM screen record.
* **Firmware upgrade over Ethernet** via the card's SDRAM staging path. The
  card now runs `E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex`, the build
  for **SM16269SH/SM16386S**, verified byte-for-byte in both bitstream regions.
* **Our frames reach the wire** — confirmed with `e120 listen --include-ours`:
  correct types, lengths, row numbering.
* **Brightness commands visibly change the panel.** A/B measured: mean panel RGB
  moved by delta 264 on `brightness 0` and 360 on the way back up. So the card
  *is* receiving and acting on our display-control frames.
* **Driver chips arm** when the chip-register pack is pushed: PSU jumps
  0.32 A → 0.79 A (at the old 2 A limit) and the panel lights fully.

### Does not work

**The panel does not render our pixel content.** At full power it shows a dense
mostly-white field with horizontal red/cyan/magenta streaks. Sending
`e120 test rgb` (which should be three clean vertical bars) changes nothing
recognisable. Colour fills do not produce their colour.

Treat "the panel changed" claims sceptically — see the camera trap in §5.

---

## 3. Bring-up: the sequence that gets furthest

```sh
cd ~/e120 && cargo build

# 1. Power-cycle. Card boots at ~0.63 A with the factory compiled block.
ka3005p power off && sleep 4 && ka3005p power on && sleep 8
./target/debug/e120 discover
#    -> reports 1024x512 at this point, which is the FACTORY geometry

# 2. Tell the card its real size. Without this it stays 1024x512 and our
#    128x64 frames land outside its window.
./target/debug/e120 set-layout
./target/debug/e120 discover        # -> now 128x64

# 3. Arm the SM16269S drivers. They are NOT armed from flash at boot.
./target/debug/e120 send-params firmware/P2.5-32S-128X64-SM16269S-256X384I.rcvbp --chip-only
#    -> PSU current jumps; panel lights

# 4. Send content
./target/debug/e120 --brightness 25 test rgb
```

`scripts/trial.sh <name> [cmd...]` automates power-cycle → wait → run → record
current + photo. `scripts/ab.sh <name> <cmd...>` measures a command's visible
effect as a number instead of an opinion.

---

## 4. Flash layout — the thing that cost the most time

**Block 7 holds two different configurations.** This is not documented anywhere
else and it is the key structural fact:

```
0x000000-0x02FFFF  FPGA bitstream, part 1   (host CANNOT write; only the card can)
0x030000-0x07FFFF  RESERVED - not part of the loadable bitstream
  0x070000           COMPILED PARAMETER IMAGE  <-- what the card applies AT BOOT
                     starts with the 0xA8 pack marker; factory copy ends ~0x763E0
  0x078000           the .rcvbp source file, u32-LE length-prefixed
                     this is what `read-config` returns and `write-config` writes
  0x07F000           256-byte screen-size record, mapped to a small EEPROM;
                     never accepts flash page writes, geometry at bytes 6 and 8
0x080000-0x0AFFFF  FPGA bitstream, part 2   (host CANNOT write; only the card can)
0x200000-0x2AFFFF  golden/backup bitstream  (untouched all session)
```

Writing a `.rcvbp` to `0x78000` **does not change what the card boots with**.
The card compiles its parameters into `0x70000` and reads that. Ours is still
the **factory** compiled block, restored from the day-one dump — which is why
`discover` reports the factory 1024x512 until `set-layout` overrides it in RAM.

---

## 5. Traps. Please read before touching anything

Each of these cost hours and produced confidently wrong conclusions.

1. **Never pass a firmware image as `--base-image` to `write-config`.**
   Doing so splices bitstream bytes over the compiled param block at `0x70000`
   and the card then boots unconfigured — no raster, nothing displays, and no
   `.rcvbp` write can fix it. This is what silently broke the card mid-session.
   Recovery: restore block 7 from `firmware/card-dumps/primary-region.bin`.

2. **Never restore the screen record from a firmware image either.**
   `restore-screen-record --from-image <firmware>` writes bitstream bytes as
   geometry; it set the card to `1544x128` and once to `20940x32768`.
   Use `e120 screen-size --set 128x64 --commit`, which sets it by value.

3. **The camera aliases with the panel refresh.** Single stills show phantom
   bands, colour shifts, and "changes" that are pure strobing. Always use
   `scripts/snap-avg.sh`, which averages 24 frames. Several of this session's
   "the panel changed!" moments were this artifact. Also check the camera is
   actually aimed at the panel — it drifted to pointing at the user twice.

4. **Brownout invalidates everything.** At the old 2 A limit, arming the drivers
   pulled the rail to 3.35 V and the card stopped processing frames. Every test
   run in that state was meaningless — *uninformative, not negative*. The limit
   is now 5.1 A and the rail holds at 5.00 V; if you see `CH1: Cc` in
   `ka3005p status`, stop and power-cycle before believing any result.

5. **RAM params do not survive a reboot, and chips latch config at startup.**
   `send-params` is RAM-only. Testing a config change without re-arming (or
   without a reboot when the change belongs in flash) proves nothing.

6. **`fill` sends 3 frames and exits.** For anything measured with a meter or a
   camera, use `--hold` or `probe --repeat`.

---

## 6. Two protocol details that are currently unresolved

Both are places where the documentation and the hardware disagree, and where I
changed the code back and forth. Do not "fix" either without a panel in view.

**`pixel_row` frame layout.** `crates/e120-proto/src/pixel.rs` is currently on
the **FPP ColorLight-5a-75 layout** (uncommitted in the working tree): type byte
`0x55` at offset 12, `0x00` at 13, then payload `[row MSB, row LSB, offs MSB,
offs LSB, count MSB, count LSB, 0x08, 0x88, pixels...]`. Earlier in the session
it was a split layout (row high byte in the type's second byte, row low byte
opening the payload) and that variant *did* visibly disturb the panel while the
FPP layout did not — but that observation was made during a brownout, so it is
not trustworthy. Re-test both now that the rail is stable. FPP source:
`ColorLight-5a-75.cpp` in the FalconChristmas/fpp repo.

**`basic_pack` sub-index.** `docs/config-protocol.md:2590` specifies
`payload[0x03] = 0x02` for the basic-parameter pack. Setting it **broke driver
arming** — the card then accepts a pack whose geometry fields are all zero and
the raster stops. It is currently commented out in
`crates/e120-proto/src/params.rs`, and the empirically-arming pack is preserved
byte-for-byte as `basic_pack()` with a pin test. The table-derived §21.2 version
lives beside it as `scan_pack()`, sent only with `send-params --scan-pack`.

---

## 7. What I'd try next, in order

1. **Generate an SM16269S compiled param block for `0x70000`.** This is my
   leading hypothesis for why nothing renders: the card boots from a *factory*
   compiled block built for the old DP3153-era setup and the factory 1024x512
   screen, and no amount of `.rcvbp` writing changes it. We hold a perfect
   Rosetta stone — the factory compiled block **and** the factory `.rcvbp` it
   was compiled from — so the format is recoverable by correlation:
   ```sh
   python3 -c "
   d=open('firmware/card-dumps/primary-region.bin','rb').read()
   open('/tmp/factory-compiled.bin','wb').write(d[0x70000:0x76400])
   n=int.from_bytes(d[0x78000:0x78004],'little')
   open('/tmp/factory-config.rcvbp','wb').write(d[0x78004:0x78004+n])"
   ```
   Both configs' record 0x01 already agree on `chip 0x014c` (SM16269S) and
   `scan 16`, so the difference to hunt is geometry and row mapping.

2. **Find the vendor's "save to flash" command**, which makes the card compile
   `.rcvbp` → `0x70000` itself. Much better than synthesising the block by hand.
   `docs/config-protocol.md` §3 lists the LEDVISION save-path types seen in
   capture: **0x11** (save config, `data[3]` = rx index), plus `0x1F`, `0x26`,
   `0x31`, `0x32`, `0x76`. I tried a naive `0x1100` frame and it did nothing;
   the payload layout is unresolved. **Give this to an Opus 5 subagent** —
   see [`docs/vendor-sdk-analysis.md`](docs/vendor-sdk-analysis.md) for the
   method and a ready prompt.

3. **Re-test both `pixel_row` layouts** on the now-stable rail, with
   `scripts/ab.sh` for numbers and `snap-avg.sh` for pictures.

4. **Sweep the geometry.** The config filename says `256X384I` and the panel may
   chain as 256 wide rather than 128 — `set-layout --panel-width 256
   --panel-height 32` visibly changed coverage once. Worth a systematic sweep
   now that measurements are trustworthy.

5. **Sweep the built-in test-mode selector.** `docs/config-protocol.md` §16.1
   says the selector enum "is not recoverable statically"; only a few of 256
   values have been tried, and our frame matches the disassembly exactly. If any
   selector produces a clean pattern, the card→panel path is proven good and the
   fault is entirely in our pixel path.

---

## 8. Assets on disk

* **Backups.** `firmware/card-dumps/primary-region.bin` is the day-one dump and
  the source of truth for block 7 — it is the only copy of the factory compiled
  param block. `golden-bank.bin` is the untouched backup bitstream at 0x200000.
* **Firmware images.** `firmware/images/` — see `firmware/README.md`.
* **LEDVISION 9.6 extracted**, with a full `config_files` tree and a `ChipData`
  directory, at:
  `/private/tmp/claude-501/-Users-amd-e120/261c3dad-ba97-45d2-8ea3-ab7a950a8ff9/scratchpad/ledvision/`
  The x64 sender is `$_15_/x64/Bin/CLTNic.dll` (exports `Nic_SendScreenPicture`,
  `Nic_SetBrightness`, `Nic_SetScreenSize`, `Nic_SetTestModeIndex`, ...) and
  `CLTDevice.dll` is beside it. **Opus 5 subagent only** — method and prompt
  template in [`docs/vendor-sdk-analysis.md`](docs/vendor-sdk-analysis.md).
* **Trial output** — photos and current readings — in `/tmp/e120-trials/`.
