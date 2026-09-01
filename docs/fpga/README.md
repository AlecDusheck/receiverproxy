# The E120 FPGA gateware — index

Reverse engineering of the Colorlight E120 receiving card's gateware, from the
vendor firmware images in `third-party/firmware/`.

Start at [`docs/fpga-gateware.md`](../fpga-gateware.md) for the two-page
overview. This directory is the detail.

## Confidence tags

Every claim in these files carries one:

| tag | meaning |
|---|---|
| **HIGH** | read directly out of the bytes or the database, and cross-checked |
| **MEDIUM** | a strong pattern resting on one stated assumption |
| **NOT RESOLVED** | we could not determine it; what was ruled out is stated |

Nothing here is an inference presented as fact. This project has lost hours to
that, and the tags are the defence. If a file says NOT RESOLVED, treat a
confident-sounding claim elsewhere about the same thing as suspect.

## The files

| file | covers |
|---|---|
| [bitstream-format.md](bitstream-format.md) | The `.bit` container: part identification, the ASCII header field by field, every command opcode and operand, frame geometry, the CRC-16 spec and the running-prefix rule, enough to write a parser or rebuild an image. Also the two decode traps (`word:` bit order, `BASE_TYPE` names). |
| [decode-method.md](decode-method.md) | Exact reproduction: tool versions, install, the one piece of format wrangling `ecpunpack` needs, the `.config` text format, the five pytrellis API traps that cost the most time, and what every script in `analysis/fpga/scripts/` does. |
| [flash-layout.md](flash-layout.md) | How a `.hex` maps into SPI flash (delta 0), the complete address map, the `0x7F000` EEPROM-redirect question, the 55 failing frame CRCs and what causes them, **which firmware the card is actually running (10.81, not 16.53)**, and where the compiled boot image lands. |
| [resources.md](resources.md) | LUT / FF / BRAM / DSP utilisation, the PLL decoded with its bit-order argument, the reference clock pin, every global clock net and DCC, clock domains, and how the LED outputs are registered. |
| [pinout.md](pinout.md) | The full 197-pin table with direction from the routing graph (not from `BASE_TYPE`, which is a degenerate decode), the two RGMII gigabit ports proven pin by pin, the SPI flash bank, the ~147 LED-side pins including the 96 identified RGB data lines, and the board architecture that implies. |
| [block-ram.md](block-ram.md) | The one initialised BRAM: location, size, contents, the 21-bit opcode+immediate decode, what it is and is not, and every other block RAM's mode and width. |
| [parameter-path.md](parameter-path.md) | All three transports (live Ethernet, flash writes, boot read), the 41-pack real-time push, the complete 256-byte basic pack, what is shipped precomputed vs derived, and every constant searched for with FOUND / NOT FOUND / NEVER SEARCHED. |
| [output-stage.md](output-stage.md) | HUB75E connector and signal set, what the SM16269 family requires, the S-PWM structure, scan handling and the OneScanLen/CardScanLen arithmetic, the scan table (and why its all-zero bit times are normal), the pixel mapping, **the output stage traced in the netlist** (96 RGB pins, the global blank and 2:1 source select, counter-vs-BRAM), and a ranked list of hypotheses for why the panel does not render. |
| [version-diff.md](version-diff.md) | 16.53 vs 10.81 vs 9.53 vs 13.39 vs 6.69 at the decoded level, what separates the PWM / Normal / LS families, and why the version numbers are not one sequence. |
| [chip-id.md](chip-id.md) | The full driver-chip-id investigation: how the id reaches the card, the vendor chip table, everything searched, everything ruled out, why the negative is credible but not absolute, the one concrete lead, and what to send. |
| [open-questions.md](open-questions.md) | Every unresolved item, tiered by impact, each with what evidence would settle it — plus the searches that are **dead** and must not be repeated. |

## Reading order

* **Getting a picture on the panel:** [output-stage.md](output-stage.md) §6 →
  [open-questions.md](open-questions.md) Tier 1 → [chip-id.md](chip-id.md) §8.
* **Flashing or dumping the card:** [flash-layout.md](flash-layout.md) first,
  and note §5.1 — the card is running 10.81, not 16.53.
* **Continuing the reverse engineering:**
  [decode-method.md](decode-method.md) → [pinout.md](pinout.md) →
  [open-questions.md](open-questions.md) Tier 2.

## Raw artefacts

Durable data and scripts live in **`analysis/fpga/`**:

| file | what |
|---|---|
| `PINTABLE_16.53.txt` | the clean 197-pin table with method and caveats |
| `pinmap_16.53.txt` | package pin → row/col/PIO/bank/special function + raw IO config lines |
| `final_*.tsv` | per-pin raw routing evidence, all five images |
| `bramdump_*_3.txt` | the initialised BRAM, raw 2048 × 9-bit words, all five images |
| `rom_*_decoded.txt` | the same, decoded as 21-bit opcode + immediate |
| `lut_hist_16.53.txt` | LUT INIT histogram |
| `clusters_16.53.txt` | AND-of-one-hot comparator clusters |
| `chains_16.53.txt` | CCU2 carry-chain constant-compare candidates |
| `bytecmp_16.53.txt` | exhaustive byte-register comparator sweep |
| `pll_dump.txt` | verbatim PLL tile text, all five images |
| `summary_cross_image.txt` | cross-image LUT/comparator counts |
| `flash-layout.txt`, `flash-address-map.txt` | the annotated flash address map |
| `image-identity.tsv`, `image-match-matrix.tsv` | which vendor image each dump is |
| `failing-frames-*.tsv` | the failing frame CRCs, listed |
| `flash-map.py` | regenerates all the flash artefacts, read-only |
| `basic-pack-fields.tsv` | the complete 256-byte pack field table |
| `parameter-transports.tsv` | every wire transport + the boot-image region map |
| `constants-searched.tsv` | every constant, FOUND / NOT FOUND / NEVER SEARCHED |
| `sm16269-register-map.tsv` | the 33-register chip file, both vendor tables |
| `output-stage-arithmetic.tsv` | scan/mapping numbers for this panel, verified |
| `minoe_corpus_survey.py` | the 370-file corpus survey behind the minOE result |
| `rgb96_pins.txt` | **the 96 RGB data pins**, with how they were found |
| `output_stage_16.53.txt` | the output stage traced from the pads backward |
| `led_pin_classification_16.53.txt` | LED-side pins grouped by role |
| `pad_driver_logic_16.53.tsv` | per-pad driver-LUT logic |
| `build_comparison.txt` | output-stage diff, PWM vs Normal/LS families |
| `negative_results_and_method.txt` | **searches that are dead and must not be repeated**, plus tracing-reliability limits |
| `scripts/` | every analysis script, including `repro.sh`; `scripts/netlist/` is the reusable netlist core (`netlist2.py`, `slices.py`, `lut.py`, `deep.py`) |

Large intermediates (`arcs.pkl`, `full_16.53.pkl`) are **not** committed —
regenerate them with `scripts/netbuild.py` after `scripts/repro.sh`. About a
minute per image.
