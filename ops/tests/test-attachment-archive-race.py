#!/usr/bin/env python3
"""Synthetic regular-file growth race: archive must reject, not save a prefix."""
import hashlib
import json
import os
import pathlib
import runpy
import sys
import tempfile

root = pathlib.Path(tempfile.mkdtemp(prefix="bblbb-archive-race-", dir=os.environ.get("TMPDIR", "/tmp")))
try:
    storage = root / "storage"
    storage.mkdir(mode=0o700)
    data_path = storage / "object"
    original = b"old-bytes"
    data_path.write_bytes(original)
    manifest = root / "manifest.json"
    manifest.write_text(json.dumps({"attachments": {"objects": [{
        "path": "./object", "size": len(original), "sha256": hashlib.sha256(original).hexdigest()
    }]}}), encoding="utf-8")
    output = root / "archive.tar.gz"
    output.touch(mode=0o600)

    module = runpy.run_path(sys.argv[1], run_name="archive_race_test")
    real_read = os.read
    injected = [False]

    def append_before_archive_read(fd, size):
        if not injected[0]:
            with data_path.open("ab") as target:
                target.write(b"-concurrent-growth")
            injected[0] = True
        return real_read(fd, size)

    os.read = append_before_archive_read
    try:
        sys.argv = [sys.argv[1], "--manifest", str(manifest), "--storage", str(storage),
                    "--output", str(output), "--max-files", "10", "--max-bytes", "1024",
                    "--max-output-bytes", "1048576"]
        try:
            module["main"]()
        except ValueError as error:
            if "size changed" not in str(error) and "grew" not in str(error):
                raise
        except SystemExit as error:
            if error.code in (None, 0):
                raise AssertionError("archiver accepted a file that grew after its inventory read")
        else:
            raise AssertionError("archiver accepted a file that grew after its inventory read")
    finally:
        os.read = real_read
    assert injected[0], "race injection did not run"
    assert data_path.read_bytes() == original + b"-concurrent-growth"
finally:
    # Guard recursive cleanup strictly to this mkdtemp-created private root.
    resolved = root.resolve(strict=True)
    if resolved.parent != pathlib.Path(os.environ.get("TMPDIR", "/tmp")).resolve(strict=True) or not resolved.name.startswith("bblbb-archive-race-"):
        raise RuntimeError(f"refusing unsafe cleanup: {resolved}")
    import shutil
    shutil.rmtree(resolved)
