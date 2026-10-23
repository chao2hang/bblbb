#!/usr/bin/env bash
# Validate shipped systemd units against a disposable filesystem root.
# No host services, credentials, or deployment paths are accessed.
# Requires systemd with LoadCredential and optional credential-source support.
set -euo pipefail
umask 077

if ! command -v systemd-analyze >/dev/null 2>&1; then
  echo "SKIP: systemd-analyze is unavailable"
  exit 0
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
TMP_PARENT="${TMPDIR:-/tmp}"
[[ "$TMP_PARENT" == /* && -d "$TMP_PARENT" && ! -L "$TMP_PARENT" ]] || {
  echo "unsafe TMPDIR for systemd unit test" >&2
  exit 1
}
TMP_PARENT="$(cd "$TMP_PARENT" && pwd -P)"
TEST_ROOT="$(mktemp -d "$TMP_PARENT/bblbb-systemd-unit-tests.XXXXXX")"
TEST_ROOT="$(cd "$TEST_ROOT" && pwd -P)"
case "$TEST_ROOT" in "$TMP_PARENT"/bblbb-systemd-unit-tests.*) ;; *) echo "unsafe test root" >&2; exit 1 ;; esac
cleanup() {
  local resolved
  [[ -n "${TEST_ROOT:-}" && -d "$TEST_ROOT" && ! -L "$TEST_ROOT" ]] || return 0
  resolved="$(realpath -e "$TEST_ROOT")"
  case "$resolved" in "$TMP_PARENT"/bblbb-systemd-unit-tests.*) rm -rf -- "$resolved" ;; *) echo "refusing unsafe cleanup: $resolved" >&2; return 1 ;; esac
}
trap cleanup EXIT

mkdir -p "$TEST_ROOT/etc/systemd/system" "$TEST_ROOT/etc/bblbb/secrets" \
  "$TEST_ROOT/opt/bblbb/current/backend" "$TEST_ROOT/opt/bblbb/current/ops/backup" \
  "$TEST_ROOT/opt/bblbb/current/frontend/build" "$TEST_ROOT/opt/bblbb/current/frontend/node_modules" \
  "$TEST_ROOT/var/lib/bblbb" "$TEST_ROOT/usr/bin"
cp "$ROOT"/deploy/systemd/bblbb-{backend,worker,frontend,backup}.service \
   "$ROOT/deploy/systemd/bblbb-backup.timer" "$TEST_ROOT/etc/systemd/system/"

# Empty synthetic executables are only checked for existence/mode by verify.
for executable in \
  "$TEST_ROOT/opt/bblbb/current/backend/bblbb-backend" \
  "$TEST_ROOT/opt/bblbb/current/ops/backup/daily.sh" \
  "$TEST_ROOT/usr/bin/env" \
  "$TEST_ROOT/opt/bblbb/current/backend/bblbb-migrate" \
  "$TEST_ROOT/opt/bblbb/current/frontend/node"; do
  mkdir -p "$(dirname "$executable")"
  printf '#!/bin/sh\nexit 0\n' > "$executable"
  chmod 0755 "$executable"
done
: > "$TEST_ROOT/etc/bblbb/backend.env"
: > "$TEST_ROOT/etc/bblbb/frontend.env"
chmod 0640 "$TEST_ROOT/etc/bblbb/backend.env" "$TEST_ROOT/etc/bblbb/frontend.env"
# Required credential exists; all feature-specific credentials intentionally do not.
: > "$TEST_ROOT/etc/bblbb/secrets/settings_encryption_key"
chmod 0600 "$TEST_ROOT/etc/bblbb/secrets/settings_encryption_key"

systemd-analyze verify --root="$TEST_ROOT" --recursive-errors=no \
  "$TEST_ROOT/etc/systemd/system/bblbb-backend.service" \
  "$TEST_ROOT/etc/systemd/system/bblbb-worker.service" \
  "$TEST_ROOT/etc/systemd/system/bblbb-frontend.service" \
  "$TEST_ROOT/etc/systemd/system/bblbb-backup.service" \
  "$TEST_ROOT/etc/systemd/system/bblbb-backup.timer" 2>"$TEST_ROOT/systemd-verify.log" || {
    cat "$TEST_ROOT/systemd-verify.log" >&2
    exit 1
  }
if [[ -s "$TEST_ROOT/systemd-verify.log" ]]; then
  cat "$TEST_ROOT/systemd-verify.log" >&2
  echo "systemd unit verification emitted diagnostics" >&2
  exit 1
fi
echo "PASS: systemd units parse with required credential present and optional credentials absent"
