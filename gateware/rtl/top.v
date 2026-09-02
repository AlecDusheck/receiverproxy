// ---------------------------------------------------------------------------
// e120_top -- top level for our own E120 gateware.
//
// Build targets, selected by `make TARGET=...` (see the Makefile). They exist
// because the bring-up order is a risk order, not a feature order: each target
// is the smallest thing that can be flashed to answer one question.
//
//   TARGET=blink    M1. Drive nothing but the D2 signal LED via connector J19.
//                   No panel, no Ethernet output. The J19 external-interface
//                   header brings KEY+/KEY- and DATA_LED- off the board, so a
//                   blinking LED is a free, non-destructive proof that our
//                   bitstream configured, the PLL locked and the clock is the
//                   frequency we think it is -- with a webcam as the
//                   instrument and the panel unplugged.
//   TARGET=echo     M2. RGMII RX + raw nibble echo on TX. Resolves the RGMII
//                   lane permutation from the host side without a scope.
//   TARGET=flash    M3. Ethernet -> SPI flash programmer. THIS IS THE
//                   RECOVERY VEHICLE and it must work before anything is
//                   allowed to overwrite the vendor image.
//   TARGET=hub75    M5+. The real thing.
//
// Nothing here is speculative about pins. Every LOCATE in e120.lpf is taken
// from the vendor-bitstream decode; the HUB75 pins are deliberately absent
// because they are not known, and `make hub75` refuses to build without a
// pinmap file produced by the bench experiments.
// ---------------------------------------------------------------------------
`default_nettype none

module e120_top #(
    parameter TARGET_BLINK = 0,
    parameter TARGET_ECHO  = 0,
    parameter TARGET_FLASH = 0,
    parameter TARGET_HUB75 = 0
) (
    input  wire        clk25,

    // Ethernet PHY-A (left). Group membership HIGH; lane order UNVERIFIED.
    input  wire        rgmii_a_rxc,
    input  wire [4:0]  rgmii_a_rx_raw,
    output wire        rgmii_a_txc,
    output wire [4:0]  rgmii_a_tx_raw,

    // SPI configuration flash. CCLK is NOT here -- it is reached only through
    // the USRMCLK primitive, because it is a dedicated configuration pin.
    output wire        flash_csn,
    output wire        flash_mosi,
    input  wire        flash_miso,
    output wire        flash_holdn,
    output wire        flash_wpn,

    // Board straps. Six pads are tied to constants through the CIB input mux
    // at DRIVE 16 in every vendor image. What they do is NOT RESOLVED -- they
    // may be level-shifter enables or panel power gates. Reproduce the levels
    // rather than find out the hard way.
    output wire [2:0]  strap0,
    output wire [2:0]  strap1

    // TODO: hub_* and btn/led ports, once E-LED and E-DATA have resolved
    // which pads they are. Adding a port with a guessed LOCATE would be worse
    // than having no port at all.
);

    // ---- clocking ----------------------------------------------------------
    wire clk125, pll_locked;
    pll125 u_pll (.clkin(clk25), .clkout0(clk125), .locked(pll_locked));

    // Synchronous reset released a while after lock. GSR handles power-on; this
    // covers the PLL relock case.
    reg [7:0] rstcnt = 8'd0;
    reg       rst    = 1'b1;
    always @(posedge clk125) begin
        if (!pll_locked) begin
            rstcnt <= 8'd0;
            rst    <= 1'b1;
        end else if (rstcnt != 8'hFF) begin
            rstcnt <= rstcnt + 8'd1;
        end else begin
            rst <= 1'b0;
        end
    end

    assign strap0 = 3'b000;
    assign strap1 = 3'b111;

    // ---- receive path ------------------------------------------------------
    wire        rx_valid, rx_active;
    wire [7:0]  rx_data;
    wire [4:0]  raw_rise, raw_fall;

    rgmii_rx u_rgmii (
        .rxc(rgmii_a_rxc), .rx_raw(rgmii_a_rx_raw),
        .rx_valid(rx_valid), .rx_data(rx_data), .rx_active(rx_active),
        .raw_rise(raw_rise), .raw_fall(raw_fall)
    );

    wire        mrx_valid, mrx_sof, mrx_eof, mrx_good, mrx_bad;
    wire [7:0]  mrx_data;
    wire [12:0] mrx_idx;

    mac_rx u_mac (
        .clk(rgmii_a_rxc), .rst(rst),
        .rx_valid(rx_valid), .rx_data(rx_data), .rx_active(rx_active),
        .out_valid(mrx_valid), .out_data(mrx_data), .out_idx(mrx_idx),
        .out_sof(mrx_sof), .out_eof(mrx_eof),
        .out_good(mrx_good), .out_bad(mrx_bad)
    );

    // ---- M1: proof of life -------------------------------------------------
    // A ~1 Hz square wave. Divide 125 MHz by 62_500_000. Route it to whichever
    // pad experiment E-LED identifies as the D2 signal indicator; until then
    // it drives nothing and exists so the timing and the divider can be
    // reviewed.
    reg [26:0] heartbeat = 27'd0;
    always @(posedge clk125) heartbeat <= heartbeat + 27'd1;
    wire led_1hz = heartbeat[26];   // 125e6 / 2^27 = 0.93 Hz
    // verilator lint_off UNUSED
    wire _unused_ok = &{1'b0, led_1hz, mrx_valid, mrx_data, mrx_idx, mrx_sof,
                        mrx_eof, mrx_good, mrx_bad, raw_rise, raw_fall,
                        flash_miso, 1'b0};
    // verilator lint_on UNUSED

    // ---- M2: raw nibble echo ----------------------------------------------
    // TODO. Transmit back, on the five TX pads in pad order, exactly the five
    // RX pads' captured values in pad order, with no de-permutation applied at
    // either end. The host then sends frames whose nibbles walk a single bit
    // and reads which lane it came back on, recovering the composed RX*TX
    // permutation directly. That is the whole calibration, and it needs no
    // scope and no panel.

    // ---- M3: flash agent ---------------------------------------------------
    // TODO. See PLAN.md S3. This is the module that makes everything else
    // safe, so it is written and proven BEFORE the vendor image is touched.
    assign flash_csn   = 1'b1;
    assign flash_mosi  = 1'b0;
    assign flash_holdn = 1'b1;
    assign flash_wpn   = 1'b1;

    // ---- TX ----------------------------------------------------------------
    assign rgmii_a_txc    = 1'b0;
    assign rgmii_a_tx_raw = 5'd0;

    // ---- M5: the panel -----------------------------------------------------
    // TODO. framebuffer + frame_parser + spwm_engine, gated on TARGET_HUB75.

endmodule

`default_nettype wire
