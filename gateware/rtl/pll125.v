// ---------------------------------------------------------------------------
// pll125 -- 25 MHz reference (pin P6) -> 125 MHz system clock.
//
// Generated verbatim by `ecppll -i 25 -o 125` from the installed prjtrellis
// 1.4. Kept as a checked-in file rather than a build step so the divisors are
// reviewable.
//
// The divisors it chose -- CLKI_DIV=1, CLKFB_DIV=5, CLKOP_DIV=5, VCO 625 MHz
// -- are EXACTLY the ones decoded out of the vendor bitstream in
// docs/fpga/resources.md S3. That agreement is an independent confirmation
// both of the 25 MHz reference on P6 and of the MSB-first bit order used to
// read the vendor PLL fields.
//
// If TX is ever implemented, add a second output at 125 MHz with ~+9 degrees
// of phase for RGMII TXC, which is what the vendor does (CLKOS3, CPHASE 4 /
// FPHASE 1, ~ +0.2 ns of TXC/TXD skew).
// ---------------------------------------------------------------------------
`default_nettype none

module pll125 (
    input  wire clkin,    // 25 MHz
    output wire clkout0,  // 125 MHz
    output wire locked
);

(* FREQUENCY_PIN_CLKI="25" *)
(* FREQUENCY_PIN_CLKOP="125" *)
(* ICP_CURRENT="12" *) (* LPF_RESISTOR="8" *)
(* MFG_ENABLE_FILTEROPAMP="1" *) (* MFG_GMCREF_SEL="2" *)
EHXPLLL #(
    .PLLRST_ENA("DISABLED"),
    .INTFB_WAKE("DISABLED"),
    .STDBY_ENABLE("DISABLED"),
    .DPHASE_SOURCE("DISABLED"),
    .OUTDIVIDER_MUXA("DIVA"),
    .OUTDIVIDER_MUXB("DIVB"),
    .OUTDIVIDER_MUXC("DIVC"),
    .OUTDIVIDER_MUXD("DIVD"),
    .CLKI_DIV(1),
    .CLKOP_ENABLE("ENABLED"),
    .CLKOP_DIV(5),
    .CLKOP_CPHASE(2),
    .CLKOP_FPHASE(0),
    .FEEDBK_PATH("CLKOP"),
    .CLKFB_DIV(5)
) pll_i (
    .RST(1'b0), .STDBY(1'b0),
    .CLKI(clkin), .CLKOP(clkout0), .CLKFB(clkout0), .CLKINTFB(),
    .PHASESEL0(1'b0), .PHASESEL1(1'b0), .PHASEDIR(1'b1),
    .PHASESTEP(1'b1), .PHASELOADREG(1'b1),
    .PLLWAKESYNC(1'b0), .ENCLKOP(1'b0),
    .LOCK(locked)
);

endmodule

`default_nettype wire
