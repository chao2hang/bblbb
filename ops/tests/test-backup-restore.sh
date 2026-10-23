#!/usr/bin/env bash
# Synthetic-only regression tests for backup/restore helpers.
# Every database path is guarded to remain below a private temporary directory;
# AWS calls are served by a deterministic mock and never reach the network.
set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
REAL_SQLITE3="$(command -v sqlite3 || true)"
command -v python3 >/dev/null || { echo "SKIP/FAIL: python3 is required" >&2; exit 1; }
[[ -n "$REAL_SQLITE3" ]] || { echo "SKIP/FAIL: sqlite3 CLI is required for guarded synthetic fixtures" >&2; exit 1; }

TMP_PARENT="${TMPDIR:-/tmp}"
TEST_ROOT="$(mktemp -d "$TMP_PARENT/bblbb-backup-tests.XXXXXX")"
TEST_ROOT="$(cd "$TEST_ROOT" && pwd -P)"
case "$TEST_ROOT" in
  "$TMP_PARENT"/bblbb-backup-tests.*) ;;
  *) echo "拒绝非预期测试临时目录: $TEST_ROOT" >&2; exit 1 ;;
esac
WRITER_PID=""
cleanup() {
  if [[ -n "${WRITER_PID:-}" ]]; then
    kill "$WRITER_PID" 2>/dev/null || true
    wait "$WRITER_PID" 2>/dev/null || true
  fi
  case "${TEST_ROOT:-}" in
    "$TMP_PARENT"/bblbb-backup-tests.*)
      if [[ -d "$TEST_ROOT" && ! -L "$TEST_ROOT" ]]; then rm -rf -- "$TEST_ROOT"; fi
      ;;
    *) echo "拒绝清理非预期测试目录" >&2; return 1 ;;
  esac
}
trap cleanup EXIT

MOCK_BIN="$TEST_ROOT/mock-bin"
mkdir -m 0700 "$MOCK_BIN"
export TEST_ROOT REAL_SQLITE3

cat > "$MOCK_BIN/sqlite3" <<'MOCK_SQLITE'
#!/usr/bin/env bash
set -euo pipefail
ALL_ARGS=("$@")
DB=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -*) shift ;;
    *) DB="$1"; shift; break ;;
  esac
done
[[ -n "$DB" ]] || { echo "mock sqlite3: database argument missing" >&2; exit 90; }
case "$DB" in
  "$TEST_ROOT"/*) ;;
  *) echo "mock sqlite3: refusing non-synthetic database path" >&2; exit 91 ;;
esac
exec "$REAL_SQLITE3" "${ALL_ARGS[@]}"
MOCK_SQLITE
chmod 0700 "$MOCK_BIN/sqlite3"

cat > "$MOCK_BIN/aws" <<'MOCK_AWS'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == s3api && "${2:-}" == list-object-versions ]] || {
  echo "mock aws: unexpected command" >&2; exit 92;
}
KEY_MARKER=""
VERSION_MARKER=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --key-marker) KEY_MARKER="$2"; shift 2 ;;
    --version-id-marker) VERSION_MARKER="$2"; shift 2 ;;
    *) shift ;;
  esac
done
printf '%s|%s\n' "$KEY_MARKER" "$VERSION_MARKER" >> "$MOCK_AWS_CALLS"
case "${MOCK_AWS_MODE:-pages}:$KEY_MARKER:$VERSION_MARKER" in
  pages::)
    printf '%s\n' '{"Versions":[{"Key":"alpha","VersionId":"v2"}],"DeleteMarkers":[],"IsTruncated":true,"NextKeyMarker":"alpha","NextVersionIdMarker":"v2"}' ;;
  pages:alpha:v2)
    printf '%s\n' '{"Versions":[{"Key":"alpha","VersionId":"v1"}],"DeleteMarkers":[{"Key":"beta","VersionId":"d1"}],"IsTruncated":true,"NextKeyMarker":"beta","NextVersionIdMarker":"d1"}' ;;
  pages:beta:d1)
    printf '%s\n' '{"Versions":[{"Key":"zeta","VersionId":"v3"}],"DeleteMarkers":[],"IsTruncated":false}' ;;
  repeated-marker::)
    printf '%s\n' '{"Versions":[],"DeleteMarkers":[],"IsTruncated":true,"NextKeyMarker":"repeat","NextVersionIdMarker":"v1"}' ;;
  repeated-marker:repeat:v1)
    printf '%s\n' '{"Versions":[],"DeleteMarkers":[],"IsTruncated":true,"NextKeyMarker":"repeat","NextVersionIdMarker":"v1"}' ;;
  missing-marker::)
    printf '%s\n' '{"Versions":[],"DeleteMarkers":[],"IsTruncated":true}' ;;
  *) echo "mock aws: unexpected pagination marker" >&2; exit 93 ;;
esac
MOCK_AWS
chmod 0700 "$MOCK_BIN/aws"
export MOCK_AWS_CALLS="$TEST_ROOT/aws.calls"
export BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/no-production-config"
export PATH="$MOCK_BIN:$PATH"

fail() { echo "FAIL: $*" >&2; exit 1; }
mode_is() {
  python3 - "$1" "$2" <<'PY'
import os, sys
actual = os.stat(sys.argv[1], follow_symlinks=False).st_mode & 0o777
expected = int(sys.argv[2], 8)
raise SystemExit(0 if actual == expected else 1)
PY
}

# Build only a disposable WAL database with enough synthetic rows for the
# online snapshot to overlap an active writer. The writer updates its sequence
# and row in one transaction, allowing snapshot consistency to be asserted.
FIXTURE_DB="$TEST_ROOT/source.sqlite"
python3 - "$FIXTURE_DB" <<'PY'
import sqlite3, sys
conn = sqlite3.connect(sys.argv[1])
assert conn.execute("PRAGMA journal_mode=WAL").fetchone()[0].lower() == "wal"
conn.executescript("CREATE TABLE sample (id INTEGER PRIMARY KEY, value TEXT NOT NULL); CREATE TABLE state (id INTEGER PRIMARY KEY CHECK(id=1), last_id INTEGER NOT NULL); INSERT INTO state VALUES (1,0);")
conn.executemany("INSERT INTO sample(id,value) VALUES (?,?)", ((i, f'seed-{i}') for i in range(1, 40001)))
conn.execute("UPDATE state SET last_id=40000 WHERE id=1")
conn.commit(); conn.close()
PY
python3 - "$FIXTURE_DB" "$TEST_ROOT/writer.commits" <<'PY' > "$TEST_ROOT/writer.log" 2>&1 &
import sqlite3, sys, time
path, commits_path = sys.argv[1:]
conn = sqlite3.connect(path, timeout=5)
print("ready", flush=True)
seq = 40000
commits = 0
while True:
    seq += 1
    conn.execute("BEGIN IMMEDIATE")
    conn.execute("INSERT INTO sample(id,value) VALUES (?,?)", (seq, f'writer-{seq}'))
    conn.execute("UPDATE state SET last_id=? WHERE id=1", (seq,))
    conn.commit()
    commits += 1
    with open(commits_path, "w", encoding="ascii") as out: out.write(str(commits))
    time.sleep(0.002)
PY
WRITER_PID=$!
for _ in $(seq 1 100); do
  grep -q '^ready$' "$TEST_ROOT/writer.log" 2>/dev/null && break
  sleep 0.02
done
grep -q '^ready$' "$TEST_ROOT/writer.log" || fail "synthetic concurrent writer did not start"
for _ in $(seq 1 500); do
  [[ -s "$TEST_ROOT/writer.commits" ]] && break
  sleep 0.02
done
if [[ ! -s "$TEST_ROOT/writer.commits" ]]; then
  sed -n '1,80p' "$TEST_ROOT/writer.log" >&2
  fail "synthetic writer did not commit within the 10-second fixture deadline"
fi
[[ -f "$FIXTURE_DB-wal" ]] || fail "synthetic source did not retain a WAL sidecar"

BACKUP_ROOT="$TEST_ROOT/backups"
"$ROOT/ops/backup/sqlite.sh" "$FIXTURE_DB" "$BACKUP_ROOT" --label online-test > "$TEST_ROOT/sqlite-backup.log"
BACKUP_DIR="$BACKUP_ROOT/sqlite/online-test"
BACKUP_DB="$BACKUP_DIR/bblbb.sqlite"
[[ -f "$BACKUP_DB" && -f "$BACKUP_DB.sha256" && -f "$BACKUP_DIR/backup.json" ]] || fail "online backup outputs missing"
mode_is "$BACKUP_DIR" 700 || fail "backup directory mode is not 0700"
mode_is "$BACKUP_DB" 400 || fail "database snapshot mode is not 0400"
mode_is "$BACKUP_DB.sha256" 400 || fail "checksum mode is not 0400"
mode_is "$BACKUP_DIR/backup.json" 400 || fail "metadata mode is not 0400"
python3 - "$BACKUP_DB" "$BACKUP_DB.sha256" "$BACKUP_DIR/backup.json" <<'PY'
import hashlib, json, sqlite3, sys
backup, checksum_path, metadata_path = sys.argv[1:]
with open(backup, "rb") as f: digest = hashlib.sha256(f.read()).hexdigest()
assert open(checksum_path, encoding="utf-8").read().strip() == digest
metadata = json.load(open(metadata_path, encoding="utf-8"))
assert metadata["snapshot_method"] == "sqlite3.Connection.backup"
assert metadata["integrity_check"] == "ok"
conn = sqlite3.connect(f"file:{backup}?mode=ro", uri=True)
assert conn.execute("PRAGMA integrity_check").fetchone() == ("ok",)
last_id = conn.execute("SELECT last_id FROM state WHERE id=1").fetchone()[0]
max_id = conn.execute("SELECT MAX(id) FROM sample").fetchone()[0]
count = conn.execute("SELECT COUNT(*) FROM sample").fetchone()[0]
assert last_id == max_id == count, (last_id, max_id, count)
assert conn.execute("SELECT value FROM sample WHERE id=?", (last_id,)).fetchone() == (f"writer-{last_id}",)
assert conn.execute("SELECT value FROM sample WHERE id=1").fetchone() == ("seed-1",)
conn.close()
PY
BEFORE_DIGEST="$(sha256sum "$BACKUP_DB" | awk '{print $1}')"
if "$ROOT/ops/backup/sqlite.sh" "$FIXTURE_DB" "$BACKUP_ROOT" --label online-test > "$TEST_ROOT/sqlite-no-clobber.log" 2>&1; then
  fail "online backup unexpectedly replaced an existing label"
fi
[[ "$(sha256sum "$BACKUP_DB" | awk '{print $1}')" == "$BEFORE_DIGEST" ]] || fail "no-clobber attempt modified prior snapshot"

# Synthetic manifest DB and one disposable local attachment.
MANIFEST_DB="$TEST_ROOT/manifest.sqlite"
ATTACH_DIR="$TEST_ROOT/attachments"
MANIFEST_DIR="$TEST_ROOT/manifests"
mkdir -m 0700 "$ATTACH_DIR" "$MANIFEST_DIR"
printf 'synthetic attachment bytes\n' > "$ATTACH_DIR/object-1"
UNUSUAL_KEY=$'legacy "quoted"\nname'
printf 'quoted/newline attachment bytes\n' > "$ATTACH_DIR/$UNUSUAL_KEY"
printf 'pipe attachment bytes\n' > "$ATTACH_DIR/legacy|pipe"
printf 'dot dot attachment bytes\n' > "$ATTACH_DIR/foo..bar"
python3 - "$MANIFEST_DB" "$ATTACH_DIR" "$UNUSUAL_KEY" <<'PY'
import hashlib, pathlib, sqlite3, sys
conn = sqlite3.connect(sys.argv[1])
conn.executescript("""
CREATE TABLE schema_migrations(version INTEGER, name TEXT, checksum TEXT, applied_at INTEGER);
CREATE TABLE themes(id TEXT, name TEXT, status TEXT, created_at INTEGER, updated_at INTEGER);
CREATE TABLE plugins(id TEXT);
CREATE TABLE attachments(id TEXT, storage_backend TEXT, storage_key TEXT, status TEXT, size_bytes INTEGER, sha256 TEXT);
INSERT INTO schema_migrations VALUES (1, 'synthetic', 'abc123', 1);
INSERT INTO themes VALUES ('theme-1', 'Synthetic', 'active', 1, 1);
INSERT INTO plugins VALUES ('plugin-1');
""")
for idx, key in enumerate(['object-1', sys.argv[3]], 1):
    data = (pathlib.Path(sys.argv[2]) / key).read_bytes()
    conn.execute("INSERT INTO attachments VALUES (?, 'local', ?, 'ready', ?, ?)", (f'att-{idx}', key, len(data), hashlib.sha256(data).hexdigest()))
# Keep an unreferenced legacy file in the filesystem inventory; it is not DB authority.
conn.commit(); conn.close()
PY

MANIFEST="$MANIFEST_DIR/s3-manifest.json"
printf 'BBLBB__STORAGE_BACKEND=local\nBBLBB__S3_SECRET_ACCESS_KEY=secret-one\n' > "$TEST_ROOT/config.env"
export BBLBB_MANIFEST_CONFIG_FILE="$TEST_ROOT/config.env"
export MOCK_AWS_MODE=pages
export TMPDIR="$TEST_ROOT"
"$ROOT/ops/backup/manifest.sh" --db "$MANIFEST_DB" --storage "$ATTACH_DIR" \
  --s3-bucket synthetic-bucket --out "$MANIFEST" > "$TEST_ROOT/manifest.log"
mode_is "$MANIFEST" 600 || fail "manifest mode is not 0600"
shopt -s nullglob
MANIFEST_TEMPS=("$TEST_ROOT"/bblbb-manifest.*)
[[ ${#MANIFEST_TEMPS[@]} -eq 0 ]] || fail "manifest success leaked its temporary work directory"
python3 - "$MANIFEST" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1], encoding="utf-8"))
assert manifest["config_sha256_nonsecret"]
assert "secret-one" not in json.dumps(manifest)
s3 = manifest["s3"]
assert s3["bucket"] == "synthetic-bucket"
assert s3["is_truncated"] is False
assert s3["inventory_only"] is True
assert s3["ready_attachment_count"] == 0
assert [v["VersionId"] for v in s3["versions"]] == ["v2", "v1", "v3"]
assert [v["VersionId"] for v in s3["delete_markers"]] == ["d1"]
assert manifest["attachments"]["count"] == 4
assert manifest["attachments"]["db_snapshot_ready_local_count"] == 2
refs = {item["path"]: item for item in manifest["attachments"]["db_snapshot_ready_local"]}
assert set(refs) == {"./object-1", './legacy "quoted"\nname'}
objects = {item["path"]: item for item in manifest["attachments"]["objects"]}
unusual = './legacy "quoted"\nname'
assert unusual in objects, repr(objects)
assert './legacy|pipe' in objects, repr(objects)
assert './foo..bar' in objects, repr(objects)
assert objects[unusual]["sha256"]
PY
[[ "$(cat "$MOCK_AWS_CALLS")" == $'|\nalpha|v2\nbeta|d1' ]] || fail "S3 pagination markers were not advanced correctly"

CONFIG_DIGEST_ONE="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["config_sha256_nonsecret"])' "$MANIFEST")"
printf 'BBLBB__STORAGE_BACKEND=s3\nBBLBB__S3_SECRET_ACCESS_KEY=secret-two\n' > "$TEST_ROOT/config.env"
CONFIG_MANIFEST="$MANIFEST_DIR/config-change.json"
"$ROOT/ops/backup/manifest.sh" --db "$MANIFEST_DB" --storage "$ATTACH_DIR" --out "$CONFIG_MANIFEST" > "$TEST_ROOT/config-change.log"
CONFIG_DIGEST_TWO="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["config_sha256_nonsecret"])' "$CONFIG_MANIFEST")"
[[ "$CONFIG_DIGEST_ONE" != "$CONFIG_DIGEST_TWO" ]] || fail "non-secret BBLBB config change did not alter manifest digest"
python3 - "$CONFIG_MANIFEST" <<'PY'
import json, sys
assert "secret-two" not in json.dumps(json.load(open(sys.argv[1])))
PY
MANIFEST_DIGEST="$(sha256sum "$MANIFEST" | awk '{print $1}')"
if "$ROOT/ops/backup/manifest.sh" --db "$MANIFEST_DB" --storage "$ATTACH_DIR" \
  --out "$MANIFEST" > "$TEST_ROOT/manifest-no-clobber.log" 2>&1; then
  fail "manifest unexpectedly replaced an existing output"
fi
[[ "$(sha256sum "$MANIFEST" | awk '{print $1}')" == "$MANIFEST_DIGEST" ]] || fail "manifest no-clobber attempt modified existing output"
MISSING_STORAGE_MANIFEST="$MANIFEST_DIR/missing-storage.json"
if "$ROOT/ops/backup/manifest.sh" --db "$MANIFEST_DB" --storage "$TEST_ROOT/no-such-storage" \
  --out "$MISSING_STORAGE_MANIFEST" > "$TEST_ROOT/manifest-missing-storage.log" 2>&1; then
  fail "standalone manifest accepted an explicitly missing storage directory"
fi
grep -q '配置的附件存储目录不可用' "$TEST_ROOT/manifest-missing-storage.log" || fail "standalone manifest missing-storage failure was not explicit"
[[ ! -e "$MISSING_STORAGE_MANIFEST" ]] || fail "failed standalone manifest published an output"
MISSING_DB="$TEST_ROOT/does-not-exist.sqlite"
if "$ROOT/ops/backup/manifest.sh" --db "$MISSING_DB" --storage "$ATTACH_DIR" --out "$MANIFEST_DIR/missing-db.json" > "$TEST_ROOT/manifest-missing-db.log" 2>&1; then
  fail "standalone manifest accepted a nonexistent database"
fi
[[ ! -e "$MISSING_DB" ]] || fail "standalone manifest created a stray database on typo"
shopt -s nullglob
MANIFEST_TEMPS=("$TEST_ROOT"/bblbb-manifest.*)
[[ ${#MANIFEST_TEMPS[@]} -eq 0 ]] || fail "manifest failure leaked its temporary work directory"

BAD_MANIFEST="$MANIFEST_DIR/missing-marker.json"
export MOCK_AWS_MODE=missing-marker
if "$ROOT/ops/backup/manifest.sh" --db "$MANIFEST_DB" --storage "$ATTACH_DIR" \
  --s3-bucket synthetic-bucket --out "$BAD_MANIFEST" > "$TEST_ROOT/manifest-bad-page.log" 2>&1; then
  fail "truncated S3 page without continuation marker was accepted"
fi
[[ ! -e "$BAD_MANIFEST" ]] || fail "failed pagination published an incomplete manifest"
REPEATED_MANIFEST="$MANIFEST_DIR/repeated-marker.json"
export MOCK_AWS_MODE=repeated-marker
if "$ROOT/ops/backup/manifest.sh" --db "$MANIFEST_DB" --storage "$ATTACH_DIR" \
  --s3-bucket synthetic-bucket --out "$REPEATED_MANIFEST" > "$TEST_ROOT/manifest-repeated-marker.log" 2>&1; then
  fail "repeated S3 pagination marker was accepted"
fi
[[ ! -e "$REPEATED_MANIFEST" ]] || fail "repeated-marker failure published a partial manifest"

# Restore only a synthetic snapshot. A legacy fixed write-test filename is left
# in place to prove the helper uses a unique mktemp name rather than truncating it.
RESTORE_DIR="$TEST_ROOT/restore"
mkdir -m 0700 "$RESTORE_DIR"
printf 'do-not-overwrite\n' > "$RESTORE_DIR/.bblbb-restore-write-test"
"$ROOT/ops/restore/sqlite.sh" "$BACKUP_DB" "$RESTORE_DIR/restored.sqlite" > "$TEST_ROOT/restore.log"
[[ "$(<"$RESTORE_DIR/.bblbb-restore-write-test")" == "do-not-overwrite" ]] || fail "restore write check clobbered predictable sentinel"
mode_is "$RESTORE_DIR/restored.sqlite" 600 || fail "restored database mode is not 0600"
shopt -s nullglob
WRITE_TESTS=("$RESTORE_DIR"/.bblbb-restore-write-test.*)
[[ ${#WRITE_TESTS[@]} -eq 0 ]] || fail "restore left unique write-test file behind"
python3 - "$RESTORE_DIR/restored.sqlite" <<'PY'
import sqlite3, sys
conn = sqlite3.connect(sys.argv[1])
last_id = conn.execute("SELECT last_id FROM state WHERE id=1").fetchone()[0]
assert last_id == conn.execute("SELECT MAX(id) FROM sample").fetchone()[0]
assert last_id == conn.execute("SELECT COUNT(*) FROM sample").fetchone()[0]
assert conn.execute("SELECT value FROM sample WHERE id=?", (last_id,)).fetchone() == (f"writer-{last_id}",)
assert conn.execute("PRAGMA integrity_check").fetchone() == ("ok",)
conn.close()
PY

# Delimiter-independent attachment verification against a synthetic DB. A pipe
# and newline in a legacy storage key must not split sqlite output rows.
VERIFY_DB="$TEST_ROOT/verify.sqlite"
VERIFY_STORAGE="$TEST_ROOT/verify-storage"
mkdir -m 0700 "$VERIFY_STORAGE"
printf 'pipe object' > "$VERIFY_STORAGE/has|pipe"
VERIFY_NEWLINE_KEY=$'has|pipe\nnewline'
printf 'newline object' > "$VERIFY_STORAGE/$VERIFY_NEWLINE_KEY"
printf 'two dots file' > "$VERIFY_STORAGE/foo..bar"
python3 - "$VERIFY_DB" "$VERIFY_STORAGE" <<'PY'
import hashlib, pathlib, sqlite3, sys
path, storage = sys.argv[1:]
conn = sqlite3.connect(path)
conn.executescript("""
CREATE TABLE attachments(id TEXT PRIMARY KEY, storage_key TEXT NOT NULL, size_bytes INTEGER NOT NULL, sha256 TEXT NOT NULL, status TEXT NOT NULL, storage_backend TEXT NOT NULL);
CREATE TABLE users(cover_attachment_id TEXT);
CREATE TABLE posts(cover_attachment_id TEXT);
""")
root = pathlib.Path(storage)
for i, key in enumerate(("has|pipe", "has|pipe\nnewline", "foo..bar")):
    content = (root / key).read_bytes()
    conn.execute("INSERT INTO attachments VALUES (?, ?, ?, ?, 'ready', 'local')", (str(i), key, len(content), hashlib.sha256(content).hexdigest()))
conn.commit(); conn.close()
PY
"$ROOT/ops/restore/verify-attachments.sh" --db "$VERIFY_DB" --storage "$VERIFY_STORAGE" > "$TEST_ROOT/verify-attachments.log" || fail "verifier rejected valid delimiter keys"

# The verifier must also fail closed on a path traversal row.
python3 - "$VERIFY_DB" <<'PY'
import sqlite3, sys
conn = sqlite3.connect(sys.argv[1])
conn.execute("INSERT INTO attachments VALUES ('unsafe', '../outside', 1, '" + "0"*64 + "', 'ready', 'local')")
conn.commit(); conn.close()
PY
if "$ROOT/ops/restore/verify-attachments.sh" --db "$VERIFY_DB" --storage "$VERIFY_STORAGE" > "$TEST_ROOT/verify-unsafe.log" 2>&1; then
  fail "verifier accepted unsafe traversal storage key"
fi

echo "PASS: synthetic SQLite online backup, manifest escaping/no-clobber, restore unique write check, S3 pagination, and attachment-key verification"
