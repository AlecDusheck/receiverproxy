"""The card's EEPROM record map, (address, length, description), from docs/eeprom-map.md.

Shared by flash-review.py and eeprom-restore.py. Lengths are load-bearing: the card
ignores a write that spans record boundaries. Must agree with
e120_proto::eeprom::RECORDS for addresses <= 0x0fd; 0x118 and 0x127 (opcodes
0x45/0x88) are listed for labelling only.
"""

EEPROM = [
    (0x000, 2, 'debug bytes'),
    (0x002, 42, 'CONTROL AREA (startX, startY, endX, endY) — an erased one drops every pixel'),
    (0x02c, 18, 'colour-gamut coefficients'),
    (0x03e, 1, 'gamut-adjust enable'),
    (0x040, 1, 'calibration status'),
    (0x041, 1, '"no input" show info'),
    (0x042, 1, 'turn-on screen show'),
    (0x043, 3, 'white-balance adjust'),
    (0x04b, 1, 'calibration-coefficient source'),
    (0x04c, 1, 'seam enable'),
    (0x04d, 9, "NOT RESOLVED — factory content holds the seller's wall dims 384x256"),
    (0x056, 3, 'void-line info'),
    (0x059, 1, 'receiver-card light'),
    (0x05a, 20, 'receiver card name (ASCII)'),
    (0x06e, 1, '14-way open flag'),
    (0x06f, 1, 'gamma-calibration status'),
    (0x070, 1, 'ROE current/bright flag'),
    (0x072, 1, 'virtual-pixel param'),
    (0x076, 1, 'full-screen seam-factor enable'),
    (0x077, 1, 'four-deseam'),
    (0x07b, 1, 'plus-module 7-way adjust enable'),
    (0x07c, 1, 'double-cali chroma enable'),
    (0x07d, 1, 'plus low-bright cali enable'),
    (0x07e, 1, 'double-cali enable'),
    (0x07f, 2, 'double-cali threshold'),
    (0x092, 32, 'control-area blob, high half — companion to 0x02'),
    (0x0b2, 1, 'parameter switch'),
    (0x0b3, 1, 'plus-chip low-bright cali enable'),
    (0x0b4, 3, 'plus-chip low-bright uniformity'),
    (0x0c1, 12, 'GX custom FCCL'),
    (0x0c8, 1, 'plus temperature-control enable'),
    (0x0ce, 16, 'double-cali threshold (long form)'),
    (0x0e1, 1, 'plus-module current-adjust enable'),
    (0x0f4, 2, 'preset temperature info / ROE fan'),
    (0x0f6, 1, 'power-off bright coefficient'),
    (0x0f7, 2, 'EMC info'),
    (0x0f9, 1, 'module power switch'),
    (0x0fa, 1, 'current/bright flag'),
    (0x0fd, 1, 'screen-shake param'),
    (0x118, 16, 'multi-seam param'),
    (0x127, 1, 'thermal-cali param'),
]
