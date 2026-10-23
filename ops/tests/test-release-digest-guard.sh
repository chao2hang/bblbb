#!/usr/bin/env bash
# Exercises release digest gate using only a synthetic invalid bundle. It must
# stop before reading env configuration, touching a DB, or invoking system tools.
set -euo pipefail
umask 077
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
TMP_PARENT="${TMPDIR:-/tmp}"
TEST_ROOT="$(mktemp -d "$TMP_PARENT/bblbb-release-digest-tests.XXXXXX")"
TEST_ROOT="$(cd "$TEST_ROOT" && pwd -P)"
case "$TEST_ROOT" in "$TMP_PARENT"/bblbb-release-digest-tests.*) ;; *) echo "unsafe test path" >&2; exit 1;; esac
cleanup() {
  case "${TEST_ROOT:-}" in
    "$TMP_PARENT"/bblbb-release-digest-tests.*) [[ ! -d "$TEST_ROOT" || -L "$TEST_ROOT" ]] || rm -rf -- "$TEST_ROOT" ;;
    *) echo "refusing unsafe cleanup" >&2; return 1 ;;
  esac
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
BUNDLE="$TEST_ROOT/not-a-tar.gz"
printf 'synthetic invalid archive\n' > "$BUNDLE"
BUNDLE_HASH="$(sha256sum "$BUNDLE" | awk '{print $1}')"

if env -u BBLBB_RELEASE_EXPECTED_SHA256 BBLBB_ENV_FILE="$TEST_ROOT/no-env" \
  bash "$ROOT/deploy/scripts/release.sh" --bundle "$BUNDLE" --releases-dir "$TEST_ROOT/releases" \
  > "$TEST_ROOT/missing.log" 2>&1; then
  fail "release accepted a missing trusted digest"
fi
grep -q '必须提供带外可信' "$TEST_ROOT/missing.log" || { sed -n '1,100p' "$TEST_ROOT/missing.log" >&2; fail "missing digest rejected for the wrong reason"; }

if BBLBB_RELEASE_EXPECTED_SHA256="$(printf '0%.0s' {1..64})" BBLBB_ENV_FILE="$TEST_ROOT/no-env" \
  bash "$ROOT/deploy/scripts/release.sh" --bundle "$BUNDLE" --releases-dir "$TEST_ROOT/releases" \
  > "$TEST_ROOT/mismatch.log" 2>&1; then
  fail "release accepted a mismatched trusted digest"
fi
grep -q 'SHA-256 不匹配' "$TEST_ROOT/mismatch.log" || fail "mismatched digest rejected for the wrong reason"

mkdir -m 0700 "$TEST_ROOT/storage"
if BBLBB_RELEASE_EXPECTED_SHA256="$BUNDLE_HASH" BBLBB_ENV_FILE="$TEST_ROOT/no-env" BBLBB__DATABASE_URL="sqlite:///$TEST_ROOT/synthetic.sqlite" BBLBB__STORAGE_DIR="$TEST_ROOT/storage" \
  bash "$ROOT/deploy/scripts/release.sh" --bundle "$BUNDLE" --releases-dir "$TEST_ROOT/releases" \
  > "$TEST_ROOT/matching-invalid-bundle.log" 2>&1; then
  fail "release accepted an invalid synthetic archive"
fi
grep -q 'bundle 路径或 VERSION 预检失败' "$TEST_ROOT/matching-invalid-bundle.log" || { sed -n '1,100p' "$TEST_ROOT/matching-invalid-bundle.log" >&2; fail "matching digest did not pass digest gate"; }
[[ -f "$TEST_ROOT/releases/.release.lock" ]] || fail "digest guard did not exercise guarded release path setup"
[[ ! -e "$TEST_ROOT/releases/invalid" && ! -e "$TEST_ROOT/current" ]] || fail "invalid archive created a version or switched current"

# A symlink bundle is rejected before snapshot creation.
ln -s "$BUNDLE" "$TEST_ROOT/bundle-link"
if BBLBB_RELEASE_EXPECTED_SHA256="$BUNDLE_HASH" "$ROOT/deploy/scripts/release.sh" \
  --bundle "$TEST_ROOT/bundle-link" --releases-dir "$TEST_ROOT/releases" > "$TEST_ROOT/symlink.log" 2>&1; then
  fail "release accepted symlink bundle source"
fi

echo "PASS: release requires trusted digest, rejects mismatch and symlink, and stops invalid bundle before DB/system actions"
