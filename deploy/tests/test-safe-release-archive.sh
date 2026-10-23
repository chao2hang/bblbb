#!/usr/bin/env bash
# Synthetic archive extraction tests; never extracts outside a guarded temp root.
set -euo pipefail
umask 077
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
TMP_PARENT="${TMPDIR:-/tmp}"
TEST_ROOT="$(mktemp -d "$TMP_PARENT/bblbb-safe-archive-tests.XXXXXX")"
TEST_ROOT="$(cd "$TEST_ROOT" && pwd -P)"
case "$TEST_ROOT" in "$TMP_PARENT"/bblbb-safe-archive-tests.*) ;; *) echo "unsafe test root" >&2; exit 1 ;; esac
cleanup() { case "${TEST_ROOT:-}" in "$TMP_PARENT"/bblbb-safe-archive-tests.*) [[ ! -d "$TEST_ROOT" || -L "$TEST_ROOT" ]] || rm -rf -- "$TEST_ROOT" ;; *) return 1 ;; esac; }
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
make_archive() {
  local archive=$1 member=$2 kind=${3:-file}
  python3 - "$archive" "$member" "$kind" <<'PY'
import sys, tarfile, io
path, name, kind = sys.argv[1:]
with tarfile.open(path, 'w:gz') as tar:
    info = tarfile.TarInfo(name)
    if kind == 'file':
        data = b'fixture\n'; info.size = len(data); tar.addfile(info, io.BytesIO(data))
    elif kind == 'symlink':
        info.type = tarfile.SYMTYPE; info.linkname = '../../outside'; tar.addfile(info)
    elif kind == 'hardlink':
        info.type = tarfile.LNKTYPE; info.linkname = './safe'; tar.addfile(info)
PY
}
make_directory_payload_archive() {
  local archive=$1 directory=$2
  python3 - "$archive" "$directory" <<'PY'
import io, sys, tarfile
archive_path, name = sys.argv[1:]
with tarfile.open(archive_path, 'w:gz') as tar:
    payload = b'directory-payload-must-be-rejected'
    directory = tarfile.TarInfo(name)
    directory.type = tarfile.DIRTYPE
    directory.size = len(payload)
    tar.addfile(directory, io.BytesIO(payload))
    version = b'fixture-version\n'
    member = tarfile.TarInfo('./VERSION')
    member.size = len(version)
    tar.addfile(member, io.BytesIO(version))
PY
}
EXTRACTOR="$ROOT/deploy/scripts/safe-extract-release-bundle.py"
make_archive "$TEST_ROOT/good.tar.gz" ./bundle/VERSION
python3 - "$TEST_ROOT/executable.tar.gz" <<'PY'
import sys, tarfile, io
with tarfile.open(sys.argv[1], 'w:gz') as tar:
    data=b'#!/bin/sh\nexit 0\n'
    info=tarfile.TarInfo('./backend/bblbb-backend'); info.size=len(data); info.mode=0o755
    tar.addfile(info, io.BytesIO(data))
PY
mkdir -m 0700 "$TEST_ROOT/executable"
python3 "$EXTRACTOR" --archive "$TEST_ROOT/executable.tar.gz" --destination "$TEST_ROOT/executable"
[[ -x "$TEST_ROOT/executable/backend/bblbb-backend" ]] || fail "extractor discarded a regular executable bit"
[[ "$(stat -c '%a' "$TEST_ROOT/executable/backend/bblbb-backend")" == 700 ]] || fail "extractor preserved unsafe executable permissions instead of normalized 0700"
mkdir -m 0700 "$TEST_ROOT/good"
python3 "$EXTRACTOR" --archive "$TEST_ROOT/good.tar.gz" --destination "$TEST_ROOT/good"
[[ "$(<"$TEST_ROOT/good/bundle/VERSION")" == fixture ]] || fail "valid nested member was not extracted"

for case_name in traversal absolute backslash symlink hardlink duplicate; do
  archive="$TEST_ROOT/$case_name.tar.gz"
  case "$case_name" in
    traversal) make_archive "$archive" ./../outside ;;
    absolute) make_archive "$archive" /tmp/outside ;;
    backslash) make_archive "$archive" './safe\escape' ;;
    symlink) make_archive "$archive" ./safe symlink ;;
    hardlink) make_archive "$archive" ./safe hardlink ;;
    duplicate)
      python3 - "$archive" <<'PY'
import sys, tarfile, io
with tarfile.open(sys.argv[1], 'w:gz') as tar:
    for _ in range(2):
        data=b'x'; info=tarfile.TarInfo('./same'); info.size=1; tar.addfile(info, io.BytesIO(data))
PY
      ;;
  esac
  destination="$TEST_ROOT/out-$case_name"
  mkdir -m 0700 "$destination"
  if python3 "$EXTRACTOR" --archive "$archive" --destination "$destination" > "$TEST_ROOT/$case_name.log" 2>&1; then
    fail "extractor accepted $case_name archive"
  fi
done
[[ ! -e "$TEST_ROOT/outside" ]] || fail "archive escaped extraction root"
[[ ! -e "$TEST_ROOT/safe/escape" ]] || fail "backslash member escaped or wrote unsafe path"

mkdir -m 0700 "$TEST_ROOT/storage"
for directory_member in ./ ./nested/; do
  case_name="directory-payload-${directory_member//[^A-Za-z0-9]/_}"
  archive="$TEST_ROOT/$case_name.tar.gz"
  make_directory_payload_archive "$archive" "$directory_member"
  destination="$TEST_ROOT/out-$case_name"
  mkdir -m 0700 "$destination"
  if python3 "$EXTRACTOR" --archive "$archive" --destination "$destination" > "$TEST_ROOT/$case_name.log" 2>&1; then
    fail "extractor accepted nonzero directory payload for $directory_member"
  fi
  grep -q 'directory member has nonzero size' "$TEST_ROOT/$case_name.log" || fail "extractor rejected $directory_member for the wrong reason"
  digest="$(sha256sum "$archive" | awk '{print $1}')"
  if BBLBB_RELEASE_EXPECTED_SHA256="$digest" BBLBB_ENV_FILE="$TEST_ROOT/no-env" \
      BBLBB__DATABASE_URL="sqlite:///$TEST_ROOT/synthetic.sqlite" BBLBB__STORAGE_DIR="$TEST_ROOT/storage" \
      bash "$ROOT/deploy/scripts/release.sh" --bundle "$archive" --releases-dir "$TEST_ROOT/releases-$case_name" \
      > "$TEST_ROOT/$case_name-preflight.log" 2>&1; then
    fail "release preflight accepted nonzero directory payload for $directory_member"
  fi
  grep -q 'bundle directory member has nonzero size' "$TEST_ROOT/$case_name-preflight.log" || {
    sed -n '1,100p' "$TEST_ROOT/$case_name-preflight.log" >&2
    fail "release preflight rejected $directory_member for the wrong reason"
  }
done

# Count decompressed trailing payload too, not only declared file contents (small cap fixture).
python3 - "$TEST_ROOT/padded.tar.gz" <<'PY'
import gzip, io, sys, tarfile
archive_path = sys.argv[1]
buffer = io.BytesIO()
with tarfile.open(fileobj=buffer, mode='w') as archive:
    payload = b'fixture-version\n'
    member = tarfile.TarInfo('./VERSION')
    member.size = len(payload)
    archive.addfile(member, io.BytesIO(payload))
with gzip.open(archive_path, 'wb') as compressed:
    compressed.write(buffer.getvalue())
    compressed.write(b'padding' * (200_000))
PY
mkdir -m 0700 "$TEST_ROOT/padded-destination"
if python3 "$EXTRACTOR" --archive "$TEST_ROOT/padded.tar.gz" --destination "$TEST_ROOT/padded-destination" \
    --max-unpacked-bytes 64 --max-members 1 > "$TEST_ROOT/padded.log" 2>&1; then
  fail "extractor ignored decompressed stream-size cap"
fi
grep -q 'archive exceeds uncompressed stream-size limit' "$TEST_ROOT/padded.log" || fail "trailing decompressed payload rejected for the wrong reason"
# Metadata validation uses the same safe extractor and requires executable backend files.
META_ROOT="$TEST_ROOT/metadata-source"
mkdir -m 0700 -p "$META_ROOT/backend" "$META_ROOT/frontend/build" "$META_ROOT/frontend/node_modules" "$META_ROOT/openapi" "$META_ROOT/migrations/sqlite" "$META_ROOT/migrations/mysql" "$META_ROOT/migrations/mariadb"
printf '#!/bin/sh\nexit 0\n' > "$META_ROOT/backend/bblbb-backend"
printf '#!/bin/sh\nexit 0\n' > "$META_ROOT/backend/bblbb-migrate"
chmod 0755 "$META_ROOT/backend/bblbb-backend" "$META_ROOT/backend/bblbb-migrate"
printf 'openapi: 3.1.0\npaths: {}\n' > "$META_ROOT/openapi/openapi.yaml"
printf '{}\n' > "$META_ROOT/SBOM.json"
printf '{"dependencies":{}}\n' > "$META_ROOT/deps-npm.json"
printf 'lock\n' > "$META_ROOT/Cargo.lock"
printf '{}\n' > "$META_ROOT/package-lock.json"
printf 'version\n' > "$META_ROOT/VERSION"
printf '{"version":"1.2.3","build_commit":"abc","build_commit_short":"abc","rust":"test","dependency_locks":{"backend":"Cargo.lock","frontend":"package-lock.json"},"sbom":"SBOM.json","checksums":"SHA256SUMS","built_at":"2026-01-01T00:00:00Z"}\n' > "$META_ROOT/METADATA.json"
(cd "$META_ROOT" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
tar -czf "$TEST_ROOT/metadata.tar.gz" -C "$META_ROOT" .
export TMPDIR="$TEST_ROOT"
"$ROOT/deploy/scripts/record-release-metadata.sh" --bundle "$TEST_ROOT/metadata.tar.gz" > "$TEST_ROOT/metadata.log"
grep -q '==> OK' "$TEST_ROOT/metadata.log" || fail "metadata validator rejected safe executable bundle"
shopt -s nullglob
metadata_temps=("$TEST_ROOT"/bblbb-release-metadata.*)
[[ ${#metadata_temps[@]} -eq 0 ]] || fail "metadata validation leaked its temporary extraction directory"

# Enforce both count and unpacked-byte bounds.
make_archive "$TEST_ROOT/large.tar.gz" ./large
mkdir -m 0700 "$TEST_ROOT/limited"
if python3 "$EXTRACTOR" --archive "$TEST_ROOT/large.tar.gz" --destination "$TEST_ROOT/limited" --max-unpacked-bytes 1 > "$TEST_ROOT/limit.log" 2>&1; then
  fail "extractor ignored uncompressed-size cap"
fi

echo "PASS: bounded safe release bundle extraction rejects traversal, links, duplicates, and excess size"
