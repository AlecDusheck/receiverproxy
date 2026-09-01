#!/usr/bin/env python3
"""Emit variant panel specs for a sweep: one TOML per (name, overrides).

Overrides are `section.key = value` strings applied textually to the base
spec (keys replaced in place, or appended to the section when absent).

Usage: variants.py <base.toml> <outdir> NAME 'section.key=value' ... [-- NAME ...]
"""
import os
import re
import sys

base_path, outdir = sys.argv[1], sys.argv[2]
os.makedirs(outdir, exist_ok=True)
base = open(base_path).read()

groups, cur = [], None
for tok in sys.argv[3:]:
    if tok == "--":
        cur = None
    elif cur is None:
        cur = [tok, []]
        groups.append(cur)
    else:
        cur[1].append(tok)

for name, overrides in groups:
    text = base
    for ov in overrides:
        key, value = ov.split("=", 1)
        section, k = key.split(".", 1)
        sec_re = re.compile(rf"(^\[{re.escape(section)}\]\n)(.*?)(?=^\[|\Z)", re.S | re.M)
        m = sec_re.search(text)
        if not m:
            text += f"\n[{section}]\n{k} = {value}\n"
            continue
        body = m.group(2)
        line_re = re.compile(rf"^{re.escape(k)}\s*=.*$", re.M)
        if line_re.search(body):
            body = line_re.sub(f"{k} = {value}", body)
        else:
            body = f"{k} = {value}\n" + body
        text = text[: m.start(2)] + body + text[m.end(2) :]
    text = re.sub(r'^name = ".*"$', f'name = "{name}"', text, count=1, flags=re.M)
    with open(os.path.join(outdir, f"{name}.toml"), "w") as f:
        f.write(text)
    print(os.path.join(outdir, f"{name}.toml"))
