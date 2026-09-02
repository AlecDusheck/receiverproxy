// ---------------------------------------------------------------------------
// eth_crc32 -- byte-parallel Ethernet FCS (CRC-32/ISO-HDLC).
//
// Standard reflected CRC-32, poly 0x04C11DB7, init 0xFFFFFFFF, input and
// output reflected, final XOR 0xFFFFFFFF. A frame including its own four FCS
// bytes leaves the residue 0xC704DD7B, which is what mac_rx checks.
//
// NOTE this is NOT the CRC used by the ECP5 bitstream container. That one is
// CRC-16 poly 0x8005, init 0, MSB-first, no reflection (see
// docs/fpga/bitstream-format.md S4). Do not confuse the two -- the bitstream
// CRC belongs to the flash-agent work, this one to the Ethernet MAC.
// ---------------------------------------------------------------------------
`default_nettype none

module eth_crc32 (
    input  wire        clk,
    input  wire        rst,      // synchronous, reloads 0xFFFFFFFF
    input  wire        valid,
    input  wire [7:0]  data,
    output reg  [31:0] crc
);
    integer b;
    reg [31:0] c;
    always @(posedge clk) begin
        if (rst) begin
            crc <= 32'hFFFFFFFF;
        end else if (valid) begin
            c = crc ^ { 24'd0, data };
            for (b = 0; b < 8; b = b + 1)
                c = c[0] ? ((c >> 1) ^ 32'hEDB88320) : (c >> 1);
            crc <= c;
        end
    end
endmodule

`default_nettype wire
