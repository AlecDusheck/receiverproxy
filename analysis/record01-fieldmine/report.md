# Record 0x01 field mining — statistical validation

Corpus: 1219 unique 764-byte record-0x01 payloads (deduped by file md5) from the vendor config tree; 974 older 508-byte payloads set aside.

## Coverage of the 764 payload bytes

- constant: 280 bytes (37%)
- near-constant: 89 bytes (12%)
- annotated-field: 237 bytes (31%)
- geometry-determined: 1 bytes (0%)
- chip-determined: 41 bytes (5%)
- unknown: 116 bytes (15%)

## Verified field hypotheses (targeted tests)

- 0x000: module width W (or a divisor of the cabinet width) — single-WxH-token files: token width == byte in 175/335, == byte*2^k in 240/335 [95 counterexamples]
- 0x001: module height / 2 — single-WxH-token files: token height == 2*byte in 248/335 [87 counterexamples]
- 0x01c: f32 gamma coefficient — values [(np.float32(2.8), 1119), (np.float32(2.0), 26), (np.float32(3.0), 26), (np.float32(2.5), 17)]; 100.0% in [1,4] or 0 (2.8 is the LEDVision default)
- 0x020: scan denominator S — == filename <N>S token in 329/334 labeled files [5 counterexamples]
- 0x021: timing, chip+scan dependent (== byte 0x4b) — equals 0x4b in 99.4% [7 counterexamples]
- 0x036: chip id low byte (with 0x204 high) — 33 distinct chip ids in corpus
- 0x04b: timing, chip+scan dependent (== byte 0x21) — equals 0x21 in 99.4% [7 counterexamples]
- 0x053: f32 = 60.0 (frame rate?) — exactly 60.0 in 1219/1219
- 0x057: paired with 0x58 (per-channel value?) — equals 0x58 in 97.6%; common values 16/0/128
- 0x058: paired with 0x57 — equals 0x57 in 97.6%
- 0x05a: 16-entry row-order map — permutation of 0..15 in 79.8%; other files hold row indices from wider ranges (89.7% all < 128) [246 counterexamples]
- 0x0aa: f32 refresh rate (Hz) — values [(np.float32(60.0), 758), (np.float32(0.0), 219), (np.float32(120.0), 145), (np.float32(240.0), 80), (np.float32(3300.0), 12)]; 100.0% in 0 or [50,10000]
- 0x0ae: f32, chip-dependent (default 1e-4) — values [(np.float32(1e-04), 1010), (np.float32(24.0), 38), (np.float32(48.0), 15), (np.float32(80.0), 13)]
- 0x0b4: f32 coefficient (duty/current-gain triple B4/B8/BC) — values [(np.float32(0.1), 613), (np.float32(0.25), 536), (np.float32(0.1007), 13), (np.float32(0.11), 11)]; 100.0% in [0,4]
- 0x0b8: f32 coefficient (duty/current-gain triple B4/B8/BC) — values [(np.float32(0.1), 607), (np.float32(0.25), 540), (np.float32(0.0), 23), (np.float32(0.3819), 13)]; 100.0% in [0,4]
- 0x0bc: f32 coefficient (duty/current-gain triple B4/B8/BC) — values [(np.float32(0.1), 414), (np.float32(0.5), 300), (np.float32(0.8169), 65), (np.float32(0.0982), 15)]; 100.0% in [0,4]
- 0x0c0: u16 MaxWidth (configured wall/card width) — (MaxW,MaxH) matches a filename WxH token in 5/5 files with >=2 tokens; MaxH multiple of module H in 1176/1219, MaxW multiple of module W in 1194/1219
- 0x0c2: u16 MaxHeight (configured wall/card height) — (MaxW,MaxH) matches a filename WxH token in 5/5 files with >=2 tokens; MaxH multiple of module H in 1176/1219, MaxW multiple of module W in 1194/1219
- 0x114: 96-entry row map (identity default) — identity 0..95 in 99.6%, all-zero in 5 files, nothing else observed
- 0x178: u16 timing (15 default) — [(np.int64(15), 1218), (np.int64(31), 1)] [1 counterexamples]
- 0x17b: u16 = 1000 (timing block constant) — 1219/1219
- 0x182: u16 PWM-chip timing (1 on non-PWM chips) — values [(np.int64(25), 1011), (np.int64(1), 187), (np.int64(50), 21)]
- 0x184: u16 PWM-chip timing (1 on non-PWM chips) — values [(np.int64(125), 1011), (np.int64(1), 187), (np.int64(30), 14), (np.int64(80), 7)]
- 0x186: u16 PWM-chip timing (1 on non-PWM chips) — values [(np.int64(125), 1011), (np.int64(1), 187), (np.int64(45), 14), (np.int64(30), 7)]
- 0x188: u16 PWM-chip timing (1 on non-PWM chips) — values [(np.int64(62), 1011), (np.int64(1), 187), (np.int64(60), 14), (np.int64(10), 7)]
- 0x18c: u16 = 1000 (timing block constant) — 1219/1219
- 0x194: u16 = 1000 (timing block constant) — 1219/1219
- 0x19a: 64-entry table: ramp 64..127 or all-zero — ramp in 79.0%, zero in 21.0%, nothing else; zero/ramp not predicted by any single other byte
- 0x1f8: f32 in {0, 0.1} — 1219/1219 files
- 0x204: chip id high byte (with 0x36 low) — 1 for 35 files (ids >= 0x100)

## Formula matches from blind scan


## Headline notes

- 0x01c f32 gamma: top values [(np.float32(2.8), 1119), (np.float32(2.0), 26), (np.float32(3.0), 26), (np.float32(2.5), 17)]
- 0x0aa f32 refresh rate: top values [(np.float32(60.0), 758), (np.float32(0.0), 219), (np.float32(120.0), 145), (np.float32(240.0), 80), (np.float32(3300.0), 12)]
- 0x0b4/0x0b8/0x0bc f32 coefficient triple: typ {0.1,0.25,0.5}; plausibly per-channel duty/gain
- 0x0c0/0x0c2 u16 MaxWidth/MaxHeight: (MaxW,MaxH) matches a filename WxH token in 5/5 files with >=2 tokens; MaxH multiple of module H in 1176/1219, MaxW multiple of module W in 1194/1219
- 0x05a..0x069 16-entry row-order map: perm of 0..15 in 80% of files; interleaved orders (e.g. 16,0,17,1,...) on high-scan panels
- 0x114..0x173 96-entry row map: identity in 99.6%, zero in 5
- 0x19a..0x1d9 64-entry table: ramp 64..127 in 79%, zero in 21% (no other pattern)
- 0x174..0x199 timing block: u16 fields; 1000 at 0x17b/0x18c/0x194 in every file; quad at 0x182/184/186/188 = (25,125,125,62) default, (1,1,1,1) on 187 files, other values chip-specific

## Unknown bytes

Ranges: 0x002, 0x018, 0x01a-0x01b, 0x023-0x024, 0x026-0x027, 0x02c, 0x02e-0x02f, 0x03d-0x03e, 0x043-0x044, 0x049, 0x050-0x051, 0x059, 0x06a-0x06b, 0x06d, 0x06f, 0x071, 0x078, 0x07b-0x07d, 0x07f, 0x081, 0x083, 0x085, 0x087, 0x089, 0x0cb-0x0cd, 0x0cf, 0x0d1, 0x0d4-0x0d5, 0x0dc, 0x0de-0x0e0, 0x0e2, 0x0e6-0x0ef, 0x0f6, 0x0fa-0x111, 0x113, 0x175, 0x17f, 0x18a, 0x190-0x191, 0x198, 0x1da, 0x1dc, 0x1e1-0x1e2, 0x1e5-0x1e6, 0x1e9-0x1eb, 0x1ee, 0x1f0, 0x200-0x202, 0x246, 0x248, 0x24b, 0x257-0x259, 0x25c-0x25e, 0x269, 0x26c-0x26d, 0x275-0x276

Most unknown bytes still sit at high (0.90+) agreement under the chip+geometry key but below the 0.95 cutoff — consistent with per-file user-tuned values (brightness, per-panel timing tweaks) layered on chip/geometry defaults, not with random noise.

## Enum-like unknown bytes (candidate settings enums)

- 0x0d5: 2 values [(0, 1195), (2, 24)], corr(pitch)=-0.08, corr(scan)=0.12, chip-agreement=1.00
- 0x0dc: 4 values [(0, 1205), (1, 9), (240, 4)], corr(pitch)=-0.00, corr(scan)=-0.03, chip-agreement=0.99
- 0x043: 5 values [(0, 1202), (2, 5), (28, 5)], corr(pitch)=0.05, corr(scan)=-0.05, chip-agreement=0.99
- 0x06a: 2 values [(128, 1201), (0, 18)], corr(pitch)=-0.17, corr(scan)=0.14, chip-agreement=0.99
- 0x018: 5 values [(0, 1196), (128, 11), (8, 6)], corr(pitch)=-0.06, corr(scan)=0.10, chip-agreement=0.98
- 0x0de: 6 values [(0, 1196), (1, 16), (16, 3)], corr(pitch)=-0.02, corr(scan)=0.02, chip-agreement=0.98
- 0x0df: 4 values [(0, 1193), (63, 16), (27, 9)], corr(pitch)=-0.08, corr(scan)=0.06, chip-agreement=0.98
- 0x191: 3 values [(0, 1192), (14, 19), (12, 8)], corr(pitch)=-0.13, corr(scan)=0.16, chip-agreement=0.98
- 0x106: 5 values [(0, 1191), (110, 19), (105, 6)], corr(pitch)=-0.10, corr(scan)=0.06, chip-agreement=0.98
- 0x10c: 5 values [(0, 1190), (96, 19), (216, 7)], corr(pitch)=-0.05, corr(scan)=0.03, chip-agreement=0.98
- 0x101: 5 values [(0, 1189), (1, 24), (92, 4)], corr(pitch)=-0.06, corr(scan)=0.01, chip-agreement=0.98
- 0x105: 7 values [(0, 1188), (92, 19), (97, 6)], corr(pitch)=-0.10, corr(scan)=0.04, chip-agreement=0.97
- 0x26c: 2 values [(0, 1186), (1, 33)], corr(pitch)=0.08, corr(scan)=-0.09, chip-agreement=0.97
- 0x03d: 4 values [(242, 1186), (114, 18), (2, 10)], corr(pitch)=-0.15, corr(scan)=0.08, chip-agreement=0.97
- 0x059: 2 values [(0, 1184), (16, 35)], corr(pitch)=0.05, corr(scan)=0.02, chip-agreement=0.97
- 0x0f6: 2 values [(128, 1178), (129, 41)], corr(pitch)=-0.18, corr(scan)=0.22, chip-agreement=0.97
- 0x175: 2 values [(0, 1170), (1, 49)], corr(pitch)=0.04, corr(scan)=-0.11, chip-agreement=0.96
- 0x10b: 5 values [(0, 1159), (2, 56), (46, 2)], corr(pitch)=-0.20, corr(scan)=-0.01, chip-agreement=0.95
- 0x113: 5 values [(0, 1156), (1, 58), (118, 3)], corr(pitch)=0.04, corr(scan)=-0.05, chip-agreement=0.95
- 0x248: 2 values [(0, 1154), (20, 65)], corr(pitch)=-0.02, corr(scan)=-0.08, chip-agreement=0.95
- 0x02c: 3 values [(2, 1152), (0, 61), (1, 6)], corr(pitch)=0.06, corr(scan)=-0.10, chip-agreement=0.95
- 0x107: 5 values [(0, 1153), (1, 58), (118, 6)], corr(pitch)=-0.02, corr(scan)=0.01, chip-agreement=0.95
- 0x02e: 3 values [(0, 1151), (2, 65), (1, 3)], corr(pitch)=-0.05, corr(scan)=0.09, chip-agreement=0.95
- 0x25c: 2 values [(0, 1139), (136, 80)], corr(pitch)=0.04, corr(scan)=0.02, chip-agreement=0.94
- 0x25d: 2 values [(0, 1139), (16, 80)], corr(pitch)=0.04, corr(scan)=0.02, chip-agreement=0.94

## Chip families in corpus (payload enum -> filename token)

- 0x00e5: 186 files, common filename token 3264
- 0x00fd: 153 files, common filename token 16380
- 0x00bb: 85 files, common filename token 16389
- 0x009e: 69 files, common filename token 9935
- 0x00c2: 66 files, common filename token 2065
- 0x0063: 65 files, common filename token 9936
- 0x0098: 60 files, common filename token 9933
- 0x0000: 58 files, common filename token 3216
- 0x0096: 54 files, common filename token 9929
- 0x0065: 48 files, common filename token 9929
- 0x000a: 46 files, common filename token 5125
- 0x0085: 45 files, common filename token 2153
- 0x009d: 33 files, common filename token 9935
- 0x00cf: 27 files, common filename token 6363
- 0x00b2: 27 files, common filename token 16237
