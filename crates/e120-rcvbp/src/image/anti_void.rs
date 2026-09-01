//! The anti-void-line packs (`GetAntiVoidLineParam` @ 0x1604d0).
//!
//! With no void lines configured the generator degenerates to two identical
//! blocks of 2048 big-endian counters `0x2000 + n` (bit 5 always set, bit 7
//! cleared for every line since none is void), sliced into four 0x400-byte
//! packs at 0x1800. Packs 4–7 (0x7000) stay zero without large-load support.

pub const LEN: usize = 0x1000;

#[must_use]
pub fn region() -> [u8; LEN] {
    let mut out = [0u8; LEN];
    for block in 0..2 {
        for n in 0..0x400u16 {
            let at = block * 0x800 + usize::from(n) * 2;
            out[at..at + 2].copy_from_slice(&(0x2000 + n).to_be_bytes());
        }
    }
    out
}
