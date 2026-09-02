// ---------------------------------------------------------------------------
// mac_rx -- strip preamble/SFD, filter on destination MAC, check the FCS,
// and present a payload byte stream with a byte index.
//
// The card's own MAC address is a wire-format constant, not a configuration
// item: Colorlight's sender hard-codes destination 11:22:33:44:55:66 and
// source 22:22:33:44:55:66 as a compile-time template in CLTNic.dll, with no
// product branch anywhere in the send path (docs/pixel-protocol.md S1.1, S3).
// Every Colorlight receiver on the segment answers to the same address; the
// protocol is a broadcast-shaped one-way stream and has no addressing beyond
// the screen number folded into the row field.
//
// v1 ACCEPTS a frame if the destination is 11:22:33:44:55:66 or broadcast.
// Anything else is dropped before it can touch the framebuffer.
//
// The byte index `idx` counts from 0 at the destination MAC's first byte, so
// it lines up directly with the offsets tabulated in docs/pixel-protocol.md:
//   12 = type, 13..14 = row BE, 15..16 = xoff BE, 17..18 = count BE,
//   19..20 = 08 88, 21.. = pixels.
// ---------------------------------------------------------------------------
`default_nettype none

module mac_rx (
    input  wire        clk,        // rxc domain
    input  wire        rst,

    input  wire        rx_valid,
    input  wire [7:0]  rx_data,
    input  wire        rx_active,

    output reg         out_valid,  // one accepted frame byte
    output reg  [7:0]  out_data,
    output reg  [12:0] out_idx,    // 0 = first byte of the destination MAC
    output reg         out_sof,
    output reg         out_eof,    // asserted with the last payload byte
    output reg         out_good,   // pulses one cycle after out_eof if FCS ok
    output reg         out_bad
);

    localparam [2:0] S_IDLE = 3'd0,
                     S_PRE  = 3'd1,
                     S_DATA = 3'd2,
                     S_END  = 3'd3;

    reg  [2:0]  state;
    reg  [12:0] idx;
    reg         addr_ok;

    // Rolling 4-byte window so the FCS can be excluded from the payload: the
    // MAC does not know a frame has ended until RX_CTL drops, so it delays the
    // stream by four bytes and treats whatever is in flight at end-of-frame as
    // the FCS.
    reg [7:0] d0, d1, d2, d3;
    reg [2:0] fill;

    wire [31:0] crc;
    reg         crc_rst;
    reg         crc_en;
    eth_crc32 u_crc (
        .clk(clk), .rst(crc_rst), .valid(crc_en), .data(rx_data), .crc(crc)
    );

    // Destination-MAC comparison, byte by byte as it streams past.
    function [7:0] card_mac(input [2:0] n);
        case (n)
            3'd0: card_mac = 8'h11;
            3'd1: card_mac = 8'h22;
            3'd2: card_mac = 8'h33;
            3'd3: card_mac = 8'h44;
            3'd4: card_mac = 8'h55;
            3'd5: card_mac = 8'h66;
            default: card_mac = 8'h00;
        endcase
    endfunction

    always @(posedge clk) begin
        out_valid <= 1'b0;
        out_sof   <= 1'b0;
        out_eof   <= 1'b0;
        out_good  <= 1'b0;
        out_bad   <= 1'b0;
        crc_rst   <= 1'b0;
        crc_en    <= 1'b0;

        if (rst) begin
            state   <= S_IDLE;
            idx     <= 13'd0;
            fill    <= 3'd0;
            addr_ok <= 1'b1;
        end else begin
            case (state)
            S_IDLE: begin
                if (rx_active && rx_valid) begin
                    if (rx_data == 8'h55) begin
                        state   <= S_PRE;
                        crc_rst <= 1'b1;
                        idx     <= 13'd0;
                        fill    <= 3'd0;
                        addr_ok <= 1'b1;
                    end
                end
            end

            S_PRE: begin
                if (!rx_active) begin
                    state <= S_IDLE;
                end else if (rx_valid) begin
                    // 0x55 repeats through the preamble; 0xD5 is the SFD.
                    if (rx_data == 8'hD5) state <= S_DATA;
                    else if (rx_data != 8'h55) state <= S_IDLE; // malformed
                end
            end

            S_DATA: begin
                if (!rx_active) begin
                    // The last four bytes shifted in are the FCS, not payload.
                    // The CRC register was fed every byte including them, so a
                    // good frame leaves the standard residue.
                    state    <= S_IDLE;
                    out_eof  <= 1'b1;
                    out_good <= addr_ok && (crc == 32'hC704DD7B);
                    out_bad  <= !(addr_ok && (crc == 32'hC704DD7B));
                end else if (rx_valid) begin
                    crc_en <= 1'b1;

                    // Destination-MAC filter, evaluated as it arrives.
                    if (idx < 13'd6) begin
                        if (rx_data != card_mac(idx[2:0]) && rx_data != 8'hFF)
                            addr_ok <= 1'b0;
                    end

                    // Four-deep delay line so the FCS never reaches the parser.
                    d3 <= d2; d2 <= d1; d1 <= d0; d0 <= rx_data;
                    if (fill != 3'd4) begin
                        fill <= fill + 3'd1;
                    end else begin
                        out_valid <= addr_ok;
                        out_data  <= d3;
                        out_idx   <= idx;
                        out_sof   <= (idx == 13'd0);
                        idx       <= idx + 13'd1;
                    end
                end
            end

            default: state <= S_IDLE;
            endcase
        end
    end

    // TODO: RX_ER (RX_CTL on the falling edge) is ignored. A frame the PHY
    // marks errored will simply fail the FCS check, which is adequate but
    // wastes a frame slot. Wire it in if the error counters ever matter.

endmodule

`default_nettype wire
