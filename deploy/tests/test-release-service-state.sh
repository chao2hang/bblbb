#!/usr/bin/env bash
# Synthetic tests for release-service-state.sh; mock systemctl and temp paths only.
set -euo pipefail
umask 077
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
TMP_PARENT="${TMPDIR:-/tmp}"
TEST_ROOT="$(mktemp -d "$TMP_PARENT/bblbb-release-state-tests.XXXXXX")"
TEST_ROOT="$(cd "$TEST_ROOT" && pwd -P)"
case "$TEST_ROOT" in "$TMP_PARENT"/bblbb-release-state-tests.*) ;; *) echo "unsafe test root" >&2; exit 1 ;; esac
cleanup() {
  case "${TEST_ROOT:-}" in "$TMP_PARENT"/bblbb-release-state-tests.*) [[ ! -d "$TEST_ROOT" || -L "$TEST_ROOT" ]] || rm -rf -- "$TEST_ROOT" ;; *) return 1 ;; esac
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
MOCK_BIN="$TEST_ROOT/bin"
mkdir -m 0700 "$MOCK_BIN"
cat > "$MOCK_BIN/systemctl" <<'MOCK_SYSTEMCTL'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$MOCK_SYSTEMCTL_LOG"
command="$1"; shift
if [[ "$command" == show ]]; then
  unit="${*: -1}"
  case "$unit" in
    bblbb-backend) printf '%s\n' "${MOCK_BACKEND_STATE:-active}" ;;
    bblbb-worker) printf '%s\n' "${MOCK_WORKER_STATE:-active}" ;;
    bblbb-frontend) printf '%s\n' "${MOCK_FRONTEND_STATE:-active}" ;;
  esac
  exit 0
fi
if [[ "$command" == stop && "${MOCK_FAIL_STOP_UNIT:-}" == "$1" ]]; then exit 1; fi
if [[ "$command" == start && "${MOCK_FAIL_START_UNIT:-}" == "$1" ]]; then exit 1; fi
exit 0
MOCK_SYSTEMCTL
chmod 0700 "$MOCK_BIN/systemctl"
export PATH="$MOCK_BIN:$PATH"
export MOCK_SYSTEMCTL_LOG="$TEST_ROOT/systemctl.log"

# Helper can acquire the release lock and rejects concurrent processes.
source "$ROOT/deploy/scripts/release-service-state.sh"
RELEASES_DIR="$TEST_ROOT/releases"
release_acquire_lock "$RELEASES_DIR" 1 || fail "failed to create/acquire lock"
[[ "$(stat -c '%a' "$RELEASES_DIR/.release.lock")" == 600 ]] || fail "lock mode is not 0600"
if bash -c 'source "$1"; release_acquire_lock "$2" 0' _ "$ROOT/deploy/scripts/release-service-state.sh" "$RELEASES_DIR" >/dev/null 2>&1; then
  fail "second process acquired held release lock"
fi

# Partial stop before migration restores only previously active services.
: > "$MOCK_SYSTEMCTL_LOG"
release_capture_service_state || fail "could not capture active state"
export MOCK_FAIL_STOP_UNIT=bblbb-worker
if release_stop_stack; then fail "partial stop failure was ignored"; fi
unset MOCK_FAIL_STOP_UNIT
[[ "$(grep -c '^start ' "$MOCK_SYSTEMCTL_LOG")" -eq 3 ]] || fail "pre-migration recovery did not restore active services"

# Failed units are accepted in explicit rollback recovery mode, then stop/cutover is fail-closed.
export MOCK_BACKEND_STATE=failed MOCK_WORKER_STATE=inactive MOCK_FRONTEND_STATE=active
release_capture_service_state allow-nonactive || fail "rollback rejected failed/inactive service state"
release_stop_stack || fail "rollback stop failed"
RELEASE_CUTOVER_STARTED=1
RELEASE_MIGRATION_STARTED=1
: > "$MOCK_SYSTEMCTL_LOG"
release_handle_exit 1
[[ "$(grep -c '^stop ' "$MOCK_SYSTEMCTL_LOG")" -eq 3 ]] || fail "uncertain cutover did not fail-closed stop every service"
! grep -q '^start ' "$MOCK_SYSTEMCTL_LOG" || fail "uncertain cutover started a service automatically"

# Transitioning state remains rejected rather than racing an in-flight systemd job.
MOCK_BACKEND_STATE=activating
if release_capture_service_state allow-nonactive; then fail "transitioning systemd unit state was accepted"; fi

echo "PASS: release lock, partial-stop recovery, failed-unit rollback state, and fail-closed exit"
