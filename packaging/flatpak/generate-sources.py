#!/usr/bin/env python3
"""Regenerate packaging/flatpak/generated-sources.json from Cargo.lock.

Every crates.io dependency becomes a flatpak "archive" source pinned to the
checksum cargo already recorded, vendored into cargo/vendor/<crate>-<version>
so the Flatpak build never touches the network. Re-run after changing
dependencies, and commit the result.

    python3 packaging/flatpak/generate-sources.py
"""
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parents[2]
lock_path = root / "Cargo.lock"
out_path = pathlib.Path(__file__).resolve().parent / "generated-sources.json"

sources = []
for block in lock_path.read_text().split("[[package]]")[1:]:
    name = version = checksum = None
    for line in block.splitlines():
        line = line.strip()
        if line.startswith("name = "):
            name = line.split('"')[1]
        elif line.startswith("version = "):
            version = line.split('"')[1]
        elif line.startswith("checksum = "):
            checksum = line.split('"')[1]
    if not (name and version and checksum):
        continue  # workspace member or non-registry dependency
    crate = f"{name}-{version}"
    sources.extend(
        [
            {
                "type": "archive",
                # flatpak-builder detects archive format by extension only and
                # does not know ".crate" (a gzipped tar).
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{crate}.crate",
                "sha256": checksum,
                "dest": f"cargo/vendor/{crate}",
            },
            # cargo requires a checksum manifest in every vendored crate; raw
            # .crate archives do not ship one, so it is synthesized here.
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": f"cargo/vendor/{crate}",
                "dest-filename": ".cargo-checksum.json",
            },
        ]
    )

if not sources:
    sys.exit("no registry dependencies found in Cargo.lock")

out_path.write_text(json.dumps(sources, indent=2) + "\n")
print(f"wrote {len(sources)} crate sources to {out_path}")
