# receiverproxy

![A Raspberry Pi drives chained Colorlight E120 cards over Ethernet; each card drives part of an LED wall over HUB75 ribbons](docs/readme-header.png)

`receiverproxy` is a command-line tool (`rxp`) and a web app that drive a Colorlight E120 LED receiving card and its modules over raw Ethernet: they generate and flash the module configuration, then put images, video and live streams on the panel.

The aim is to drive any LED receiving card and its panels from a computer with an Ethernet port, without a sender card, vendor software or an account.

**Drive receiving cards without a sender card and without vendor software.** `rxp` discovers the card, backs up and installs firmware, generates and flashes the configuration from a text file, sets the card's place in a wall, and streams to it. Useful for cards and panels the vendor supports badly or not at all.

The card speaks a layer-2 protocol with fixed MAC addresses and no IP, so `rxp` writes whole Ethernet frames itself (BPF on macOS, a packet socket on Linux) and needs no vendor software. It reads and writes the card's flash and EEPROM, with address allowlists that keep configuration writes inside the parameter block and firmware writes on the card's own staging path. It installs FPGA firmware. It generates the receiver configuration (the `.rcvbp` file and the 64 KB boot image the card loads at power-on) from a short TOML panel spec plus a chip library, and ships libraries for common driver chips and module classes mined from 2,381 vendor config files. It shows still images, plays video through ffmpeg, reads raw rgb24 frames from stdin, and serves a unix socket other programs can write frames to. Every command that writes flash or EEPROM prints its plan and stops unless `--commit` is given.

## Motivation

Colorlight's own software is the only official way to drive its receiving cards, and LEDVISION dropped the ability to send content to a receiving card directly: a sender card or box is required, and the software itself is a large Windows install. `rxp` works with zero Colorlight software. A Mac or Linux machine with an Ethernet port talks to the card directly, provisions it from a text file, and plays whatever ffmpeg can decode.

## Project status

Tested only on the hardware listed under [Tested](#tested), one card and one module. Other cards and firmware builds may behave differently, and a firmware or flash write to an untested card can leave it unbootable: take `rxp flash snapshot` first and keep the result. If you run this on other hardware, whatever the outcome, open a pull request or an issue with the card model, the module, and what you saw.

## Install

Rust stable via [rustup](https://rustup.rs) (`rust-toolchain.toml` pins the channel), then:

```sh
cargo install --path crates/cli
```

receiverproxy is the project, `rxp` is the command (`receiverproxy` is the same binary); crates are named for their role (`crates/cli` builds both).

`rxp show video` and `rxp show stream` pipelines need `ffmpeg` on the PATH (`brew install ffmpeg`).

Raw Ethernet needs privileges. On macOS the BPF devices must be readable and writable; without that `rxp` fails with `could not open any /dev/bpf* device ... (try: sudo chmod o+rw /dev/bpf*)`:

```sh
sudo chmod o+rw /dev/bpf*    # resets on reboot
```

On Linux the binary needs the raw-socket capabilities (`CAP_NET_ADMIN` because the socket is put in promiscuous mode to see the card's replies), or root; redo after every `cargo install`:

```sh
sudo setcap cap_net_raw,cap_net_admin+ep "$(command -v rxp)"
```

The card is connected directly to one interface; the default is `en24`, pass `--iface` for another (`eth0`, `enp3s0` and the like on Linux; the link must be up but needs no address). `scripts/mirror.sh` uses `x11grab` on Linux and does not work under Wayland.

## Usage

```
Drive a Colorlight receiving card over raw Ethernet

Usage: rxp [OPTIONS] <COMMAND>

Commands:
  discover    Send a discovery packet and print any card that answers
  brightness  Set panel brightness (0-255)
  provision   Bring a card to a working state: snapshot, firmware, config, EEPROM, verify
  show        Put pixels on the panel: images, video, streams, patterns
  config      Generate, inspect and transfer .rcvbp configurations
  flash       Read and write the card's flash, and snapshot or restore it
  firmware    List, fetch and install FPGA firmware
  card        Card state held in RAM or EEPROM: layout, screen size, test modes
  debug       Wire diagnostics: listen, hand-built frames, pcap tools
  ui          Serve the web UI and its JSON API; the printed URL carries the token
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help                     Print help
  -i, --iface <IFACE>            Network interface directly connected to the receiving card [default: en24]
      --width <WIDTH>            Panel width in pixels [default: 128]
      --height <HEIGHT>          Panel height in pixels [default: 64]
      --order <ORDER>            Color order on the wire [default: bgr]
  -b, --brightness <BRIGHTNESS>  Brightness 0-255 (sent in sync frames) [default: 255]
      --card <NAME>              Card model to work from instead of the one discovery reports (`rxp card models`)
```

Find the card; the reply includes its firmware version and the model `config/cards/` gives its id byte:

```sh
rxp discover
rxp card models       # the models in config/cards and how far each is tested
rxp card probe        # read-only: check the model file's claims against the card, exit 1 on a mismatch
```

Flash, firmware and configuration commands take the card's memory map (banks, parameter block, EEPROM mirror, boot-image layout, blocks the firmware guards) from that model; `--card NAME` names one instead of discovering, and `config gen` without `--card` lays the boot image out for the first tested model. `card probe` reads the discovery reply, the head of each firmware bank, the parameter block and the EEPROM mirror and prints one `ok` / `mismatch` / `not checked` line per claim ([docs/cards.md](docs/cards.md)).

Provision a card from a panel spec and a firmware image. Without `--commit` it prints the five steps (snapshot, firmware, EEPROM read, config, EEPROM write) and does nothing. Power-cycle the card afterwards; it configures itself from flash.

```sh
rxp provision --spec config/panels/p25-128x64-sm16269s.toml \
    --firmware E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex \
    --position 0,0 --commit
```

Show an image, scaled to the panel, and keep refreshing until Ctrl-C:

```sh
rxp show image picture.png --hold
```

Play a video in a loop at 30 fps, fitted inside the panel:

```sh
rxp show video clip.mp4 --loop --fps 30 --fit contain
```

Pipe frames from ffmpeg. `show stream` reads bare rgb24 frames of `--size` from stdin; a size other than the panel's is resampled with `--fit`:

```sh
ffmpeg -i clip.mp4 -vf scale=128:64 -f rawvideo -pix_fmt rgb24 - \
    | rxp show stream --size 128x64 --fps 30

ffmpeg -f avfoundation -pixel_format bgr0 -framerate 30 -i "Capture screen 0" \
    -vf scale=128x64:flags=area -fps_mode cfr -r 30 -f rawvideo -pix_fmt rgb24 - \
    | rxp show stream --size 128x64 --fps 30
```

`scripts/mirror.sh` is the second pipeline as a script: `scripts/mirror.sh -s 128x64 -f 30 -c 0,0,640,320` mirrors a crop of the screen.

Serve a unix socket. One client at a time connects, sends a 12-byte header, then rgb24 frames, and is paced at the header's fps; the panel keeps the last frame between clients. The header is `RXP` and a zero byte, version byte `1`, one reserved byte, then width, height and fps as little-endian u16 (`crates/sources/src/raw.rs`).

```sh
rxp show serve                      # --socket PATH; /tmp/receiverproxy.sock by default
```

```python
import socket, struct
s = socket.socket(socket.AF_UNIX); s.connect("/tmp/receiverproxy.sock")
s.sendall(struct.pack("<4sBBHHH", b"RXP\0", 1, 0, 128, 64, 30) + bytes(128 * 64 * 3))  # header, then one black frame
```

Other things the panel can show:

```sh
rxp show fill ff8000 --hold        # solid colour
rxp show test rgb --hold           # gradient | rows | border | rgb | white
rxp show blank
rxp brightness 40
```

Configuration without a full provision:

```sh
rxp config gen --spec config/panels/p25-128x64-sm16269s.toml --out-dir build
rxp config formats                                # the formats --format takes; rcvbp today
rxp config info build/p25-128x64-sm16269s.rcvbp
rxp config read --out card.rcvbp                  # what the card holds
rxp config diff card.rcvbp build/p25-128x64-sm16269s.rcvbp
rxp config import card.rcvbp --out card.toml      # the spec that regenerates a file
rxp config send --spec config/panels/p25-128x64-sm16269s.toml   # RAM only, no flash
```

`config import` reads record 0x01 field by field, picks the chip library by the file's chip id from the ones under `config/chips/`, fits the wiring knobs to the pixel map, and puts whatever the regenerated file would still differ in on stderr as `not recovered: ...` lines, by name (`meta`, `boot.arm_at_boot` and the phantom-position gate always, since no `.rcvbp` carries them). A `config read` of the card followed by `config import` gives the spec the card is running.

Flash and firmware. Reads are always safe; writes need `--commit`:

```sh
rxp flash snapshot --dir before                   # primary bank + golden bank
rxp firmware list                                 # the images in config/firmware.toml and where each is
rxp firmware install E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex --commit
rxp flash restore --dir before --commit           # configuration back, not firmware
```

`config/firmware.toml` lists the five vendor images archived under `third-party/firmware/` with version, board revision, build kind, driver chips, size and sha256. `provision --firmware`, `firmware install` and `firmware write` take a name from it or a path; a name is looked for under `third-party/firmware/` and then in the config directory's `firmware/` cache, and an image that is in the manifest is checked against its sha256 before any write (a mismatch is refused). A path outside the manifest is written as is, with a warning. `rxp firmware fetch NAME` downloads `base_url/NAME` with `curl` into the cache and checks it; the manifest's `base_url` is empty, so `fetch` only reports where the image is expected.

`rxp ui` serves the same commands as a web UI and a JSON API on `http://127.0.0.1:7120`; see [Web app](#web-app).

Multi-panel walls: provision each card with its own `--position x,y`, put the same `x,y` on that card's receiver entry in a layout file (`rxp card layout-example` prints a two-card one; panels are placed inside their receiver) and stream it with `rxp show video --layout wall.json`. Every card hears the whole screen and keeps its own window of it.

## Demos

`rxp-demo` (`cargo install --path crates/demos`) is a second binary on the same driver: effects that use what an LED is rather than what an LCD is. `list` prints the names with a line each; `cycle` runs them all in turn; Ctrl-C leaves the panel showing its last frame.

```sh
rxp-demo stars
rxp-demo cycle --every 20
rxp-demo list
rxp-demo comet --seconds 30 --brightness 40 --iface en24 --layout wall.json
```

- `stars`, `fireflies`, `comet`, `fog`: an LED that is off emits nothing, so a single lit pixel, a pixel at a few percent, or a gradient near black sits in real black with no backlight floor.
- `lightning`, `pulse`, `cast`: the whole panel goes from black to full and back within one refresh, and the latch frame's gain and per-channel gain bytes change brightness or colour balance without a pixel being resent.
- `primaries`, `fire`: each pixel is three narrow-band emitters, so red at a low level stays red and pure R, G and B discs overlap into additive white.
- `life`, `sand`, `rain`: each pixel is a physical light, so a grid of discrete cells reads as one.
- `scanner`: a bright row then a column sweeping at 240 fps, sending only the rows that changed; a phone camera's rolling shutter slices the sweep into bands the eye does not see.

`--fps` defaults to 30, or 240 where an effect asks (`comet`, `scanner`); the card's own scan sets what the panel can follow. Row-only updates apply to the default single panel; with `--layout` every frame is sent whole. Whether the per-channel gain bytes change anything on the card has not been measured.

## Web app

`web/` is a browser front end for the same commands, with four screens:

- Gallery: every panel spec under `config/panels/` as a table (pitch, module, scan, chip, the formats it generates, how many vendor files it came from, tested or generates), with a filter row. Selecting a row shows the spec field by field, with one "download as" button per output format (`rcvbp` today: the `.rcvbp`, the basic pack and the boot image) and "open in Builder". A file dropped on the Gallery or the Builder is read back into a spec: the format is detected from the bytes, the chip library is picked by the file's chip id, and the fields the file does not carry are listed by name.
- Builder: the panel spec as a form and as TOML, kept in sync; generate for a chosen format, inspect and diff `.rcvbp` files.
- Wall: an editor for the layout JSON, as a drawing and as tables.
- Cards: discovered cards, brightness, show, provision, firmware, flash snapshot and restore.

The site runs on its own: Gallery, Builder and Wall work in the browser through `rcvbp` and `wall` compiled to WebAssembly (`crates/rcvbp-wasm`), and nothing touches a card. The daemon is optional: `rxp ui` starts `crates/daemon`, which holds the Ethernet link, serves the built site and a JSON API under `/api/v1`, and runs the long operations as jobs, one at a time. With it the Cards screen appears, and the Gallery, Builder and Wall gain their card actions (provision this card, send to card, write to card, show on the wall). Without it the sidebar says so and how to install it.

Build and run (needs [pnpm](https://pnpm.io), the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.127, the version `crates/rcvbp-wasm/Cargo.toml` pins):

```sh
web/scripts/build-wasm.sh                 # cargo build -p rcvbp-wasm for wasm32, then wasm-bindgen into web/src/wasm
cd web && pnpm install && pnpm build:embed   # svelte-check, then the static build into web/build-static
cargo install --path crates/cli      # embeds web/build-static; rerun after every pnpm build:embed
rxp ui                                   # http://127.0.0.1:7120/#token=...; --port, --listen ADDR, --no-open, --token TOKEN, --data-dir DIR, --iface
```

The same app is the site at [receiverproxy.com](https://receiverproxy.com): `pnpm build` (adapter-cloudflare) and `pnpm deploy` (`wrangler deploy`, `web/wrangler.jsonc`). Its Gallery and Cards pages are prerendered from `config/` at build time; the Builder and Wall run in the browser.

Every API request needs a token. `rxp ui` generates a random one at start (or takes `--token TOKEN`), prints the URL with it in the fragment and opens that URL; the app keeps the token for the tab and sends it as `X-Token`. A request without it gets 401. The token is what keeps other pages open in the same browser, which can reach loopback too, from driving the panel or writing the card's flash while the daemon runs. The daemon binds 127.0.0.1 unless `--listen ADDR` names another address (`0.0.0.0` for every interface); then the token is what keeps other machines out, and it crosses the network in clear HTTP. Writes keep the CLI's gate: the API returns the plan unless the request carries `commit: true`. The API, the WASM surface and the app are specified in [docs/ui.md](docs/ui.md).

## Tested

The table is generated by `rxp card models --markdown` from `config/cards/*.toml` and the mined module classes; a test keeps it current.

<!-- tested -->
✅ driven on the bench · ⚠️ configuration generates, never driven · ❌ not supported

| panel (driver chip) | Colorlight E120 | other Colorlight E-series | Linsn · Novastar · Huidu |
|---|:---:|:---:|:---:|
| 128x64 1/16, SM16269S (`config/panels/p25-128x64-sm16269s.toml`) | ✅ | ⚠️ | ❌ |
| DP5525 | ⚠️ | ⚠️ | ❌ |
| ICN2038S · ICN2053 · ICN2055 · ICN2065 · ICND2163 | ⚠️ | ⚠️ | ❌ |
| LS9929 · LS9930 · LS9935B · LS9936 | ⚠️ | ⚠️ | ❌ |
| MBI5124 · MBI5124N · MBI5153 | ⚠️ | ⚠️ | ❌ |
| MY9862 · MY9868 | ⚠️ | ⚠️ | ❌ |
| SM16169S · SM16237DS · SM16259 · SM16380 · SM16389 | ⚠️ | ⚠️ | ❌ |
| SM16369S · ICND2263 (register record not decoded) | ❌ | ❌ | ❌ |
| snake-wired outdoor modules (1/2, 1/4, 1/5, 1/10 scan) | ❌ | ❌ | ❌ |

The ⚠️ chip rows are the 87 module classes in `config/panels/mined/`, grouped by driver-chip family.
<!-- /tested -->

Other E-series cards are ⚠️ because the 16.53 firmware image is itself named E320 and the protocol is shared, but none has been tried. Host: macOS ✅, Linux ⚠️ (builds and lints for x86_64 and aarch64, not run against a card). Adding a card or a panel is described in [docs/cards.md](docs/cards.md).

## Configuration

A panel is one TOML file in `config/panels/`. `rxp config gen` turns it into the `.rcvbp`, the boot image, and a text file saying where each byte came from. The bench panel's spec, shortened:

```toml
name = "p25-128x64-sm16269s"

[meta]
pitch_mm = 2.5
status = "tested"
origin = "bench"
sources = 1
vendors = ["Colorlight"]

[module]
gray_bits = 12
width = 128
height = 64
scan = 16
line_dir = 0
data_groups = 1
serial_clock = 8

[screen]
width = 128
height = 64

[chip]
library = "config/chips/sm16269s-factory.toml"

[color]
swap = 3
source = [2, 1, 0]

[current]
gains = [43, 43, 43, 43]
percent = [0.1, 0.1, 0.1]

[timing]
gamma = 2.8
refresh_hz = 60.0
gclock = 0x14
min_oe = 0.0001
luminance_level = 188
oe_8ns = true

[mapping]
reversed_groups = true
reversed_lines = false
block = 64

[boot]
arm_at_boot = true

[record01_overrides]
"0x02F" = 0x01
```

- `[meta]` where the values came from and how far they are trusted: `status` `tested` (driven from flash) or `generates`, `origin` `bench` or `mined`, the number of vendor files behind it and, for a mined spec, the share that agree and a few of them by name. Optional; a spec without it counts as mined from nothing.
- `[module]` the module: size, scan (1/16 here), line direction, data groups, grey depth, serial clock.
- `[screen]` the whole screen this card drives.
- `[chip]` the driver-chip library file: chip ids, the 20-byte chip-control block, the register table.
- `[color]` channel swap and the R/G/B source order.
- `[current]` per-channel current gains and percentages.
- `[timing]` gamma, refresh rate, GCLK, minimum OE, luminance level, 8 ns OE.
- `[mapping]` the wiring: group and line reversal, and the column block after which the row halves alternate along the shift chain.
- `[boot]` whether the card arms the drivers from flash at power-on.
- `[record01_overrides]` individual record 0x01 bytes applied last.

Chip libraries are in `config/chips/`, panel specs in `config/panels/`. `config/chips/mined/` (21 chip families) and `config/panels/mined/` (87 module classes) were generated by `scripts/corpus-mine.py` from the vendor config corpus; a chip library's header and a panel spec's `[meta]` table state how many files agreed. Only the P2.5 128x64 SM16269S module on firmware 16.53, with `config/panels/p25-128x64-sm16269s.toml`, has been driven on a bench. Everything mined is a vendor default, not a measurement; use it as a starting point and check it against a vendor file for the exact module when one exists.

How the generator derives each record, and its limits, is in [docs/building-a-config.md](docs/building-a-config.md).

## Notes and evidence

[docs/README.md](docs/README.md) indexes the notes: the `.rcvbp` container and record 0x01 byte by byte, the boot image region by region, the pixel map, the chip-control block, the EEPROM records, the pixel protocol recovered from the vendor sender DLL, the FPGA bitstream and flash layout, and the firmware 16.53 install. `crates/rcvbp/tests/factory.rs` pins the generator to the card: the factory basic pack and boot image regenerate byte for byte from the card's factory flash dump (kept outside the repository; those tests skip without it). Bench results were taken with PSU current readings and averaged camera captures ([docs/bench.md](docs/bench.md)); the values that mattered and what each alternative did are in [docs/rendering.md](docs/rendering.md), and the claims that measurement disproves are in [docs/retracted-findings.md](docs/retracted-findings.md).

## Hardware

Developed against one Colorlight E120 receiving card running firmware 16.53 (`E320_PWM_FPGA16.53_20231227_SM16386S_SM16269SH.hex`), one P2.5 128x64 SMD1415 module, 1/16 scan, with SM16269S driver chips, on macOS. Linux is supported through a packet socket and builds and lints for that target, but has not been run against the card.

## License

MIT, see [LICENSE](LICENSE). The Raspberry Pi photo in the header is from Wikimedia Commons, CC BY-SA 4.0.
