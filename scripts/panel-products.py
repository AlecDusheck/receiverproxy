#!/usr/bin/env python3
"""Write matched product details into the derived panel specs' [meta].

Reads the match files produced by the product search (TSV with the columns
spec_name, matched, maker, product_name, pitch_mm, module_mm, url, image_url,
datasheet_url, confidence, note) and fills maker, product, pitch_mm, url,
datasheet and image for the specs that matched. A row is written only at the
stated confidence or better; the confidence and the note go into meta.notes,
so a page can say how firm the match is.

Usage:
  panel-products.py match-*.tsv [--min-confidence medium] [--images DIR] [--commit]
"""
import argparse
import csv
import glob
import os
import re

ORDER = {"low": 0, "medium": 1, "high": 2}
ASSETS = "https://assets.receiverproxy.com/images/panels"


def specs(paths):
    for path in paths:
        with open(path, newline="") as f:
            for row in csv.DictReader(f, delimiter="\t"):
                if row.get("spec_name"):
                    yield row


def edit(spec_path, row, image_name):
    text = open(spec_path).read()
    if "[meta]" not in text:
        return None
    fields = []
    if row["maker"]:
        fields.append('maker = "' + row["maker"].replace('"', "'") + '"')
    if row["product_name"]:
        fields.append('product = "' + row["product_name"].replace('"', "'") + '"')
    if row["pitch_mm"]:
        fields.append(f"pitch_mm = {float(row['pitch_mm'])}")
    if row["url"]:
        fields.append(f'url = "{row["url"]}"')
    if row["datasheet_url"]:
        fields.append(f'datasheet = "{row["datasheet_url"]}"')
    if image_name:
        fields.append(f'image = "{ASSETS}/{image_name}"')
        fields.append(f'image_source = "{row["maker"] or "vendor"} product photo"')
    note = f"Matched to a product on sale at {row['confidence']} confidence"
    if row["note"]:
        note += f": {row['note'].rstrip('.')}"
    fields.append('notes = "' + note.replace('"', "'") + '."')
    body = "\n".join(fields)

    # Replace the fields we own inside [meta], keep the derived ones.
    out, seen = [], False
    for line in text.splitlines():
        if line.startswith("[") and line != "[meta]" and seen:
            out.append(body)
            seen = False
        if line == "[meta]":
            seen = True
        if seen and re.match(r"(maker|product|pitch_mm|url|datasheet|image|image_source|notes) =", line):
            continue
        out.append(line)
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="+")
    ap.add_argument("--min-confidence", default="medium", choices=list(ORDER))
    ap.add_argument("--images", help="directory of downloaded product photos, named <spec>.jpg")
    ap.add_argument("--commit", action="store_true")
    a = ap.parse_args()

    paths = [p for pattern in a.files for p in glob.glob(pattern)]
    written = skipped = 0
    for row in specs(paths):
        if row["matched"] != "yes" or ORDER[row["confidence"]] < ORDER[a.min_confidence]:
            skipped += 1
            continue
        spec = f"config/panels/{row['spec_name']}.toml"
        if not os.path.exists(spec):
            print(f"no spec: {spec}")
            continue
        image = None
        if a.images and os.path.exists(os.path.join(a.images, f"{row['spec_name']}.jpg")):
            image = f"{row['spec_name']}.jpg"
        text = edit(spec, row, image)
        if text is None:
            print(f"no [meta]: {spec}")
            continue
        written += 1
        if a.commit:
            open(spec, "w").write(text)
    print(f"{written} specs {'written' if a.commit else 'to write'}, {skipped} left alone")
    if not a.commit:
        print("dry run: nothing written. Re-run with --commit.")


main()
