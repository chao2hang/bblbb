#!/usr/bin/env python3
"""Bound and safely extract regular-file/directory release tar.gz bundles."""
import argparse
import gzip
import os
import pathlib
import stat
import tarfile


class BoundedReader:
    """Cap decompressed tar bytes, including ignored headers and directory payloads."""

    def __init__(self, stream, limit: int):
        self.stream = stream
        self.limit = limit
        self.read_bytes = 0

    def read(self, size=-1):
        remaining = self.limit - self.read_bytes
        if size is None or size < 0 or size > remaining + 1:
            size = remaining + 1
        block = self.stream.read(size)
        self.read_bytes += len(block)
        if self.read_bytes > self.limit:
            raise ValueError("archive exceeds uncompressed stream-size limit")
        return block


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", required=True)
    parser.add_argument("--destination", required=True)
    parser.add_argument("--max-compressed-bytes", type=int, default=2 * 1024**3)
    parser.add_argument("--max-unpacked-bytes", type=int, default=20 * 1024**3)
    parser.add_argument("--max-members", type=int, default=250_000)
    args = parser.parse_args()
    if args.max_compressed_bytes <= 0 or args.max_unpacked_bytes <= 0 or args.max_members <= 0:
        parser.error("size and member limits must be positive")
    archive_path = pathlib.Path(args.archive)
    destination = pathlib.Path(args.destination)
    archive_fd = os.open(archive_path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        archive_stat = os.fstat(archive_fd)
        if not stat.S_ISREG(archive_stat.st_mode) or archive_stat.st_size <= 0 or archive_stat.st_size > args.max_compressed_bytes:
            raise ValueError("archive is not a regular file or exceeds compressed-size limit")
        if destination.is_symlink() or not destination.is_dir() or any(destination.iterdir()):
            raise ValueError("destination must be an empty, non-symlink directory")
        root_fd = os.open(destination, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            seen = set()
            kinds = {}
            unpacked = 0
            count = 0
            with os.fdopen(os.dup(archive_fd), "rb") as archive_stream:
                decompressed_stream = gzip.GzipFile(fileobj=archive_stream, mode="rb")
                stream_limit = args.max_unpacked_bytes + args.max_members * 1024 + 1024**2
                bounded_stream = BoundedReader(decompressed_stream, stream_limit)
                with tarfile.open(fileobj=bounded_stream, mode="r|") as archive:
                    for member in archive:
                        count += 1
                        if count > args.max_members:
                            raise ValueError("archive has too many members")
                        raw = member.name
                        if member.isdir() and member.size != 0:
                            raise ValueError("directory member has nonzero size")
                        if raw in (".", "./") and member.isdir():
                            continue
                        if not raw.startswith("./") or chr(92) in raw or chr(0) in raw:
                            raise ValueError(f"unsafe archive member path: {raw!r}")
                        key = raw[2:]
                        if member.isdir() and key.endswith("/"):
                            key = key[:-1]
                        path = pathlib.PurePosixPath(key)
                        parts = path.parts
                        if (not key or path.is_absolute() or any(part in ("", ".", "..") for part in parts)
                                or path.as_posix() != key):
                            raise ValueError(f"unsafe archive member path: {raw!r}")
                        if not (member.isdir() or member.isfile()):
                            raise ValueError(f"unsupported archive member type: {raw!r}")
                        if key in seen:
                            raise ValueError(f"duplicate archive member: {raw!r}")
                        kind = "dir" if member.isdir() else "file"
                        if kinds.get(key) not in (None, kind):
                            raise ValueError(f"conflicting archive member type: {raw!r}")
                        seen.add(key)
                        kinds[key] = kind
                        for index in range(1, len(parts)):
                            parent = "/".join(parts[:index])
                            if kinds.get(parent) == "file":
                                raise ValueError(f"file member is also an ancestor: {parent!r}")
                            kinds.setdefault(parent, "dir")
                        if member.isdir():
                            if member.size != 0:
                                raise ValueError("directory member has nonzero size")
                            current_fd = os.dup(root_fd)
                            try:
                                for part in parts:
                                    try:
                                        os.mkdir(part, 0o700, dir_fd=current_fd)
                                    except FileExistsError:
                                        pass
                                    next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=current_fd)
                                    os.close(current_fd)
                                    current_fd = next_fd
                            finally:
                                os.close(current_fd)
                            continue
                        if member.size < 0:
                            raise ValueError("negative file size")
                        unpacked += member.size
                        if unpacked > args.max_unpacked_bytes:
                            raise ValueError("archive exceeds uncompressed-size limit")
                        current_fd = os.dup(root_fd)
                        try:
                            for part in parts[:-1]:
                                try:
                                    os.mkdir(part, 0o700, dir_fd=current_fd)
                                except FileExistsError:
                                    pass
                                next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=current_fd)
                                os.close(current_fd)
                                current_fd = next_fd
                            safe_mode = 0o700 if member.mode & 0o111 else 0o600
                            output_fd = os.open(parts[-1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, safe_mode, dir_fd=current_fd)
                            try:
                                source = archive.extractfile(member)
                                if source is None:
                                    raise ValueError(f"archive member is unreadable: {raw!r}")
                                remaining = member.size
                                while remaining:
                                    block = source.read(min(1024 * 1024, remaining))
                                    if not block:
                                        raise ValueError(f"truncated archive member: {raw!r}")
                                    view = memoryview(block)
                                    while view:
                                        written = os.write(output_fd, view)
                                        view = view[written:]
                                    remaining -= len(block)
                                if source.read(1):
                                    raise ValueError(f"archive member exceeds declared size: {raw!r}")
                                os.fsync(output_fd)
                            finally:
                                os.close(output_fd)
                        finally:
                            os.close(current_fd)
                    while bounded_stream.read(1024 * 1024):
                        pass
            os.fsync(root_fd)
        finally:
            os.close(root_fd)
    finally:
        os.close(archive_fd)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, tarfile.TarError) as error:
        raise SystemExit(f"release archive rejected: {error}")
