#!/usr/bin/env python3
"""Build the update manifest from signed release artifacts.

Usage: make-manifest.py <version> <download-base-url> <dist-dir> [notes-file] > latest.json

Artifacts are named kubernet-wizard-<os>-<arch>[.app.tar.gz|.exe] and each
must have a sibling <file>.minisig.
"""
import hashlib, json, pathlib, re, sys, datetime

version, base, dist = sys.argv[1], sys.argv[2].rstrip("/"), pathlib.Path(sys.argv[3])
notes = pathlib.Path(sys.argv[4]).read_text() if len(sys.argv) > 4 else ""
pattern = re.compile(r"^kubernet-wizard-(macos|linux|windows)-(x86_64|aarch64)(\.app\.tar\.gz|\.exe)?$")

platforms = {}
for f in sorted(dist.iterdir()):
    m = pattern.match(f.name)
    if not m:
        continue
    sig = f.with_name(f.name + ".minisig")
    if not sig.exists():
        sys.exit(f"missing signature for {f.name}")
    platforms[f"{m[1]}-{m[2]}"] = {
        "url": f"{base}/{f.name}",
        "sha256": hashlib.sha256(f.read_bytes()).hexdigest(),
        "signature": sig.read_text(),
        "kind": "app_tar_gz" if m[3] == ".app.tar.gz" else "binary",
        "size": f.stat().st_size,
    }

if not platforms:
    sys.exit("no artifacts found")
json.dump({
    "version": version.lstrip("v"),
    "notes": notes,
    "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
    "platforms": platforms,
}, sys.stdout, indent=2)
