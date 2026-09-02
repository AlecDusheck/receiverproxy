// ---------------------------------------------------------------------------
// spwm_engine -- SM16269S serial protocol: config writes, grey upload, VSYNC,
// free-running RCLK, and the A..E row scan.
//
// SKELETON. The state machine below is the shape of the real thing with the
// command primitive and the upload nesting written out; the numbers that are
// not yet known are collected in one parameter block at the top so a bench
// sweep is a `-D` on the yosys command line rather than an edit.
//
// ===========================================================================
// WHAT THIS CHIP ACTUALLY IS -- read this before changing anything
// ===========================================================================
// The SM16269S is NOT a shift register with a latch. It is a self-scanning
// S-PWM driver: 16-bit shift register -> 8 Kbit SRAM -> sixteen SM-PWM
// processors, with RCLK feeding a 16-bit counter that drives the PWM
// controller. Consequences that break every HUB75 habit:
//
//   1. THERE IS NO OE PIN. The 24-pin part is GND, SDI, DCLK, LE, OUT0..15,
//      RCLK, SDO, REXT, VDD. On a HUB75 panel the connector's OE wire is
//      routed to the panel's RCLK. So OE must carry a CONTINUOUS PULSE TRAIN,
//      not a blanking level. Hold it static and the panel shows power-up SRAM
//      noise forever, no matter how correct the data is.
//   2. RCLK IS THE GREY CLOCK AND THE ROW ADVANCE AT ONCE. It must free-run
//      during uploads. The open-source reference runs it on a separate pinned
//      thread specifically so a frame upload cannot stall it, and the sibling
//      SM16380 corpus records a verified hardware failure when the grey clock
//      was held low during upload.
//   3. THE ROW IS IMPLICIT IN UPLOAD ORDER, not addressed. A..E go to the
//      module's own row decoder (an SM5166-class part), and the chip advances
//      its OWN row pointer from RCLK. Those two counters must stay in phase.
//      If they drift, every physical row shows the same SRAM row -- which is
//      exactly the symptom this project has been chasing, and it coexists
//      happily with "gain writes work" and "brightness scales current",
//      because those are command-channel behaviour that never touches the row
//      pointer.
//   4. COMMANDS ARE SELECTED BY LE PULSE COUNT, not by an opcode. The
//      datasheet says so in as many words ("LE: data latch control; issues
//      control commands in conjunction with DCLK") and then omits the table.
//   5. R/G/B ARE SIX PARALLEL LANES, not a serialised stream. A register
//      write puts the SAME register address with THREE DIFFERENT per-colour
//      values on the R, G and B lanes simultaneously. That is how a
//      per-colour register file is reached over a HUB75 bus.
//
// ===========================================================================
// COMMAND TAILS -- from the vendor's own parameter block
// ===========================================================================
// SChipControl, record 0x01 +0x0C4, for chip id 0x014C (this panel), as
// shipped in the seller's own .rcvbp:
//
//   00 0e 01 05 06 01 03 00 00 00 00 97 00 97 00 08 02 00 0a 02
//      ^^ 14  pre-activation / unlock tail       HIGH
//         ^^ 1   protocol variant selector       MEDIUM
//            ^^ 5   config-register write tail   HIGH
//               ^^ 6   second command tail       UNKNOWN, do not emit
//                  ^^ 1   data-latch tail        HIGH
//                     ^^ 3   VSYNC tail          HIGH
//                              ^^^^^ ^^^^^ 151, 151 -- "scan cycle level",
//                                    i.e. the RCLK-per-row count.  LOW.
//
// The decode is corroborated four ways: the SM16380 open-source command enum
// is literally PREACTIVE=14, CFG1=4, CFG2=8, VSYNC=3 and its corpus entry
// carries bytes 14/4/8/.../3; the DP3265S profile is 13 addressed registers
// all with tail 5 and its corpus entry carries 5/5 with exactly 13 registers;
// the block is all-zero for exactly the non-S-PWM (plain shift register)
// chips; and the GCLK column across the corpus is a clean (1024 >> n) + small
// ladder.
// ===========================================================================
`default_nettype none

module spwm_engine #(
    // ---- geometry, HIGH ----
    parameter WIDTH        = 128,
    parameter HALF_H       = 32,
    parameter SCAN         = 16,     // 1/16 duty
    parameter CHAIN        = 8,      // SM16269S per lane per half:
                                     //   128 columns / 16 outputs = 8
    parameter WORD_BITS    = 16,     // chip shift register width, MSB first

    // ---- command tails, from SChipControl ----
    parameter T_PREACT     = 14,
    parameter T_CFG        = 5,
    parameter T_DATA       = 1,
    parameter T_VSYNC      = 3,

    // ---- UNKNOWNS. Every one of these is a bench sweep. ----
    // RCLK pulses per row. 151 is the vendor's computed "scan cycle level" for
    // this panel's reg 0x07 = 0x04 and sub-id 0x0000, and the formula that
    // produces it is recovered and verified -- but nothing confirms that the
    // number means what we think it means. Sweep it. (E-RCLK)
    parameter RCLK_PER_ROW = 151,
    // DCLK divider off the 125 MHz system clock. The datasheet gives 25 MHz in
    // the dynamic characteristics and 30 MHz in the absolute maxima; design
    // for 25. /6 = 20.8 MHz, /8 = 15.6 MHz. Start slow. (E-DCLK)
    parameter DCLK_DIV     = 8,
    // Does a pre-activation precede EVERY config write, or is it sent once at
    // the start of the whole init sequence? The vendor block carries the 14
    // and the sibling protocol doc says "each is preceded by 14", but the only
    // reference implementation that DEMONSTRABLY DROVE A PANEL sends it once.
    // Try once first. (E-PREACT)
    parameter PREACT_EVERY = 0,
    // Does the 5-clock config tail OVERLAP the last five payload bits, or
    // follow the payload as separate clocks? The working reference overlaps;
    // the abandoned SM16269S-specific code in the same tree did not. These are
    // different waveforms. (E-TAIL)
    parameter TAIL_OVERLAP = 1,
    // Phase between the A..E row change and the chip's internal row rollover.
    // Nothing anywhere specifies this. (E-PHASE)
    parameter ROW_PHASE    = 0
) (
    input  wire        clk,          // 125 MHz system clock
    input  wire        rst,
    input  wire        start_frame,  // from the 0x0107 latch frame

    // Framebuffer read port. Six 8-bit lanes packed: fb_rd[8*n +: 8] is lane
    // n, order R1 G1 B1 R2 G2 B2.
    output reg  [11:0] fb_addr,
    input  wire [47:0] fb_rd,

    // Chip register file: 33 entries of (addr, R, G, B). Loaded from flash or
    // from a send-params frame; see PLAN.md M5.
    input  wire [7:0]  reg_addr,
    input  wire [7:0]  reg_r,
    input  wire [7:0]  reg_g,
    input  wire [7:0]  reg_b,
    output reg  [5:0]  reg_index,
    input  wire        reg_valid,

    // HUB75 pads. NOTE the naming: hub_oe is the connector's OE wire and it
    // carries RCLK. It is not an output enable and it is never held static.
    output reg  [5:0]  hub_rgb,      // R1 G1 B1 R2 G2 B2
    output reg         hub_clk,      // DCLK
    output reg         hub_lat,      // LE
    output reg         hub_oe,       // RCLK  <-- pulse train
    output reg  [4:0]  hub_addr      // A B C D E
);

    // =======================================================================
    // RCLK / row scan -- a completely independent process.
    // =======================================================================
    // Deliberately NOT sequenced with the upload. This mirrors the reference
    // implementation's separate pinned thread, and it is the whole point: the
    // display scan must not be able to stall.
    reg [15:0] rclk_div;
    reg [15:0] rclk_cnt;
    reg [4:0]  row;

    always @(posedge clk) begin
        if (rst) begin
            rclk_div <= 16'd0; rclk_cnt <= 16'd0; row <= 5'd0; hub_oe <= 1'b0;
        end else begin
            rclk_div <= rclk_div + 16'd1;
            if (rclk_div == (DCLK_DIV/2 - 1)) begin
                rclk_div <= 16'd0;
                hub_oe   <= ~hub_oe;
                if (hub_oe) begin                  // count falling edges
                    if (rclk_cnt == RCLK_PER_ROW - 1) begin
                        rclk_cnt <= 16'd0;
                        row      <= (row == SCAN-1) ? 5'd0 : row + 5'd1;
                    end else begin
                        rclk_cnt <= rclk_cnt + 16'd1;
                    end
                end
            end
            hub_addr <= { {(5-4){1'b0}}, row[3:0] };  // 1/16 -> A..D, E = 0
        end
    end

    // TODO(E-PHASE): ROW_PHASE must offset the A..E change relative to the
    // rclk_cnt rollover. Implement it as a signed skew on the comparison, and
    // sweep it across a full row period. Wrong phase = every physical row
    // shows the same SRAM row, which looks exactly like "no data arrived".

    // =======================================================================
    // Command primitive: LE high across exactly N DCLK rising edges.
    // =======================================================================
    //   RGB := 0; LE := 0; DCLK := 0; LE := 1
    //   repeat N { DCLK^; DCLKv }
    //   LE := 0
    //   repeat SPACER { DCLK^; DCLKv }
    // Timing floor from the datasheet: tSU1/tSU2 (LE to DCLK rising) 10 ns
    // min, tH1 (DCLK rising to LE falling) 10 ns min, tSU0/tH0 (data) 5 ns.
    // At a 15-25 MHz DCLK with a half-period of 20-33 ns those are met with a
    // wide margin by driving LE and RGB on the DCLK falling edge.

    // =======================================================================
    // Config write: (addr << 8) | value, 16 bits MSB first, per chip.
    // =======================================================================
    // for chip in 0 .. CHAIN-1:
    //   for bit in 15 downto 0:
    //     if TAIL_OVERLAP && last_chip && bit < T_CFG:  LE := 1
    //     R_lane := bit of ((addr<<8)|value_r)
    //     G_lane := bit of ((addr<<8)|value_g)     <-- three DIFFERENT values
    //     B_lane := bit of ((addr<<8)|value_b)         of the SAME register
    //     DCLK^; DCLKv
    //   LE := 0
    //
    // The 33 registers to write, in order 0x02,0x03,...,0x20,0x22,0xF0, are
    // the vendor table in config/chips/sm16269.toml. Two independently derived
    // sources agree on it byte-for-byte -- a decompiled vendor DLL and an
    // open-source bit-banger -- which is the strongest single result available
    // about this chip. Of those, three are computed rather than copied:
    //   0x02 [5:0] = scan - 1 = 15        (the chip is told its scan depth)
    //   0x03 [7:6] and 0x07 [4:3] set the grey depth:
    //        g = 128 << ((r07>>3)&3), m = (r03 < 0x40) ? 64 : 32, total = m*g
    //        ours: 128 * 64 = 8192 -> 14-bit, matching the module datasheet
    //   0x16 [5:0] = current gain, and this is where brightness goes
    //
    // ORDER OF OPERATIONS AT POWER ON, and it matters for the 5.1 A supply:
    //   1. RCLK free-running, A..E scanning       (before anything else)
    //   2. VSYNC (tail 3)
    //   3. write 0x16 (gain) LOW
    //   4. upload a BLACK frame
    //   5. write the remaining 32 registers
    //   6. only then raise the gain
    // An armed panel showing unmodulated content already draws ~4.5 A on this
    // bench and rails the limit at full brightness.

    // =======================================================================
    // Grey upload
    // =======================================================================
    // for channel in 0 .. 15:                 <-- chip OUTPUT index, outer
    //   for chip in 0 .. CHAIN-1:             <-- chip index, inner
    //     for bit in 15 downto 0:
    //       six lanes := that pixel's grey bit
    //       DCLK^; DCLKv
    //   LE := 1; DCLK^; DCLKv; LE := 0        <-- 1-clock data latch, on the
    //                                             LAST chip only, once per
    //                                             output-index group
    // then VSYNC (tail 3)
    //
    // OUTPUT-MAJOR, CHIP-MINOR. Reversing the nesting produces "scrambled
    // 16-pixel rectangles", which the reference bring-up notes call out by
    // name -- a useful signature to recognise in a photograph.
    //
    // ARITHMETIC CHECK against the card's own tables, which agrees exactly:
    //   OneScanLen = W * (H/2) / scan = 128 * 32 / 16 = 256 slots per address
    //   256 slots = 16 chips-worth * 16 outputs on a 128-wide half at 1/16
    //   so the card's "slot" index IS the chip-output index, and one 256-slot
    //   pass is one full 16-bit-word sweep of the chain.
    // (Note CHAIN=8 per lane per half here, with two halves on the two RGB
    // groups; the 16 above is the combined figure the vendor tables use.)
    //
    // 8 -> 16 BIT EXPANSION. The framebuffer stores 8 bits per channel because
    // that is all the wire carries. The chip wants 16. Left-justify
    // ({pix, 8'h00}) as a first cut; replace with a gamma ROM once anything
    // renders. Do NOT do this before the panel works -- a wrong gamma and a
    // wrong protocol look the same in a photograph.
    //
    // THROUGHPUT. Per row address: 16 outputs * 8 chips * 16 bits = 2048 DCLK.
    // Times 16 rows = 32768 DCLK per frame. At a /8 DCLK of 15.6 MHz that is
    // 2.1 ms, i.e. 476 full uploads per second -- far more than the 60 Hz the
    // host sends, and the chip's own S-PWM supplies the >=3840 Hz visual
    // refresh from RCLK. There is no throughput problem anywhere in this
    // design, which is worth knowing before anyone optimises.

    // =======================================================================
    // State machine -- TODO
    // =======================================================================
    localparam [3:0] S_RESET   = 4'd0,
                     S_VSYNC0  = 4'd1,
                     S_PREACT  = 4'd2,
                     S_CFG     = 4'd3,
                     S_IDLE    = 4'd4,
                     S_UPLOAD  = 4'd5,
                     S_LATCH   = 4'd6,
                     S_VSYNC   = 4'd7;

    reg [3:0] state;
    always @(posedge clk) begin
        if (rst) begin
            state    <= S_RESET;
            hub_rgb  <= 6'd0;
            hub_clk  <= 1'b0;
            hub_lat  <= 1'b0;
            fb_addr  <= 12'd0;
            reg_index<= 6'd0;
        end else begin
            // TODO(M5): implement. Keep the command primitive as a shared
            // sub-sequencer so the tail length is a register, not a case arm --
            // every unknown in the parameter block above is then sweepable
            // from one bitstream driven by a host-set register.
            state <= state;
        end
    end

endmodule

`default_nettype wire
