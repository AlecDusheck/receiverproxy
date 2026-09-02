// ---------------------------------------------------------------------------
// framebuffer -- 128 x 64 x RGB, stored 8 bits per channel, in EBR.
//
// WHY 8 BITS AND NOT 16
// ---------------------
// The wire format carries 8 bits per channel and nothing more: CLTNic copies
// the low three bytes of a 32-bit source pixel, verbatim, 3 bytes per pixel
// (docs/pixel-protocol.md S1.7, HIGH). Storing 16 bits would double the memory
// for zero information. The SM16269S wants a 16-bit word per channel, so the
// expansion happens on the way OUT, in spwm_engine, where a gamma or
// left-justify choice is one line rather than a memory-layout decision.
//
//   stored single buffer : 128*64*3*8  = 196 608 bits = 19 % of the 25F's EBR
//   double buffered      : 393 216 bits = 39 %
//   if we had stored 16b : 786 432 bits = 78 %, and no room to breathe
//
// For reference the vendor design uses essentially ALL 1008 Kbit across 53 of
// 56 EBRs -- but it serves twelve connectors and two PHYs. One connector is
// cheap.
//
// LAYOUT
// ------
// Six independent banks, one per HUB75 lane:
//   bank 0,1,2 = R1,G1,B1  -- the UPPER half of the module (y = 0..31)
//   bank 3,4,5 = R2,G2,B2  -- the LOWER half (y = 32..63)
// Each bank is 4096 x 8, addressed by (y mod 32) * 128 + x.
//
// This split is not arbitrary: a 128x64 HUB75 module is driven as two vertical
// halves on the two RGB groups, and the card's own mapping record stores only
// HALF the module height for exactly that reason (docs/fpga/output-stage.md
// S2, HIGH). It also makes the write path trivial -- one pixel write touches
// three of the six banks, never two banks of the same colour -- and the read
// path trivial: the serialiser reads all six banks at the same address and
// gets one complete HUB75 column.
//
// SKELETON: single-buffered. Add the `bank` bit to both addresses and a swap
// on the 0x0107 latch frame for milestone M4.
// ---------------------------------------------------------------------------
`default_nettype none

module framebuffer #(
    parameter WIDTH  = 128,
    parameter HALF_H = 32,
    parameter AW     = 12          // clog2(128*32)
) (
    // Write port -- rxc domain in v1. See the note on clock domains below.
    input  wire            wclk,
    input  wire            we,
    input  wire [AW-1:0]   waddr,
    input  wire            whalf,      // 0 = upper banks, 1 = lower banks
    input  wire [7:0]      wr,
    input  wire [7:0]      wg,
    input  wire [7:0]      wb,

    // Read port -- system clock domain. Six lanes packed into one bus rather
    // than an unpacked array port: unpacked ports are SystemVerilog, and this
    // file should read cleanly under plain `read_verilog` with no -sv.
    // rd[8*n +: 8] is lane n, order R1 G1 B1 R2 G2 B2.
    input  wire            rclk,
    input  wire [AW-1:0]   raddr,
    output reg  [47:0]     rd
);

    // Six 4096 x 8 memories. yosys infers ECB/DP16KD block RAM from this shape
    // on ECP5; check the `Number of cells` report and confirm DP16KD appears,
    // because a mis-inferred memory silently becomes thousands of LUTs and
    // will not fit alongside anything else.
    reg [7:0] mem0 [0:(WIDTH*HALF_H)-1];
    reg [7:0] mem1 [0:(WIDTH*HALF_H)-1];
    reg [7:0] mem2 [0:(WIDTH*HALF_H)-1];
    reg [7:0] mem3 [0:(WIDTH*HALF_H)-1];
    reg [7:0] mem4 [0:(WIDTH*HALF_H)-1];
    reg [7:0] mem5 [0:(WIDTH*HALF_H)-1];

    always @(posedge wclk) begin
        if (we && !whalf) begin
            mem0[waddr] <= wr;
            mem1[waddr] <= wg;
            mem2[waddr] <= wb;
        end
        if (we && whalf) begin
            mem3[waddr] <= wr;
            mem4[waddr] <= wg;
            mem5[waddr] <= wb;
        end
    end

    always @(posedge rclk) begin
        rd[ 7: 0] <= mem0[raddr];
        rd[15: 8] <= mem1[raddr];
        rd[23:16] <= mem2[raddr];
        rd[31:24] <= mem3[raddr];
        rd[39:32] <= mem4[raddr];
        rd[47:40] <= mem5[raddr];
    end

    // CLOCK DOMAINS. Writes arrive on the PHY's recovered receive clock and
    // reads happen on the PLL system clock. A true dual-port EBR handles that
    // natively -- and it is exactly what the vendor does: EXACTLY two of its
    // block RAMs have CLKA != CLKB, one per PHY, and they are the design's
    // only clock-domain crossings (docs/fpga-gateware.md, HIGH). There is no
    // metastability hazard on the data itself because a given address is never
    // read and written in the same cycle by design; the hazard is on the
    // CONTROL handshake (frame-complete), which must go through a proper
    // synchroniser. See frame_parser.
    //
    // Alternative worth considering for M2: put a small async FIFO right after
    // mac_rx and run the whole parser on the system clock. It costs one EBR
    // and removes every remaining CDC question from the design. Recommended if
    // the receive path ever misbehaves in a way that is not reproducible.

endmodule

`default_nettype wire
