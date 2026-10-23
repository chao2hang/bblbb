#!/usr/bin/env bash
# Synthetic-only daily backup test: all artifacts and the disposable SQLite DB
# remain inside a guarded /tmp directory. Never use this harness on real data.
set -euo pipefail
umask 077
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
TMP_PARENT="${TMPDIR:-/tmp}"
TEST_ROOT="$(mktemp -d "$TMP_PARENT/bblbb-daily-tests.XXXXXX")"
TEST_ROOT="$(cd "$TEST_ROOT" && pwd -P)"
case "$TEST_ROOT" in "$TMP_PARENT"/bblbb-daily-tests.*) ;; *) echo "unsafe test path" >&2; exit 1;; esac
cleanup() {
  case "${TEST_ROOT:-}" in
    "$TMP_PARENT"/bblbb-daily-tests.*) [[ ! -d "$TEST_ROOT" || -L "$TEST_ROOT" ]] || rm -rf -- "$TEST_ROOT" ;;
    *) echo "refusing unsafe cleanup" >&2; return 1 ;;
  esac
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
MOCK_BIN="$TEST_ROOT/mock-bin"
mkdir -m 0700 "$MOCK_BIN"
cat > "$MOCK_BIN/df" <<'MOCK_DF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${MOCK_DF_MODE:-high}" == low ]]; then
  printf '%s\n' 'Filesystem 1024-blocks Used Available Capacity Mounted on' 'synthetic 100000 99999 1 99% /tmp'
else
  printf '%s\n' 'Filesystem 1024-blocks Used Available Capacity Mounted on' 'synthetic 100000 1 99999 1% /tmp'
fi
MOCK_DF
chmod 0700 "$MOCK_BIN/df"
cat > "$MOCK_BIN/aws" <<'MOCK_AWS'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == *"--bucket synthetic-bucket"* ]] || { echo "bucket not propagated" >&2; exit 90; }
printf '%s\n' "$*" >> "$MOCK_AWS_CALLS"
printf '%s\n' '{"Versions":[],"DeleteMarkers":[],"IsTruncated":false}'
MOCK_AWS
chmod 0700 "$MOCK_BIN/aws"
export MOCK_AWS_CALLS="$TEST_ROOT/aws.calls"
export PATH="$MOCK_BIN:$PATH"
export BBLBB_BACKUP_RETENTION_POLICY=synthetic-test-external-retention
export RACE_REAL_PYTHON="$(command -v python3)"

BACKUPS="$TEST_ROOT/backups"
mkdir -m 0700 "$BACKUPS"
[[ "$(realpath -e "$BACKUPS")" == "$TEST_ROOT/backups" ]] || fail "temp backup path changed unexpectedly"
STORAGE="$TEST_ROOT/storage"
mkdir -m 0700 "$STORAGE"
printf 'small data' > "$STORAGE/one"
DB="$TEST_ROOT/synthetic.sqlite"
MISSING_STORAGE="$TEST_ROOT/missing-storage"
MISSING_BACKUP_PARENT="$TEST_ROOT/missing-storage-backups"
mkdir -m 0700 "$MISSING_BACKUP_PARENT"
if BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" "$ROOT/ops/backup/daily.sh" \
  --db "$DB" --storage "$MISSING_STORAGE" --backup-dir "$MISSING_BACKUP_PARENT/backups" > "$TEST_ROOT/missing-storage.log" 2>&1; then
  fail "daily backup accepted an unavailable storage directory"
fi
grep -q '拒绝不可用或符号链接存储路径' "$TEST_ROOT/missing-storage.log" || fail "missing storage test failed for the wrong reason"
[[ ! -e "$MISSING_BACKUP_PARENT/backups" ]] || fail "missing-storage rejection created backup artifacts"
SYMLINK_PARENT="$TEST_ROOT/symlink-parent"
SYMLINK_TARGET="$TEST_ROOT/symlink-target"
mkdir -m 0700 "$SYMLINK_PARENT" "$SYMLINK_TARGET"
ln -s "$SYMLINK_TARGET" "$SYMLINK_PARENT/alias"
if BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" "$ROOT/ops/backup/daily.sh" \
  --db "$DB" --storage "$STORAGE" --backup-dir "$SYMLINK_PARENT/alias/backups" > "$TEST_ROOT/backup-symlink-path.log" 2>&1; then
  fail "daily backup accepted an ancestor symlink path"
fi
[[ ! -e "$SYMLINK_TARGET/backups" ]] || fail "ancestor symlink rejection created artifacts at its target"
S3_BACKUP_ROOT="$TEST_ROOT/s3-backups/backups"
mkdir -m 0700 "$TEST_ROOT/s3-backups"
if BBLBB_BACKUP_STORAGE_BACKEND=s3 BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" "$ROOT/ops/backup/daily.sh" \
  --db "$DB" --backup-dir "$S3_BACKUP_ROOT" > "$TEST_ROOT/s3-missing-bucket.log" 2>&1; then
  fail "S3 daily backup accepted missing bucket"
fi
grep -q 'requires BBLBB_BACKUP_S3_BUCKET' "$TEST_ROOT/s3-missing-bucket.log" || fail "S3 missing bucket did not fail closed"
if BBLBB_BACKUP_STORAGE_BACKEND=s3 BBLBB_BACKUP_S3_BUCKET=synthetic-bucket BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" "$ROOT/ops/backup/daily.sh" \
  --db "$DB" --backup-dir "$S3_BACKUP_ROOT" > "$TEST_ROOT/s3-unsupported.log" 2>&1; then
  fail "S3 daily job reported inventory-only run as complete backup"
fi
grep -q 'S3 data backup is not implemented' "$TEST_ROOT/s3-unsupported.log" || fail "configured S3 mode did not fail as unsupported"
[[ ! -e "$S3_BACKUP_ROOT/sqlite" ]] || fail "S3 rejection created incomplete backup artifacts"
python3 - "$DB" "$STORAGE" <<'PY'
import hashlib, pathlib, sqlite3, sys
conn = sqlite3.connect(sys.argv[1])
conn.executescript("""
CREATE TABLE schema_migrations(version INTEGER, name TEXT, checksum TEXT, applied_at INTEGER);
CREATE TABLE themes(id TEXT, name TEXT, status TEXT, created_at INTEGER, updated_at INTEGER);
CREATE TABLE plugins(id TEXT);
CREATE TABLE app_settings(key TEXT, value TEXT);
CREATE TABLE attachments(id TEXT, storage_backend TEXT, storage_key TEXT, status TEXT, size_bytes INTEGER, sha256 TEXT);
INSERT INTO schema_migrations VALUES (1, 'synthetic', 'abc', 1);
""")
data = (pathlib.Path(sys.argv[2]) / 'one').read_bytes()
conn.execute("INSERT INTO attachments VALUES ('attachment-one', 'local', 'one', 'ready', ?, ?)", (len(data), hashlib.sha256(data).hexdigest()))
conn.commit(); conn.close()
PY

# Generic legacy root-level names are sentinels and must remain untouched.
for name in bblbb.sqlite bblbb.sqlite.sha256 backup.json; do printf 'preserve:%s\n' "$name" > "$BACKUPS/$name"; done
if ! BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" BBLBB_BACKUP_STORAGE_BACKEND=local BBLBB_BACKUP_MAX_ATTACHMENT_FILES=1 BBLBB_BACKUP_MAX_ATTACHMENT_BYTES=100 BBLBB_BACKUP_MIN_ATTACHMENT_FREE_BYTES=0 "$ROOT/ops/backup/daily.sh" \
  --db "$DB" --storage "$STORAGE" --storage-backend local --backup-dir "$BACKUPS" --keep-days 0 > "$TEST_ROOT/daily.log" 2>&1; then
  echo "daily.log follows:" >&2
  sed -n '1,120p' "$TEST_ROOT/daily.log" >&2
  fail "synthetic daily backup failed"
fi
[[ -f "$BACKUPS/.daily-backup.lock" ]] || fail "daily lock file missing"
shopt -s nullglob
SQLITE_SETS=("$BACKUPS"/sqlite/*/bblbb.sqlite)
MANIFESTS=("$BACKUPS"/manifest-*.json)
ATTACHMENTS=("$BACKUPS"/attachments-*.tar.gz)
[[ ${#SQLITE_SETS[@]} -eq 1 && ${#MANIFESTS[@]} -eq 1 && ${#ATTACHMENTS[@]} -eq 1 ]] || fail "daily artifact set incomplete"
BACKUP_DB="${SQLITE_SETS[0]}"
MANIFEST="${MANIFESTS[0]}"
ARCHIVE="${ATTACHMENTS[0]}"
python3 - "$DB" "$BACKUP_DB" "$MANIFEST" "$ARCHIVE" <<'PY'
import hashlib, json, sqlite3, sys, tarfile
source_db, backup_db, manifest_path, archive_path = sys.argv[1:]
manifest = json.load(open(manifest_path, encoding='utf-8'))
assert manifest['database'] == backup_db, (manifest['database'], backup_db)
conn = sqlite3.connect(f'file:{backup_db}?mode=ro', uri=True)
rows = conn.execute("SELECT storage_key, size_bytes, sha256 FROM attachments WHERE storage_backend='local' AND status='ready'").fetchall()
conn.close()
refs = {item['path'][2:]: item for item in manifest['attachments']['db_snapshot_ready_local']}
assert set(refs) == {row[0] for row in rows}, (refs, rows)
for key, size, digest in rows:
    assert refs[key]['size'] == size and refs[key]['sha256'] == digest
members = {}
with tarfile.open(archive_path, 'r:gz') as archive:
    for member in archive:
        if member.isfile():
            stream = archive.extractfile(member)
            content_hash = hashlib.sha256()
            content_size = 0
            for block in iter(lambda: stream.read(1024 * 1024), b''):
                content_size += len(block); content_hash.update(block)
            members[member.name[2:]] = (content_size, content_hash.hexdigest())
for key, item in refs.items():
    assert members[key] == (item['size'], item['sha256']), (key, members.get(key), item)
PY
grep -q '^==> daily backup SUCCESS$' "$TEST_ROOT/daily.log" || fail "daily did not report a complete backup"
for name in bblbb.sqlite bblbb.sqlite.sha256 backup.json; do [[ -f "$BACKUPS/$name" ]] || fail "daily backup removed generic root sentinel"; done

sleep 1
if BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" BBLBB_BACKUP_MAX_ATTACHMENT_FILES=1 BBLBB_BACKUP_MAX_ATTACHMENT_BYTES=2 BBLBB_BACKUP_MIN_ATTACHMENT_FREE_BYTES=0 "$ROOT/ops/backup/daily.sh" \
  --db "$DB" --storage "$STORAGE" --backup-dir "$BACKUPS" --keep-days 0 > "$TEST_ROOT/daily-oversize.log" 2>&1; then
  sed -n '1,120p' "$TEST_ROOT/daily-oversize.log" >&2
  fail "oversized attachment set passed preflight"
fi

sleep 1
if MOCK_DF_MODE=low BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" BBLBB_BACKUP_MAX_ATTACHMENT_FILES=1 BBLBB_BACKUP_MAX_ATTACHMENT_BYTES=100 BBLBB_BACKUP_MIN_ATTACHMENT_FREE_BYTES=268435456 "$ROOT/ops/backup/daily.sh" \
  --db "$DB" --storage "$STORAGE" --storage-backend local --label low-space-test --backup-dir "$BACKUPS" --keep-days 0 > "$TEST_ROOT/daily-low-space.log" 2>&1; then
  fail "daily backup ignored low destination free space"
fi
grep -q '备份卷空间不足' "$TEST_ROOT/daily-low-space.log" || fail "low-space rejection did not use capacity guard"
[[ ! -e "$BACKUPS/sqlite/low-space-test" && ! -e "$BACKUPS/manifest-low-space-test.json" ]] || fail "low-space preflight ran after publishing backup artifacts"

RACE_BIN="$TEST_ROOT/race-bin"
mkdir -m 0700 "$RACE_BIN"
cp "$ROOT/ops/backup/daily.sh" "$RACE_BIN/daily.sh"
cat > "$RACE_BIN/sqlite.sh" <<'RACE_SQLITE'
#!/usr/bin/env bash
set -euo pipefail
"$RACE_REAL_SQLITE_BACKUP" "$@"
if [[ "${RACE_MODE:-}" == s3-snapshot ]]; then
  db="$1"; backup_dir="$2"; shift 2; label=default
  while [[ $# -gt 0 ]]; do case "$1" in --label) label="$2"; shift 2;; *) shift;; esac; done
  python3 - "$backup_dir/sqlite/$label/bblbb.sqlite" <<'PY'
import os, sqlite3, sys
path = sys.argv[1]
os.chmod(path, 0o600)
connection = sqlite3.connect(path)
connection.execute("INSERT INTO attachments VALUES ('snapshot-s3', 's3', 'object-key', 'ready', 9, '" + '0'*64 + "')")
connection.commit(); connection.close()
os.chmod(path, 0o400)
PY
fi
RACE_SQLITE
chmod 0700 "$RACE_BIN/sqlite.sh"
cat > "$RACE_BIN/manifest.sh" <<'RACE_MANIFEST'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${RACE_MODE:-}" == delete ]]; then rm -f -- "$RACE_STORAGE/one"; fi
exec "$RACE_REAL_MANIFEST" "$@"
RACE_MANIFEST
chmod 0700 "$RACE_BIN/manifest.sh"
cat > "$RACE_BIN/python3" <<'RACE_PYTHON'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${RACE_MODE:-}" == add && "${1##*/}" == archive-attachments.py ]]; then printf 'late injected file\n' > "$RACE_STORAGE/late-added"; : > "$RACE_MARKER"; set -- "$RACE_REAL_ARCHIVE" "${@:2}"; fi
exec "$RACE_REAL_PYTHON" "$@"
RACE_PYTHON
chmod 0700 "$RACE_BIN/python3"
RACE_STORAGE="$STORAGE" RACE_MODE=delete RACE_REAL_MANIFEST="$ROOT/ops/backup/manifest.sh" RACE_REAL_ARCHIVE="$ROOT/ops/backup/archive-attachments.py" RACE_REAL_PYTHON="$(command -v python3)" RACE_REAL_SQLITE_BACKUP="$ROOT/ops/backup/sqlite.sh" PATH="$RACE_BIN:$PATH" \
  BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" BBLBB_BACKUP_MAX_ATTACHMENT_FILES=2 BBLBB_BACKUP_MAX_ATTACHMENT_BYTES=100 BBLBB_BACKUP_MIN_ATTACHMENT_FREE_BYTES=0 \
  "$RACE_BIN/daily.sh" --db "$DB" --storage "$STORAGE" --storage-backend local --label missing-after-snapshot --backup-dir "$BACKUPS" --keep-days 0 > "$TEST_ROOT/daily-race.log" 2>&1 && fail "daily accepted a missing snapshot-referenced object"
grep -q 'No such file or directory' "$TEST_ROOT/daily-race.log" || grep -q 'storage tree changed since manifest inventory' "$TEST_ROOT/daily-race.log" || { sed -n '1,140p' "$TEST_ROOT/daily-race.log" >&2; fail "race failure did not identify attachment inconsistency"; }
[[ ! -e "$BACKUPS/attachments-missing-after-snapshot.tar.gz" ]] || fail "race test published an incomplete attachment archive"
printf 'small data' > "$STORAGE/one"
RACE_STORAGE="$STORAGE" RACE_MODE=add RACE_MARKER="$TEST_ROOT/archive-race-triggered" RACE_REAL_MANIFEST="$ROOT/ops/backup/manifest.sh" RACE_REAL_ARCHIVE="$ROOT/ops/backup/archive-attachments.py" RACE_REAL_PYTHON="$(command -v python3)" RACE_REAL_SQLITE_BACKUP="$ROOT/ops/backup/sqlite.sh" PATH="$RACE_BIN:$PATH" \
  BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" BBLBB_BACKUP_MAX_ATTACHMENT_FILES=2 BBLBB_BACKUP_MAX_ATTACHMENT_BYTES=100 BBLBB_BACKUP_MIN_ATTACHMENT_FREE_BYTES=0 \
  "$RACE_BIN/daily.sh" --db "$DB" --storage "$STORAGE" --storage-backend local --label added-after-manifest --backup-dir "$BACKUPS" --keep-days 0 > "$TEST_ROOT/daily-growth-race.log" 2>&1 && fail "daily accepted a file added after manifest inventory"
[[ -f "$TEST_ROOT/archive-race-triggered" ]] || fail "growth-race Python wrapper did not run"
grep -q 'storage tree changed since manifest inventory' "$TEST_ROOT/daily-growth-race.log" || { sed -n '1,140p' "$TEST_ROOT/daily-growth-race.log" >&2; fail "growth race did not identify stale inventory"; }
[[ ! -e "$BACKUPS/attachments-added-after-manifest.tar.gz" ]] || fail "growth race published an archive with unmanifested files"
RACE_STORAGE="$STORAGE" RACE_MODE=s3-snapshot RACE_REAL_MANIFEST="$ROOT/ops/backup/manifest.sh" RACE_REAL_ARCHIVE="$ROOT/ops/backup/archive-attachments.py" RACE_REAL_PYTHON="$(command -v python3)" RACE_REAL_SQLITE_BACKUP="$ROOT/ops/backup/sqlite.sh" PATH="$RACE_BIN:$PATH" \
  BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config" BBLBB_BACKUP_MAX_ATTACHMENT_FILES=2 BBLBB_BACKUP_MAX_ATTACHMENT_BYTES=100 BBLBB_BACKUP_MIN_ATTACHMENT_FREE_BYTES=0 \
  "$RACE_BIN/daily.sh" --db "$DB" --storage "$STORAGE" --storage-backend local --label s3-snapshot-race --backup-dir "$BACKUPS" --keep-days 0 > "$TEST_ROOT/daily-s3-snapshot-race.log" 2>&1 && fail "daily accepted ready S3 row present only in DB snapshot"
grep -q 'SQLite snapshot 含 ready S3 attachment' "$TEST_ROOT/daily-s3-snapshot-race.log" || { sed -n '1,140p' "$TEST_ROOT/daily-s3-snapshot-race.log" >&2; fail "snapshot S3 row was rejected for the wrong reason"; }
[[ ! -e "$BACKUPS/attachments-s3-snapshot-race.tar.gz" ]] || fail "S3 snapshot race published local-only attachment archive"
grep -q '^==> daily backup FAILED$' "$TEST_ROOT/daily-s3-snapshot-race.log" || fail "S3 snapshot race did not fail the backup job"

# Static guard: lock and no-clobber publication remain in the orchestrator.
grep -q 'flock -n 9' "$ROOT/ops/backup/daily.sh" || fail "daily backup lock missing"
grep -q 'ln -- "\$ATTACH_TMP" "\$ATTACH_FINAL"' "$ROOT/ops/backup/daily.sh" || fail "attachment publication is not no-clobber"
echo "PASS: synthetic daily backup, attachment limits, retention sentinels, lock, and no-clobber publication"
