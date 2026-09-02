# ============================================================================
# Colorlight E120 -- own gateware build
#   yosys -> nextpnr-ecp5 -> ecppack
#
# The device is a Lattice ECP5 LFE5U-25F-6CABGA256, IDCODE 0x41111043, proven
# two ways: the vendor bitstream's ASCII header says so in plain text, and its
# VERIFY_ID command carries that exact operand.
#
#   make            build the default target (blink)
#   make TARGET=echo
#   make hub75      refused unless gateware/pins-hub75.lpf exists
#   make tools      print what is missing and how to install it
#   make clean
#
# NOTHING IN THIS MAKEFILE TOUCHES THE CARD. There is no `flash` target and
# there will not be one until docs/own-gateware/PLAN.md section 3 is settled
# and a hardware recovery path exists. Programming is a deliberate, separate,
# manual act.
# ============================================================================

TARGET      ?= blink
DEVICE      ?= 25k
PACKAGE     ?= CABGA256
SPEED       ?= 6
IDCODE      ?= 0x41111043
TOP         ?= e120_top
BUILD       ?= build

RTL := rtl/top.v rtl/pll125.v rtl/rgmii_rx.v rtl/mac_rx.v rtl/eth_crc32.v \
       rtl/frame_parser.v rtl/framebuffer.v rtl/spwm_engine.v

LPF := e120.lpf

# Per-target defines. TARGET_* are plain Verilog parameters on the top module.
DEFINES_blink := -DTARGET_BLINK=1
DEFINES_echo  := -DTARGET_ECHO=1
DEFINES_flash := -DTARGET_FLASH=1
DEFINES_hub75 := -DTARGET_HUB75=1
DEFINES        = $(DEFINES_$(TARGET))

JSON := $(BUILD)/$(TARGET).json
CFG  := $(BUILD)/$(TARGET).config
BIT  := $(BUILD)/$(TARGET).bit

.PHONY: all clean tools hub75 check-tools

all: $(BIT)

$(BUILD):
	@mkdir -p $(BUILD)

# ---- synthesis -------------------------------------------------------------
# -abc9 gives noticeably better timing on ECP5 and costs nothing here.
# Watch the memory report: the framebuffer MUST map to DP16KD block RAM. If
# yosys reports it as distributed logic the design silently balloons by
# thousands of LUTs and the fit fails for a reason that looks unrelated.
$(JSON): $(RTL) | $(BUILD)
	yosys -p 'read_verilog $(DEFINES) $(RTL); \
	          synth_ecp5 -abc9 -top $(TOP) -json $@' \
	      -l $(BUILD)/$(TARGET)-yosys.log
	@echo "--- block RAM inference check ---"
	@grep -E 'DP16KD|PDPW16KD' $(BUILD)/$(TARGET)-yosys.log || \
	  echo "  (none inferred -- fine for blink/echo, a BUG for hub75)"

# ---- place and route -------------------------------------------------------
# 125 MHz is the RGMII gigabit requirement, and it is also what the vendor
# runs: its PLL is CLKI_DIV=1, CLKFB_DIV=CLKOP_DIV=5 off a 25 MHz reference.
# Our v1 datapath is short, so this should close with room; if it does not,
# the first thing to look at is the framebuffer read path, not the MAC.
$(CFG): $(JSON) $(LPF)
	nextpnr-ecp5 --$(DEVICE) --package $(PACKAGE) --speed $(SPEED) \
	             --json $< --lpf $(LPF) --textcfg $@ \
	             --freq 125 --timing-allow-fail \
	             --report $(BUILD)/$(TARGET)-report.json \
	             -l $(BUILD)/$(TARGET)-nextpnr.log

# ---- bitstream -------------------------------------------------------------
# --idcode is passed explicitly so a wrong device is a build error rather than
# a card that silently refuses to configure.
#
# --bootaddr is NOT used here. See PLAN.md section 3: whether it means "fall
# back on failure" or "jump forward on the next reconfiguration request" is the
# single question that decides how dangerous the first flash is, and it is not
# settled from this repo's evidence. Do not add it casually.
$(BIT): $(CFG)
	ecppack --idcode $(IDCODE) --compress $< $@
	@ls -l $@

# ---- the LED build is gated -----------------------------------------------
hub75:
	@test -f pins-hub75.lpf || { \
	  echo "REFUSING TO BUILD."; \
	  echo "gateware/pins-hub75.lpf does not exist."; \
	  echo; \
	  echo "Which FPGA pads land on connector J1 is NOT KNOWN. It cannot be"; \
	  echo "derived from any bitstream -- nothing in a bitstream ties a pad to"; \
	  echo "a connector. It is resolved by experiments E-ADDR / E-CLK / E-DATA"; \
	  echo "in docs/own-gateware/PLAN.md, which produce that file."; \
	  echo; \
	  echo "Do not create it by guessing. A guessed pinmap that half-works is"; \
	  echo "the most expensive possible outcome."; \
	  exit 1; }
	$(MAKE) TARGET=hub75 all

# ---- toolchain -------------------------------------------------------------
check-tools:
	@for t in yosys nextpnr-ecp5 ecppack; do \
	  command -v $$t >/dev/null 2>&1 && echo "  ok      $$t" || echo "  MISSING $$t"; \
	done

tools: check-tools
	@echo
	@echo "prjtrellis (ecppack, ecpmulti, ecpunpack, ecppll, ecpbram) is"
	@echo "already installed via Homebrew on this machine."
	@echo
	@echo "yosys is in homebrew-core:      brew install yosys"
	@echo
	@echo "nextpnr-ecp5 is NOT in homebrew-core (only nextpnr-ice40 is)."
	@echo "Two ways to get it, in order of preference:"
	@echo
	@echo "  1. OSS CAD Suite -- one tarball, darwin-arm64, everything pinned"
	@echo "     and known to work together:"
	@echo "       https://github.com/YosysHQ/oss-cad-suite-build/releases"
	@echo "       curl -LO .../oss-cad-suite-darwin-arm64-<date>.tgz"
	@echo "       tar xzf oss-cad-suite-darwin-arm64-<date>.tgz -C \$$HOME"
	@echo "       source \$$HOME/oss-cad-suite/environment"
	@echo "     macOS will quarantine it; clear that with:"
	@echo "       xattr -dr com.apple.quarantine \$$HOME/oss-cad-suite"
	@echo "     NOTE this ships its own prjtrellis database. If you mix it with"
	@echo "     the Homebrew one, set TRELLIS_DB explicitly -- a mismatched"
	@echo "     database is a silent wrong-bitstream generator."
	@echo
	@echo "  2. Build from source against the Homebrew prjtrellis:"
	@echo "       brew install cmake boost eigen"
	@echo "       git clone https://github.com/YosysHQ/nextpnr"
	@echo "       cmake -S nextpnr -B nextpnr/build -DARCH=ecp5 \\"
	@echo "             -DTRELLIS_INSTALL_PREFIX=\$$(brew --prefix prjtrellis)"
	@echo "       cmake --build nextpnr/build -j\$$(sysctl -n hw.ncpu)"

clean:
	rm -rf $(BUILD)
