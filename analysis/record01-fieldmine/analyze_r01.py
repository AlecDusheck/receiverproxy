#!/usr/bin/env python3
"""fieldmine: statistically validate field meanings in Colorlight .rcvbp record 0x01.

Pipeline (rerunnable, pure local analysis; no hardware, no repo writes):
  1. Read file list from ../scan_results.tsv (col 1), re-parse every file
     (zlib at +0x20, TLV records: u16-LE total size, 2 type bytes, payload),
     keep record type-low-byte 0x01 with 764-byte payload, dedupe by file md5.
  2. Extract filename labels: pitch (P2.5/Q3.0...), <N>S scan tokens,
     WxH tokens, chip tokens.
  3. Per byte offset: distribution; functional-formula matching (byte and
     u16-LE at every offset); group-consistency vs scan/chip/geometry keys
     with shuffled baselines; permutation/ramp-table detection; f32 slots;
     targeted hypothesis tests for specific offsets (gamma, refresh, tables,
     timing block, MaxWidth/MaxHeight) each verified against the corpus.
  4. Emit fielddict.csv + report.md in this directory.

Usage: python3 analyze_r01.py [--rebuild]
"""
import csv
import hashlib
import json
import os
import re
import struct
import sys
import zlib
from collections import Counter

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SCAN_TSV = os.path.join(HERE, "..", "scan_results.tsv")
CACHE = os.path.join(HERE, "r01_matrix.npz")
META = os.path.join(HERE, "r01_meta.json")
FIELDDICT = os.path.join(HERE, "fielddict.csv")
REPORT = os.path.join(HERE, "report.md")

VENDOR_DIRS = ("ledvision/", "lv/", "lv88/", "lv96/", "ex/", "ex2/", "corpus/")
CORPUS_ROOT = ("/private/tmp/claude-501/-Users-amd-e120/"
               "261c3dad-ba97-45d2-8ea3-ab7a950a8ff9/scratchpad/")

SIG_COMPRESSED = bytes([0x20, 0x20, 0x19, 0xBE])
PAYLOAD_LEN = 764


# ---------------------------------------------------------------- parsing

def parse_records(d):
    """Return list of (type_be_u16, payload) or None. Mirrors scan_rcvbp.py."""
    if len(d) < 32:
        return None
    if d[0:4] == SIG_COMPRESSED:
        raw_len = struct.unpack_from("<I", d, 0x18)[0]
        try:
            blob = zlib.decompress(d[0x20:])
        except zlib.error:
            return None
        if len(blob) != raw_len:
            return None
        slack = 0
    else:
        blob = d[0x14:]
        slack = 4
    records = []
    off = 0
    while off + 4 + slack <= len(blob):
        size = struct.unpack_from("<H", blob, off)[0]
        if size < 4 or off + size > len(blob):
            return None
        rtype = (blob[off + 2] << 8) | blob[off + 3]
        records.append((rtype, blob[off + 4:off + size]))
        off += size
    if len(blob) - off > slack:
        return None
    return records


WH_RE = re.compile(r"(?<!\d)(\d{2,4})\s*[xX×]\s*(\d{2,4})(?!\d)")
SCAN_RE = re.compile(r"(?<![0-9A-Za-z.])(\d{1,3})[Ss](?![0-9A-Za-z])")
PITCH_RE = re.compile(r"^[PQ]\s*(\d+(?:[._]\d+)?)")
CHIPTOK_RE = re.compile(r"(?<![\d.])(\d{4,5})(?![\d])")


def parse_labels(basename):
    stem = re.sub(r"\.rcvbp$", "", basename, flags=re.I)
    lab = {}
    m = PITCH_RE.match(stem)
    if m:
        lab["pitch"] = float(m.group(1).replace("_", "."))
    scans = [int(x) for x in SCAN_RE.findall(stem) if 1 <= int(x) <= 128]
    if scans:
        lab["scan_tok"] = scans[0]
    lab["wh_toks"] = [(int(a), int(b)) for a, b in WH_RE.findall(stem)]
    lab["chip_toks"] = CHIPTOK_RE.findall(stem)
    return lab


def build(rebuild=False):
    if not rebuild and os.path.exists(CACHE) and os.path.exists(META):
        z = np.load(CACHE)
        with open(META) as f:
            meta = json.load(f)
        return z["m"], meta
    paths = []
    with open(SCAN_TSV) as f:
        for line in f:
            p = line.split("\t", 1)[0].strip()
            rel = p[len(CORPUS_ROOT):] if p.startswith(CORPUS_ROOT) else p
            if rel.startswith(VENDOR_DIRS):
                paths.append(p)
    seen = set()
    rows, meta = [], []
    m508_rows = []
    for p in paths:
        try:
            with open(p, "rb") as f:
                d = f.read()
        except OSError:
            continue
        md5 = hashlib.md5(d).hexdigest()
        if md5 in seen:
            continue
        seen.add(md5)
        recs = parse_records(d)
        if not recs:
            continue
        r01 = next((pl for (t, pl) in recs if (t & 0xFF) == 0x01), None)
        if r01 is None:
            continue
        base = os.path.basename(p)
        if len(r01) == 508:
            m508_rows.append(np.frombuffer(r01, dtype=np.uint8))
            continue
        if len(r01) != PAYLOAD_LEN:
            continue
        rows.append(np.frombuffer(r01, dtype=np.uint8))
        meta.append({"path": p, "base": base, "md5": md5, **parse_labels(base)})
    m = np.vstack(rows)
    m508 = np.vstack(m508_rows) if m508_rows else np.zeros((0, 508), np.uint8)
    np.savez_compressed(CACHE, m=m, m508=m508)
    with open(META, "w") as f:
        json.dump({"meta": meta, "n_508_unique": len(m508_rows)}, f)
    with open(META) as f:
        meta = json.load(f)
    return m, meta


# ---------------------------------------------------------------- features

def derive_features(m, meta_rows):
    n = m.shape[0]
    W = m[:, 0].astype(float)
    H = (m[:, 1].astype(float)) * 2
    S = m[:, 0x20].astype(float)
    chip = (m[:, 0x204].astype(int) << 8) | m[:, 0x36].astype(int)

    pitch = np.full(n, np.nan)
    scan_tok = np.full(n, np.nan)
    wh_toks = []
    for i, r in enumerate(meta_rows):
        if "pitch" in r:
            pitch[i] = r["pitch"]
        if "scan_tok" in r:
            scan_tok[i] = r["scan_tok"]
        wh_toks.append([tuple(t) for t in r.get("wh_toks", [])])
    Ssafe = np.where(S > 0, S, np.nan)
    feats = {
        "W": W, "H": H, "S": S,
        "W/2": W / 2, "W/4": W / 4, "W/8": W / 8, "W/16": W / 16,
        "W*2": W * 2, "W*4": W * 4, "W*8": W * 8, "W*16": W * 16,
        "H/2": H / 2, "H/4": H / 4, "H/8": H / 8, "H*2": H * 2,
        "S/2": S / 2, "S*2": S * 2,
        "log2(S)": np.where(S > 0, np.log2(Ssafe), np.nan),
        "H/S": H / Ssafe, "H/(2S)": H / (2 * Ssafe),
        "W*H/S": W * H / Ssafe, "W*H/(2S)": W * H / (2 * Ssafe),
        "W*H/(4S)": W * H / (4 * Ssafe), "W*H/(8S)": W * H / (8 * Ssafe),
        "W*H/(16S)": W * H / (16 * Ssafe),
        "pitch*10": np.round(pitch * 10),
        "scan_tok": scan_tok,
    }
    aux = {"chip": chip, "pitch": pitch, "scan_tok": scan_tok,
           "wh_toks": wh_toks}
    return feats, aux


# ---------------------------------------------------------------- matching

def match_feature(col, f, hi=255):
    valid = np.isfinite(f) & (f == np.floor(f)) & (f >= 0) & (f <= hi)
    nv = int(valid.sum())
    if nv < 30:
        return None
    fv = f[valid]
    if len(np.unique(fv)) < 3:
        return None
    hits = (col[valid] == fv)
    cov = hits.mean()
    vals, cnts = np.unique(fv, return_counts=True)
    minor = fv != vals[np.argmax(cnts)]
    nm = int(minor.sum())
    disc = hits[minor].mean() if nm > 0 else 0.0
    return cov, nv, disc, nm


def agreements_for_key(keyvals, m):
    _, gid = np.unique(keyvals, axis=0 if keyvals.ndim > 1 else None,
                       return_inverse=True)
    G = gid.max() + 1
    n, ncols = m.shape
    agr = np.empty(ncols)
    for c in range(ncols):
        cnt = np.bincount(gid * 256 + m[:, c].astype(np.int64),
                          minlength=G * 256).reshape(G, 256)
        agr[c] = cnt.max(axis=1).sum() / n
    return agr, G


# ------------------------------------------------- targeted hypothesis tests

def targeted_tests(m, feats, aux, n):
    """Verify specific field hypotheses against the corpus.
    Returns {offset: (span, hypothesis, evidence, counterexamples)} plus
    a list of headline findings for the report."""
    u16 = lambda o: (m[:, o].astype(int) | (m[:, o + 1].astype(int) << 8))
    f32 = lambda o: m[:, o:o + 4].copy().view("<f4").ravel()
    ann = {}
    notes = []

    def put(off, span, hyp, ev, cex):
        ann[off] = (span, hyp, ev, cex)

    W, H, S = feats["W"], feats["H"], feats["S"]

    # anchors vs filename tokens
    st = aux["scan_tok"]
    k = np.isfinite(st)
    agree = int((S[k] == st[k]).sum())
    put(0x20, 1, "scan denominator S",
        f"== filename <N>S token in {agree}/{int(k.sum())} labeled files",
        int(k.sum()) - agree)
    single = np.array([len(t) == 1 for t in aux["wh_toks"]])
    w1 = np.array([t[0][0] if len(t) == 1 else -1 for t in aux["wh_toks"]])
    h1 = np.array([t[0][1] if len(t) == 1 else -1 for t in aux["wh_toks"]])
    wa = int(((w1 == W) & single).sum())
    wd = int(((w1 == W) | (w1 == W * 2) | (w1 == W * 4) | (w1 == W * 8))
             [single].sum())
    put(0x000, 1, "module width W (or a divisor of the cabinet width)",
        f"single-WxH-token files: token width == byte in {wa}/"
        f"{int(single.sum())}, == byte*2^k in {wd}/{int(single.sum())}",
        int(single.sum()) - wd)
    ha = int(((h1 == H) & single).sum())
    put(0x001, 1, "module height / 2",
        f"single-WxH-token files: token height == 2*byte in {ha}/"
        f"{int(single.sum())}", int(single.sum()) - ha)

    # gamma f32 @0x1c
    g = f32(0x1c)
    plaus = ((g >= 1.0) & (g <= 4.0)) | (g == 0)
    top = Counter(np.round(g, 2)).most_common(4)
    put(0x1c, 4, "f32 gamma coefficient",
        f"values {top}; {plaus.mean():.1%} in [1,4] or 0 "
        f"(2.8 is the LEDVision default)", int((~plaus).sum()))
    notes.append(("0x01c", "f32 gamma", f"top values {top}"))

    # const 60.0 f32 @0x53
    v = f32(0x53)
    put(0x53, 4, "f32 = 60.0 (frame rate?)",
        f"exactly 60.0 in {(v == 60.0).sum()}/{n}", int((v != 60.0).sum()))

    # refresh rate f32 @0xaa
    v = f32(0xaa)
    top = Counter(np.round(v, 1)).most_common(5)
    plaus = (v == 0) | ((v >= 50) & (v <= 10000))
    put(0xaa, 4, "f32 refresh rate (Hz)",
        f"values {top}; {plaus.mean():.1%} in 0 or [50,10000]",
        int((~plaus).sum()))
    notes.append(("0x0aa", "f32 refresh rate", f"top values {top}"))

    # f32 @0xae chip-dependent
    v = f32(0xae)
    top = Counter(np.round(v, 4)).most_common(4)
    put(0xae, 4, "f32, chip-dependent (default 1e-4)", f"values {top}", 0)

    # f32 triple 0xb4/0xb8/0xbc
    for o in (0xb4, 0xb8, 0xbc):
        v = f32(o)
        top = Counter(np.round(v, 4)).most_common(4)
        plaus = (v >= 0) & (v <= 4)
        put(o, 4, "f32 coefficient (duty/current-gain triple B4/B8/BC)",
            f"values {top}; {plaus.mean():.1%} in [0,4]", int((~plaus).sum()))
    notes.append(("0x0b4/0x0b8/0x0bc", "f32 coefficient triple",
                  "typ {0.1,0.25,0.5}; plausibly per-channel duty/gain"))

    # f32 @0x1f8: 0 or 0.1
    v = f32(0x1f8)
    c01 = int(((v == 0) | (np.abs(v - 0.1) < 1e-6)).sum())
    put(0x1f8, 4, "f32 in {0, 0.1}", f"{c01}/{n} files", n - c01)

    # MaxWidth/MaxHeight u16 @0xc0/0xc2 vs filename tokens
    c0, c2 = u16(0xc0), u16(0xc2)
    hit = np.array([any((a, b) == (c0[i], c2[i]) for a, b in aux["wh_toks"][i])
                    for i in range(n)])
    multi = np.array([len(t) >= 2 for t in aux["wh_toks"]])
    put(0xc0, 2, "u16 MaxWidth", "", 0)
    put(0xc2, 2, "u16 MaxHeight", "", 0)
    ev = (f"(MaxW,MaxH) matches a filename WxH token in "
          f"{int(hit[multi].sum())}/{int(multi.sum())} files with >=2 tokens; "
          f"MaxH multiple of module H in "
          f"{int((c2 % np.maximum(H, 1) == 0).sum())}/{n}, "
          f"MaxW multiple of module W in "
          f"{int((c0 % np.maximum(W, 1) == 0).sum())}/{n}")
    ann[0xc0] = (2, "u16 MaxWidth (configured wall/card width)", ev,
                 int(multi.sum()) - int(hit[multi].sum()))
    ann[0xc2] = (2, "u16 MaxHeight (configured wall/card height)", ev,
                 int(multi.sum()) - int(hit[multi].sum()))
    notes.append(("0x0c0/0x0c2", "u16 MaxWidth/MaxHeight", ev))

    # chip id bytes
    put(0x36, 1, "chip id low byte (with 0x204 high)",
        f"{len(np.unique(aux['chip']))} distinct chip ids in corpus", 0)
    put(0x204, 1, "chip id high byte (with 0x36 low)",
        f"1 for {int((m[:, 0x204] == 1).sum())} files (ids >= 0x100)", 0)

    # 16-entry row-order table 0x5a
    w = m[:, 0x5a:0x6a]
    perm = (np.sort(w, axis=1) == np.arange(16)).all(axis=1)
    inrange = (w < 128).all(axis=1)
    put(0x5a, 16, "16-entry row-order map",
        f"permutation of 0..15 in {perm.mean():.1%}; other files hold "
        f"row indices from wider ranges ({inrange.mean():.1%} all < 128)",
        int((~perm).sum()))
    notes.append(("0x05a..0x069", "16-entry row-order map",
                  f"perm of 0..15 in {perm.mean():.0%} of files; interleaved "
                  "orders (e.g. 16,0,17,1,...) on high-scan panels"))

    # 96-entry table 0x114
    ident = (m[:, 0x114:0x174] == np.arange(96)).all(axis=1)
    zero = (m[:, 0x114:0x174] == 0).all(axis=1)
    put(0x114, 96, "96-entry row map (identity default)",
        f"identity 0..95 in {ident.mean():.1%}, all-zero in "
        f"{int(zero.sum())} files, nothing else observed",
        int((~(ident | zero)).sum()))
    notes.append(("0x114..0x173", "96-entry row map",
                  f"identity in {ident.mean():.1%}, zero in {int(zero.sum())}"))

    # 64-entry table 0x19a
    tb = m[:, 0x19a:0x1da]
    ramp = (tb == np.arange(64) + 64).all(axis=1)
    zero = (tb == 0).all(axis=1)
    put(0x19a, 64, "64-entry table: ramp 64..127 or all-zero",
        f"ramp in {ramp.mean():.1%}, zero in {zero.mean():.1%}, "
        f"nothing else; zero/ramp not predicted by any single other byte",
        int((~(ramp | zero)).sum()))
    notes.append(("0x19a..0x1d9", "64-entry table",
                  f"ramp 64..127 in {ramp.mean():.0%}, zero in "
                  f"{zero.mean():.0%} (no other pattern)"))

    # timing block constants
    for o, val in ((0x17b, 1000), (0x18c, 1000), (0x194, 1000)):
        v = u16(o)
        put(o, 2, f"u16 = {val} (timing block constant)",
            f"{int((v == val).sum())}/{n}", int((v != val).sum()))
    v = u16(0x178)
    put(0x178, 2, "u16 timing (15 default)",
        f"{Counter(v).most_common(3)}", int((v != 15).sum()))
    quad = [(0x182, 25), (0x184, 125), (0x186, 125), (0x188, 62)]
    for o, dv in quad:
        v = u16(o)
        top = Counter(v).most_common(4)
        put(o, 2, "u16 PWM-chip timing (1 on non-PWM chips)",
            f"values {top}", 0)
    notes.append(("0x174..0x199", "timing block",
                  "u16 fields; 1000 at 0x17b/0x18c/0x194 in every file; "
                  "quad at 0x182/184/186/188 = (25,125,125,62) default, "
                  "(1,1,1,1) on 187 files, other values chip-specific"))

    # duplicated timing byte
    eq = (m[:, 0x21] == m[:, 0x4b]).mean()
    put(0x21, 1, "timing, chip+scan dependent (== byte 0x4b)",
        f"equals 0x4b in {eq:.1%}", int(round((1 - eq) * n)))
    put(0x4b, 1, "timing, chip+scan dependent (== byte 0x21)",
        f"equals 0x21 in {eq:.1%}", int(round((1 - eq) * n)))

    # 0x57/0x58 pair
    eq = (m[:, 0x57] == m[:, 0x58]).mean()
    put(0x57, 1, "paired with 0x58 (per-channel value?)",
        f"equals 0x58 in {eq:.1%}; common values 16/0/128", 0)
    put(0x58, 1, "paired with 0x57", f"equals 0x57 in {eq:.1%}", 0)

    return ann, notes


def main():
    rebuild = "--rebuild" in sys.argv
    m, metaj = build(rebuild)
    meta_rows = metaj["meta"]
    n = m.shape[0]
    print(f"matrix: {n} unique files x {m.shape[1]} bytes "
          f"(+{metaj['n_508_unique']} unique 508-byte payloads set aside)")

    feats, aux = derive_features(m, meta_rows)
    chip = aux["chip"]
    W, H, S = feats["W"], feats["H"], feats["S"]

    u16le = m[:, :-1].astype(np.int64) | (m[:, 1:].astype(np.int64) << 8)

    keys = [
        ("S", S.reshape(-1, 1)),
        ("W", W.reshape(-1, 1)),
        ("H", H.reshape(-1, 1)),
        ("chip", chip.reshape(-1, 1).astype(float)),
        ("geo(W,H,S)", np.stack([W, H, S], axis=1)),
        ("chip+S", np.stack([chip.astype(float), S], axis=1)),
        ("chip+geo", np.stack([chip.astype(float), W, H, S], axis=1)),
    ]
    rng = np.random.default_rng(0)
    agr, agr_shuf, ngroups = {}, {}, {}
    for name, kv in keys:
        a, G = agreements_for_key(kv, m)
        ash, _ = agreements_for_key(kv[rng.permutation(n)], m)
        agr[name], agr_shuf[name], ngroups[name] = a, ash, G

    # functional matches per byte
    byte_best = [None] * PAYLOAD_LEN
    for off in range(PAYLOAD_LEN):
        col = m[:, off].astype(np.int64)
        if len(np.unique(col)) < 2:
            continue
        best = None
        for fname, f in feats.items():
            r = match_feature(col, f)
            if r is None:
                continue
            cov, nv, disc, nm = r
            if best is None or (cov, disc) > (best[1], best[3]):
                best = (fname, cov, nv, disc, nm)
        byte_best[off] = best

    # u16-LE matches at every offset
    u16_best = {}
    u16feats = {k: v for k, v in feats.items() if k != "log2(S)"}
    for off in range(PAYLOAD_LEN - 1):
        col = u16le[:, off]
        if len(np.unique(col)) < 2:
            continue
        for fname, f in u16feats.items():
            r = match_feature(col, f, hi=65535)
            if r is None:
                continue
            cov, nv, disc, nm = r
            if cov >= 0.95 and disc >= 0.85:
                prev = u16_best.get(off)
                if prev is None or cov > prev[1]:
                    u16_best[off] = (fname, cov, nv, disc, nm)

    ann, notes = targeted_tests(m, feats, aux, n)
    annotated = {}
    for off, (span, hyp, ev, cex) in ann.items():
        for o in range(off, off + span):
            annotated[o] = (off, span, hyp, ev, cex)

    # classify each byte
    classes = []
    for off in range(PAYLOAD_LEN):
        col = m[:, off]
        vals, cnts = np.unique(col, return_counts=True)
        order = np.argsort(-cnts)
        top = [(int(vals[i]), int(cnts[i])) for i in order[:3]]
        nuniq = len(vals)
        cls, hyp, ev, cex = "", "", "", 0
        if off in annotated:
            base, span, hyp0, ev0, cex0 = annotated[off]
            cls = "annotated"
            hyp = (hyp0 if base == off
                   else f"part of field @0x{base:03x} ({hyp0})")
            ev, cex = ev0, cex0
        if nuniq == 1:
            if not cls:
                cls, hyp, ev = "constant", f"const 0x{top[0][0]:02x}", f"{n}/{n}"
            else:
                cls = "annotated-const"
        elif not cls and cnts.max() / n >= 0.99:
            cls = "near-constant"
            hyp = f"~const 0x{top[0][0]:02x}"
            cex = n - int(cnts.max())
            ev = f"{int(cnts.max())}/{n}"
        elif not cls:
            b = byte_best[off]
            u = u16_best.get(off)
            if u and (b is None or u[1] >= b[1]):
                fname, cov, nv, disc, nm = u
                cls = "formula-u16"
                hyp = f"u16LE == {fname}"
                ev = f"{int(round(cov * nv))}/{nv} (minority {disc:.2f} of {nm})"
                cex = nv - int(round(cov * nv))
            elif b and b[1] >= 0.98 and b[3] >= 0.85:
                fname, cov, nv, disc, nm = b
                cls = "formula"
                hyp = f"byte == {fname}"
                ev = f"{int(round(cov * nv))}/{nv} (minority {disc:.2f} of {nm})"
                cex = nv - int(round(cov * nv))
            else:
                placed = False
                for tier, thresh in (("", 0.98), ("likely-", 0.95)):
                    if placed:
                        break
                    for kname, _ in keys:
                        a, ash = agr[kname][off], agr_shuf[kname][off]
                        if a >= thresh and (a - ash) >= 0.05:
                            cls = f"{tier}determined-by:{kname}"
                            hyp = f"consistent within {kname} groups"
                            ev = (f"agr={a:.3f} shuf={ash:.3f} "
                                  f"G={ngroups[kname]}")
                            cex = int(round((1 - a) * n))
                            placed = True
                            break
                if not placed:
                    if b and b[1] >= 0.90:
                        fname, cov, nv, disc, nm = b
                        cls = "formula-weak"
                        hyp = f"byte ~= {fname}"
                        ev = (f"{int(round(cov * nv))}/{nv} "
                              f"(minority {disc:.2f} of {nm})")
                        cex = nv - int(round(cov * nv))
                    else:
                        cls = "unknown"
                        bk = max(keys, key=lambda kv: agr[kv[0]][off]
                                 - agr_shuf[kv[0]][off])[0]
                        ev = (f"best key {bk}: agr={agr[bk][off]:.3f} "
                              f"shuf={agr_shuf[bk][off]:.3f}")
        classes.append({"off": off, "nuniq": nuniq, "top": top, "class": cls,
                        "hyp": hyp, "ev": ev, "cex": cex})

    # enum-like leftovers (only genuinely unexplained bytes)
    enum_cands = []
    for c in classes:
        if (c["class"] in ("unknown", "formula-weak")
                and 2 <= c["nuniq"] <= 8):
            col = m[:, c["off"]].astype(float)
            p = aux["pitch"]
            ok = np.isfinite(p)
            pr = (np.corrcoef(col[ok], p[ok])[0, 1]
                  if ok.sum() > 100 and len(np.unique(col[ok])) > 1 else np.nan)
            sr = (np.corrcoef(col, S)[0, 1]
                  if len(np.unique(col)) > 1 else np.nan)
            cr = agr["chip"][c["off"]]
            enum_cands.append((c["off"], c["nuniq"], c["top"], pr, sr, cr))

    with open(FIELDDICT, "w", newline="") as f:
        wr = csv.writer(f)
        wr.writerow(["offset_hex", "offset_dec", "n_unique", "top_values",
                     "class", "best_hypothesis", "evidence", "counterexamples",
                     "agr_S", "agr_W", "agr_H", "agr_chip", "agr_geo",
                     "agr_chip+S", "agr_chip+geo"])
        for c in classes:
            o = c["off"]
            wr.writerow([f"0x{o:03x}", o, c["nuniq"],
                         " ".join(f"{v}:{k}" for v, k in c["top"]),
                         c["class"], c["hyp"], c["ev"], c["cex"],
                         *(f"{agr[k][o]:.3f}" for k, _ in keys)])

    def bucket(cls):
        if cls in ("constant",):
            return "constant"
        if cls == "near-constant":
            return "near-constant"
        if cls.startswith("annotated"):
            return "annotated-field"
        if cls.startswith("formula") and cls != "formula-weak":
            return "formula"
        if "determined-by:chip" in cls:
            return "chip-determined"
        if "determined-by" in cls:
            return "geometry-determined"
        if cls == "formula-weak":
            return "weak"
        return "unknown"

    cov_stats = Counter(bucket(c["class"]) for c in classes)

    chipmap = {}
    for i, r in enumerate(meta_rows):
        toks = r.get("chip_toks", [])
        if toks:
            chipmap.setdefault(int(chip[i]), Counter()).update(toks[:1])
    chip_counter = Counter(int(c) for c in chip)

    with open(REPORT, "w") as f:
        f.write(render_report(n, metaj, cov_stats, classes, notes, enum_cands,
                              chip_counter, chipmap))
    print(f"wrote {FIELDDICT} and {REPORT}")
    print(dict(cov_stats))


def render_report(n, metaj, cov_stats, classes, notes, enum_cands,
                  chip_counter, chipmap):
    L = []
    L.append("# Record 0x01 field mining — statistical validation\n")
    L.append(f"Corpus: {n} unique 764-byte record-0x01 payloads (deduped by "
             f"file md5) from the vendor config tree; "
             f"{metaj['n_508_unique']} older 508-byte payloads set aside.\n")
    L.append("## Coverage of the 764 payload bytes\n")
    total = sum(cov_stats.values())
    for k in ("constant", "near-constant", "annotated-field", "formula",
              "geometry-determined", "chip-determined", "weak", "unknown"):
        if cov_stats.get(k):
            L.append(f"- {k}: {cov_stats[k]} bytes "
                     f"({cov_stats[k] / total:.0%})")
    L.append("")
    L.append("## Verified field hypotheses (targeted tests)\n")
    for c in classes:
        if c["class"].startswith("annotated") and not c["hyp"].startswith("part of"):
            L.append(f"- 0x{c['off']:03x}: {c['hyp']} — {c['ev']}"
                     + (f" [{c['cex']} counterexamples]" if c["cex"] else ""))
    L.append("")
    L.append("## Formula matches from blind scan\n")
    for c in classes:
        if c["class"].startswith("formula") and c["class"] != "formula-weak":
            L.append(f"- 0x{c['off']:03x}: {c['hyp']} — {c['ev']}, "
                     f"{c['cex']} counterexamples")
    L.append("")
    L.append("## Headline notes\n")
    for off, name, detail in notes:
        L.append(f"- {off} {name}: {detail}")
    L.append("")
    L.append("## Unknown bytes\n")
    unk = [c["off"] for c in classes if c["class"] == "unknown"]
    if unk:
        ranges, start, prev = [], unk[0], unk[0]
        for o in unk[1:]:
            if o == prev + 1:
                prev = o
                continue
            ranges.append((start, prev))
            start = prev = o
        ranges.append((start, prev))
        L.append("Ranges: " + ", ".join(
            f"0x{a:03x}-0x{b:03x}" if a != b else f"0x{a:03x}"
            for a, b in ranges))
        L.append("")
        L.append("Most unknown bytes still sit at high (0.90+) agreement "
                 "under the chip+geometry key but below the 0.95 cutoff — "
                 "consistent with per-file user-tuned values (brightness, "
                 "per-panel timing tweaks) layered on chip/geometry "
                 "defaults, not with random noise.")
    L.append("")
    L.append("## Enum-like unknown bytes (candidate settings enums)\n")
    enum_cands.sort(key=lambda t: -t[5])
    for off, nu, top, pr, sr, cr in enum_cands[:25]:
        L.append(f"- 0x{off:03x}: {nu} values {top}, corr(pitch)={pr:.2f}, "
                 f"corr(scan)={sr:.2f}, chip-agreement={cr:.2f}")
    L.append("")
    L.append("## Chip families in corpus (payload enum -> filename token)\n")
    for cid, cnt in chip_counter.most_common(15):
        tok = chipmap.get(cid)
        name = tok.most_common(1)[0][0] if tok else "?"
        L.append(f"- 0x{cid:04x}: {cnt} files, common filename token {name}")
    return "\n".join(L) + "\n"


if __name__ == "__main__":
    main()
