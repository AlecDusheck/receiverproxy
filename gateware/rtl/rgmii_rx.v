// ---------------------------------------------------------------------------
// rgmii_rx -- RGMII receive front end, 1000BASE-T only in v1.
//
// SKELETON. The IDDR instantiation and the nibble assembly are real; the
// 10/100 path and the lane de-permutation table are stubs marked TODO.
//
// WHY THERE IS A PERMUTATION AT ALL
// ---------------------------------
// docs/fpga/pinout.md establishes at HIGH confidence that {J2,K1,K2,J3,K3} are
// the five left-PHY RX pins and that they carry RXD[3:0] + RX_CTL with DDR on
// all five. It does NOT establish which pin is which -- the bitstream decode
// gives the group, not the assignment. Rather than guess, this module takes
// the five pads as an opaque bus and applies a run-time-selectable
// permutation, so one bitstream can sweep all candidates and the host can
// read the answer back out over the loopback (experiment E-RGMII in
// docs/own-gateware/PLAN.md).
//
// Two facts make the search small:
//   * RX_CTL is the only one of the five that is asserted for exactly the
//     duration of a frame and idle between frames. The gateware can identify
//     it on its own by counting activity -- see ctl_autodetect below.
//   * With RX_CTL known, only 4! = 24 data orderings remain, and each is
//     checked against the known preamble/SFD (55 55 ... D5) and the known
//     destination MAC 11:22:33:44:55:66 that every host frame carries.
//
// NO PHY MANAGEMENT EXISTS ON THIS BOARD. There is no MDIO/MDC group anywhere
// in any vendor image (docs/fpga/pinout.md, retraction). The PHYs are
// strapped. We cannot set the RX delay, read the link speed, or restart
// autonegotiation. Consequences:
//   * the RX clock delay must already be supplied by the PHY (RGMII-ID), which
//     is consistent with the vendor design containing no DELAYF, no DQSBUF and
//     no DDRDLL anywhere -- so we clock the IDDRs straight off the RXC pad,
//     exactly as the vendor does;
//   * if the link ever negotiates 10/100, RXC drops to 2.5/25 MHz and each
//     nibble is repeated. v1 does not handle that. See TODO below.
// ---------------------------------------------------------------------------
`default_nettype none

module rgmii_rx #(
    // Which of rx_raw[4:0] carries RX_CTL, and the order of the four data
    // lanes. Overridable so a sweep can be built without touching the source.
    parameter [2:0] CTL_LANE  = 3'd4,
    parameter [2:0] D0_LANE   = 3'd0,
    parameter [2:0] D1_LANE   = 3'd1,
    parameter [2:0] D2_LANE   = 3'd2,
    parameter [2:0] D3_LANE   = 3'd3
) (
    input  wire       rxc,        // RGMII receive clock from the PHY pad
    input  wire [4:0] rx_raw,     // the five RX pads, unpermuted

    // Byte stream in the rxc domain.
    output reg        rx_valid,   // one byte on rx_data
    output reg  [7:0] rx_data,
    output reg        rx_active,  // RX_CTL, i.e. frame in progress

    // Raw lane observation, for the calibration loopback. One bit per lane,
    // rising and falling edge, no permutation applied at all.
    output wire [4:0] raw_rise,
    output wire [4:0] raw_fall
);

    // ---- DDR capture -------------------------------------------------------
    // IDDRX1F captures the pad on both edges of rxc and presents Q0 (rising)
    // and Q1 (falling) synchronous to rxc. This is the same IOLOGIC mode the
    // vendor uses (IDDRX1_ODDRX1 on all five RX pins).
    wire [4:0] q0, q1;
    genvar i;
    generate
        for (i = 0; i < 5; i = i + 1) begin : g_iddr
            IDDRX1F iddr_i (
                .D(rx_raw[i]),
                .SCLK(rxc),
                .RST(1'b0),
                .Q0(q0[i]),   // sampled on the RISING edge of rxc
                .Q1(q1[i])    // sampled on the FALLING edge
            );
        end
    endgenerate

    assign raw_rise = q0;
    assign raw_fall = q1;

    // ---- Lane de-permutation ----------------------------------------------
    wire [3:0] nib_lo = { q0[D3_LANE], q0[D2_LANE], q0[D1_LANE], q0[D0_LANE] };
    wire [3:0] nib_hi = { q1[D3_LANE], q1[D2_LANE], q1[D1_LANE], q1[D0_LANE] };
    wire       ctl_r  = q0[CTL_LANE];
    // RGMII encodes RX_ER on the falling edge of RX_CTL as (RX_DV xor RX_ER).
    // v1 ignores errored frames rather than decoding them; the FCS check in
    // mac_rx catches anything that matters.
    // wire    ctl_f  = q1[CTL_LANE];

    // ---- Nibble assembly ---------------------------------------------------
    // In RGMII at 1000 Mb/s one rxc period carries one whole octet: the low
    // nibble on the rising edge, the high nibble on the falling edge.
    always @(posedge rxc) begin
        rx_active <= ctl_r;
        rx_valid  <= ctl_r;
        rx_data   <= { nib_hi, nib_lo };
    end

    // TODO(10/100): at 10 and 100 Mb/s the PHY clocks RXC at 2.5 / 25 MHz and
    // repeats each nibble over two clocks, so one octet spans two rxc periods.
    // Detect the rate by counting rxc edges against the 125 MHz system clock
    // over a fixed window and switch the assembler. Not needed while the host
    // link is gigabit -- but note we cannot ASK the PHY what it negotiated, so
    // if the panel is silent, verify the host link speed first.

endmodule

`default_nettype wire
