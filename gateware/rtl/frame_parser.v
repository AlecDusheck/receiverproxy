// ---------------------------------------------------------------------------
// frame_parser -- decode the Colorlight wire protocol into framebuffer writes.
//
// The format is NOT invented here. It is byte-exact against Colorlight's own
// sender DLL, recovered by static reading of CLTNic.dll and recorded in
// docs/pixel-protocol.md at HIGH confidence with three independent
// confirmations (the compile-time frame template, the per-field patch sites,
// and two separate length computations). Keeping it means `e120-cli` --
// image, play, fill, brightness -- works against our gateware unchanged, which
// is worth a great deal during bring-up: the host side is already known good
// and stops being a variable.
//
//   offset  size          meaning
//   ------  ----          -------
//    0..5    6            destination MAC 11 22 33 44 55 66
//    6..11   6            source MAC      22 22 33 44 55 66
//   12       1            type: 0x55 pixel row / 0x01 latch / 0x0A brightness
//   -- type 0x55 (pixel row) --
//   13..14   2   BE       row index. NOTE this is base(screen) + y, where
//                         base = (screen-1) << 12 for screen <= 9. Screen 1
//                         gives base 0, so row == y. We accept the low 12 bits
//                         and ignore the screen selector in v1.
//   15..16   2   BE       xoff, first pixel index in the row
//   17..18   2   BE       count, pixels in this packet (max 497)
//   19..20   2            08 88, fixed marker
//   21..     3*count      pixel bytes, 3 per pixel
//   -- type 0x01 0x07 (display/latch, 112 bytes) --
//   35       1            master brightness
//   38..40   3            per-channel gain
//   -- type 0x0A (brightness, 77 bytes) --
//   13..15   3            per-channel brightness
//
// Two rules that cost nothing to obey and have already burned this project:
//   * The vendor emits the latch frame FIRST in each burst, so it latches the
//     PREVIOUS frame's rows. Over a continuous stream that is equivalent to
//     rows-then-latch; it only differs on the very first burst.
//   * A row wider than 497 pixels arrives as several packets with different
//     xoff. At 128 wide that never happens, but do not assume one packet per
//     row in code.
//
// SKELETON: the 0x55 path is written out; 0x01/0x0A are stubs.
// ---------------------------------------------------------------------------
`default_nettype none

module frame_parser #(
    parameter WIDTH  = 128,
    parameter HEIGHT = 64,
    parameter AW     = 12
) (
    input  wire        clk,
    input  wire        rst,

    input  wire        in_valid,
    input  wire [7:0]  in_data,
    input  wire [12:0] in_idx,
    input  wire        in_eof,
    input  wire        in_good,

    // Framebuffer write port.
    output reg         fb_we,
    output reg [AW-1:0] fb_addr,
    output reg         fb_half,
    output reg [7:0]   fb_r,
    output reg [7:0]   fb_g,
    output reg [7:0]   fb_b,

    // Control, latched only on a frame with a good FCS.
    output reg         latch_pulse,     // one cycle, on a good 0x01 0x07 frame
    output reg [7:0]   brightness,
    output reg [7:0]   gain_r,
    output reg [7:0]   gain_g,
    output reg [7:0]   gain_b
);

    localparam [7:0] T_PIXEL = 8'h55;
    localparam [7:0] T_CTRL  = 8'h01;
    localparam [7:0] T_BRIGHT= 8'h0A;

    reg [7:0]  ftype;
    reg [15:0] row_raw, xoff, count;
    reg [1:0]  ph;           // pixel phase: 0=R 1=G 2=B  (see colour note)
    reg [15:0] pix_i;        // pixel index within this packet
    reg [7:0]  hold_r, hold_g;

    // COLOUR ORDER. CLTNic copies the low three bytes of a 32-bit GDI surface,
    // whose memory order is B,G,R,A -- so the wire order is almost certainly
    // BGR. docs/pixel-protocol.md rates that MEDIUM (it is a property of the
    // caller, not fixed inside CLTNic), and our own CLI has a configurable
    // ColorOrder. Do not hard-code it here without a bench check: a swapped
    // red and blue is one of the few faults that a photograph settles in one
    // shot. Parameterise it before M4.
    localparam WIRE_IS_BGR = 1'b1;

    wire [11:0] row  = row_raw[11:0];              // screen selector discarded
    wire        half = row[5];                     // y >= 32 -> lower group
    wire [15:0] xpos = xoff + pix_i;

    always @(posedge clk) begin
        fb_we       <= 1'b0;
        latch_pulse <= 1'b0;

        if (rst) begin
            ftype <= 8'h00;
            ph    <= 2'd0;
            pix_i <= 16'd0;
        end else begin
            if (in_valid) begin
                case (in_idx)
                    13'd12: begin ftype <= in_data; ph <= 2'd0; pix_i <= 16'd0; end
                    13'd13: row_raw[15:8] <= in_data;
                    13'd14: row_raw[7:0]  <= in_data;
                    13'd15: xoff[15:8]    <= in_data;
                    13'd16: xoff[7:0]     <= in_data;
                    13'd17: count[15:8]   <= in_data;
                    13'd18: count[7:0]    <= in_data;
                    default: ;
                endcase

                // ---- pixel payload ----
                if (ftype == T_PIXEL && in_idx >= 13'd21) begin
                    case (ph)
                    2'd0: begin hold_r <= in_data; ph <= 2'd1; end
                    2'd1: begin hold_g <= in_data; ph <= 2'd2; end
                    2'd2: begin
                        ph <= 2'd0;
                        // Drop anything outside the panel rather than wrapping
                        // it. A wrapped write is how a geometry mismatch turns
                        // into "the panel shows structured garbage" instead of
                        // "the panel shows nothing", which is much harder to
                        // diagnose. Silence is the more useful failure.
                        if (row < HEIGHT && xpos < WIDTH) begin
                            fb_we   <= 1'b1;
                            fb_half <= half;
                            fb_addr <= { row[4:0], xpos[6:0] };
                            if (WIRE_IS_BGR) begin
                                fb_b <= hold_r;   // first byte on the wire
                                fb_g <= hold_g;
                                fb_r <= in_data;
                            end else begin
                                fb_r <= hold_r;
                                fb_g <= hold_g;
                                fb_b <= in_data;
                            end
                        end
                        pix_i <= pix_i + 16'd1;
                    end
                    default: ph <= 2'd0;
                    endcase
                end

                // ---- brightness / latch payload ----
                // TODO(M4): capture offsets 35 and 38..40 for the 0x01 0x07
                // frame and 13..15 for the 0x0A frame, and hold them until
                // in_good. Do NOT act on them mid-frame -- a frame that fails
                // its FCS must change nothing.
            end

            if (in_eof && in_good) begin
                if (ftype == T_CTRL) latch_pulse <= 1'b1;
            end
        end
    end

    // TODO(M4): brightness must reach the panel as the SM16269S GAIN register
    // 0x16 [5:0], not as an OE duty cycle -- this chip has no OE pin. And the
    // gain field is NOT linear in the register value. The datasheet gives
    // IOUT = 19400/Rext * G with, reconstructed from the two stated endpoints
    // (12.5 % and 193 %) and checked against the vendor default 0x30 -> G = 1:
    //     G = 2^(2*G5 + G4)/8 * (1 + (8*G3 + 4*G2 + 2*G1 + G0)/16)
    // A linear percent-to-gain map gives a badly skewed brightness curve.
    // Build a small 256-entry ROM instead. See docs/own-gateware/PLAN.md S5.

endmodule

`default_nettype wire
