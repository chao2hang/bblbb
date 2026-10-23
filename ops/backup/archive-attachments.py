#!/usr/bin/env python3
"""Create a bounded gzip-tar matching an attachment manifest inventory exactly."""
import argparse
import hashlib
import json
import os
import pathlib
import stat
import tarfile


def inventory_tree(root: pathlib.Path, max_files: int, max_bytes: int):
    """Inventory without following swapped symlinks in files or directories."""
    rows = {}
    pending = [(os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW), "")]
    total = 0
    entries_seen = 0
    max_entries = max_files * 2 + 1024
    try:
        while pending:
            directory_fd, prefix = pending.pop()
            try:
                for name in os.listdir(directory_fd):
                    entries_seen += 1
                    if entries_seen > max_entries:
                        raise ValueError("attachment storage tree exceeds configured entry-count limit")
                    key = f"{prefix}/{name}" if prefix else name
                    if not name or name in (".", "..") or chr(92) in name or chr(0) in name:
                        raise ValueError(f"unsafe attachment path: {key!r}")
                    before = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
                    if stat.S_ISDIR(before.st_mode):
                        child_fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory_fd)
                        opened = os.fstat(child_fd)
                        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
                            os.close(child_fd)
                            raise ValueError(f"attachment directory changed during inventory: {key!r}")
                        pending.append((child_fd, key))
                    elif stat.S_ISREG(before.st_mode):
                        fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory_fd)
                        try:
                            opened = os.fstat(fd)
                            if (not stat.S_ISREG(opened.st_mode) or opened.st_nlink != 1 or
                                    (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino)):
                                raise ValueError(f"attachment file changed or has unexpected hard links: {key!r}")
                            digest = hashlib.sha256()
                            size = 0
                            while True:
                                block = os.read(fd, 1024 * 1024)
                                if not block:
                                    break
                                size += len(block)
                                digest.update(block)
                            after = os.fstat(fd)
                            if size != opened.st_size or after.st_size != opened.st_size:
                                raise ValueError(f"attachment size changed during inventory: {key!r}")
                        finally:
                            os.close(fd)
                        rows[key] = {"size": size, "sha256": digest.hexdigest()}
                        total += size
                        if len(rows) > max_files or total > max_bytes:
                            raise ValueError("attachment inventory exceeds configured count/byte limits")
                    else:
                        raise ValueError(f"unsupported attachment filesystem entry: {key!r}")
            finally:
                os.close(directory_fd)
        return rows, total
    finally:
        for directory_fd, _ in pending:
            try:
                os.close(directory_fd)
            except OSError:
                pass


def open_regular_beneath(root_fd: int, key: str) -> int:
    parts = pathlib.PurePosixPath(key).parts
    if (not key or pathlib.PurePosixPath(key).is_absolute() or
            any(part in ("", ".", "..") for part in parts) or chr(92) in key):
        raise ValueError(f"unsafe manifest attachment key: {key!r}")
    current_fd = os.dup(root_fd)
    try:
        for part in parts[:-1]:
            next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=current_fd)
            os.close(current_fd)
            current_fd = next_fd
        fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=current_fd)
        if os.fstat(fd).st_nlink != 1:
            os.close(fd)
            raise ValueError(f"attachment file has unexpected hard links: {key!r}")
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            os.close(fd)
            raise ValueError(f"attachment is not a regular file: {key!r}")
        return fd
    finally:
        os.close(current_fd)


class HashingReader:
    def __init__(self, fd: int, expected_size: int):
        self.fd = fd
        self.expected_size = expected_size
        self.read_bytes = 0
        if os.fstat(fd).st_size != expected_size:
            raise ValueError("attachment size changed after inventory scan")
        self.digest = hashlib.sha256()

    def read(self, size=-1):
        remaining = self.expected_size - self.read_bytes
        if remaining <= 0:
            return b""
        if size is None or size < 0 or size > remaining:
            size = remaining
        blocks = bytearray()
        while len(blocks) < size:
            block = os.read(self.fd, size - len(blocks))
            if not block:
                break
            blocks.extend(block)
        result = bytes(blocks)
        self.read_bytes += len(result)
        self.digest.update(result)
        return result

    def verify_unchanged_size(self):
        if self.read_bytes != self.expected_size or os.fstat(self.fd).st_size != self.expected_size:
            raise ValueError("attachment size changed while archiving")
        if os.read(self.fd, 1):
            raise ValueError("attachment grew while archiving")
        if os.fstat(self.fd).st_size != self.expected_size:
            raise ValueError("attachment size changed during final archive check")


class BoundedWriter:
    def __init__(self, stream, limit: int):
        self.stream = stream
        self.limit = limit
        self.written = 0

    def write(self, data):
        if self.written + len(data) > self.limit:
            raise OSError("compressed attachment archive exceeded remaining-space limit")
        count = self.stream.write(data)
        self.written += count
        return count

    def tell(self):
        return self.written

    def flush(self):
        return self.stream.flush()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--storage", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--max-files", required=True, type=int)
    parser.add_argument("--max-bytes", required=True, type=int)
    parser.add_argument("--max-output-bytes", required=True, type=int)
    args = parser.parse_args()
    root = pathlib.Path(args.storage).resolve(strict=True)
    manifest = json.loads(pathlib.Path(args.manifest).read_text(encoding="utf-8"))
    items = manifest["attachments"]["objects"]
    expected = {}
    for item in items:
        path = item.get("path")
        if not isinstance(path, str) or not path.startswith("./"):
            raise ValueError("unsafe attachment manifest path")
        key = path[2:]
        if key in expected or item.get("size", -1) < 0 or len(item.get("sha256", "")) != 64:
            raise ValueError("duplicate or malformed attachment manifest item")
        expected[key] = {"size": item["size"], "sha256": item["sha256"]}
    if len(expected) > args.max_files or sum(row["size"] for row in expected.values()) > args.max_bytes:
        raise ValueError("manifest attachment inventory exceeds configured count/byte limits")
    actual, raw_bytes = inventory_tree(root, args.max_files, args.max_bytes)
    if actual != expected:
        raise ValueError("storage tree changed since manifest inventory was collected")
    root_fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        output_fd = os.open(args.output, os.O_WRONLY | os.O_NOFOLLOW)
        output_stat = os.fstat(output_fd)
        if (not stat.S_ISREG(output_stat.st_mode) or output_stat.st_size != 0 or output_stat.st_nlink != 1):
            os.close(output_fd)
            raise ValueError("archive output must be a singly-linked empty regular file")
        os.ftruncate(output_fd, 0)
        with os.fdopen(output_fd, "wb") as raw_output:
            bounded_output = BoundedWriter(raw_output, args.max_output_bytes)
            with tarfile.open(fileobj=bounded_output, mode="w|gz", format=tarfile.PAX_FORMAT) as archive:
                for key in sorted(expected):
                    row = expected[key]
                    fd = open_regular_beneath(root_fd, key)
                    try:
                        reader = HashingReader(fd, row["size"])
                        info = tarfile.TarInfo("./" + key)
                        info.size = row["size"]
                        info.mode = 0o600
                        info.mtime = 0
                        archive.addfile(info, reader)
                        reader.verify_unchanged_size()
                        if reader.read_bytes != row["size"] or reader.digest.hexdigest() != row["sha256"]:
                            raise ValueError(f"attachment changed while archiving: {key!r}")
                    finally:
                        os.close(fd)
                final_inventory, final_raw_bytes = inventory_tree(root, args.max_files, args.max_bytes)
                if final_inventory != expected or final_raw_bytes != raw_bytes:
                    raise ValueError("storage tree changed while attachment archive was being written")
            raw_output.flush()
            os.fsync(raw_output.fileno())
    finally:
        os.close(root_fd)
    print(f"archived files={len(expected)} raw_bytes={raw_bytes} archive_bytes={os.path.getsize(args.output)}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, json.JSONDecodeError, tarfile.TarError) as error:
        raise SystemExit(f"attachment archive rejected: {error}")
