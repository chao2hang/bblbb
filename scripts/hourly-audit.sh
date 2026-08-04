#!/usr/bin/env bash
#
# hourly-audit.sh — BBLBB 每小时代码变更审计（机械部分）
#
# 职责（由调度任务调用；本脚本只做确定性验证与安全提交，不做主观判断）：
#   1. 原子锁避免重叠运行，陈旧锁（>3h）可替换。
#   2. 快照工作区与候选文件 hash，检测并发编辑。
#   3. 分组验证：
#      - docs 组（TODO.md、todo/、scripts/*.rb）：ruby 路线图校验。
#      - code 组（backend/frontend/prototype/migrations/工具链）：fmt、clippy、
#        test、svelte-check、原型、SQLite 迁移全部通过且无 Secret/生产 URL 命中。
#   4. 只有验证通过且未被并发修改的组才按路径显式暂存并普通提交。
#   5. fetch origin/main，确认 fast-forward 后普通 push；禁止 force-push。
#   6. 每次运行追加一条机械审计记录到 todo/issue.md 的"机械审计日志"表。
#
# 不做：reset --hard、clean、checkout --、stash、rebase、amend、force-push、删分支。
# 不做：自动修复问题代码；发现问题组保持未提交，由维护者/代理写入问题表。

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

RUN_ID="AUDIT-$(date -u +%Y%m%d-%H%M)"
LOCK_DIR="/tmp/bblbb-hourly-code-audit.lock"
ISSUE_FILE="todo/issue.md"
STAMP_DIR="/tmp/bblbb-hourly-audit-$$"
LOG_FILE="$STAMP_DIR.log"
mkdir -p "$STAMP_DIR"

DOCS_ENTRIES="TODO.md todo scripts/check-roadmap.rb scripts/sync-operation-coverage.rb"
CODE_ENTRIES="backend frontend prototype migrations Makefile rust-toolchain.toml .nvmrc README.md backend/README.md scripts/deploy.sh scripts/watch-todo.sh"

info()  { echo ">>> $*" | tee -a "$LOG_FILE"; }
fail()  { echo "!!  $*" | tee -a "$LOG_FILE"; }

# ── 工具 ──────────────────────────────────────────────────────────────────
path_in_group() {
  local p="$1" entry
  shift
  for entry in "$@"; do
    if [ "$p" = "$entry" ] || [[ "$p" == "$entry"/* ]]; then
      return 0
    fi
  done
  return 1
}

group_has_changes() {
  local entry
  for entry in "$@"; do
    if [ -e "$entry" ]; then
      if git status --porcelain -- "$entry" 2>/dev/null | grep -q .; then
        return 0
      fi
    fi
  done
  return 1
}

# 并发编辑检测：快照后文件内容是否变化
verify_unchanged() {
  local p
  while IFS= read -r p; do
    [ -f "$p" ] || continue
    grep -Fxq "$(shasum -a 256 "$p")" "$STAMP_DIR/hashes" || return 1
  done < <(git status --porcelain | sed 's/^[^ ]* //' | sort -u)
  return 0
}

# ── 1. 原子锁 ─────────────────────────────────────────────────────────────
acquire_lock() {
  if mkdir "$LOCK_DIR" 2>/dev/null; then
    printf 'started_at=%s\npid=%s\nrun=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$$" "$RUN_ID" > "$LOCK_DIR/owner"
    return 0
  fi
  local started started_epoch now_epoch age
  started="$(sed -n 's/^started_at=//p' "$LOCK_DIR/owner" 2>/dev/null | head -1)"
  if [ -n "$started" ]; then
    started_epoch="$(date -u -j -f '%Y-%m-%dT%H:%M:%SZ' "$started" +%s 2>/dev/null || echo 0)"
    now_epoch="$(date -u +%s)"
    age=$(( now_epoch - started_epoch ))
    if [ "$age" -gt 10800 ]; then
      rm -rf "$LOCK_DIR"
      mkdir "$LOCK_DIR"
      printf 'started_at=%s\npid=%s\nrun=%s\n(stale replaced)\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$$" "$RUN_ID" > "$LOCK_DIR/owner"
      return 0
    fi
  fi
  echo "SKIP: previous audit still running ($LOCK_DIR)"
  return 1
}

release_lock() {
  rm -rf "$LOCK_DIR" "$STAMP_DIR"
}

trap release_lock EXIT

if ! acquire_lock; then
  exit 0
fi

# ── 2. 快照 ───────────────────────────────────────────────────────────────
info "audit start $RUN_ID"
git fetch origin main >/dev/null 2>&1 || true
HEAD_BEFORE="$(git rev-parse HEAD)"

# 候选文件 hash（工作区内容）
git status --porcelain | sed 's/^[^ ]* //' | sort -u > "$STAMP_DIR/changed" || true
while IFS= read -r p; do
  [ -f "$p" ] && shasum -a 256 "$p"
done < "$STAMP_DIR/changed" | sort > "$STAMP_DIR/hashes" || true

# ── 3. 分组验证 ───────────────────────────────────────────────────────────
docs_green=0
code_green=0
fails=""

if ruby scripts/check-roadmap.rb >"$STAMP_DIR/docs.log" 2>&1 &&
   ruby scripts/sync-operation-coverage.rb --check >>"$STAMP_DIR/docs.log" 2>&1 &&
   git diff --check >/dev/null 2>&1; then
  docs_green=1
  info "docs group: roadmap + OpenAPI coverage checks passed"
else
  fails="docs:roadmap/coverage"
  fail "docs group: ruby roadmap/coverage checks FAILED (see $STAMP_DIR/docs.log)"
fi

code_fails=""

if [ -d backend ]; then
  ( cd backend && cargo fmt --all -- --check ) >"$STAMP_DIR/cargo-fmt.log" 2>&1 || code_fails="${code_fails}cargo-fmt|"
  ( cd backend && cargo clippy --workspace --all-targets --all-features -- -D warnings ) >"$STAMP_DIR/cargo-clippy.log" 2>&1 || code_fails="${code_fails}cargo-clippy|"
  ( cd backend && cargo test --workspace --all-features ) >"$STAMP_DIR/cargo-test.log" 2>&1 || code_fails="${code_fails}cargo-test|"
fi
if [ -f frontend/package.json ]; then
  ( cd frontend && npm run check ) >"$STAMP_DIR/frontend-check.log" 2>&1 || code_fails="${code_fails}frontend-check|"
fi
if [ -f prototype/package.json ]; then
  ( cd prototype && npm run check:all ) >"$STAMP_DIR/prototype-check.log" 2>&1 || code_fails="${code_fails}prototype-check|"
fi

sqlite_ok=1
tmpdb="$(mktemp -d)/audit.sqlite"
for m in migrations/sqlite/*.sql; do
  sqlite3 -bail "$tmpdb" < "$m" >/dev/null 2>&1 || { sqlite_ok=0; break; }
done
if [ "$sqlite_ok" -eq 1 ] && [ -z "$(sqlite3 "$tmpdb" 'PRAGMA foreign_key_check;' 2>/dev/null)" ]; then
  info "code group [sqlite-migrations]: ok"
else
  code_fails="${code_fails}sqlite-migrations|"
  fail "code group [sqlite-migrations]: FAILED"
fi

# Secret / 生产 URL 扫描（仅针对 code 组候选文件）
secret_hits=0
produrl_hits=0
while IFS= read -r p; do
  [ -f "$p" ] || continue
  if path_in_group "$p" $CODE_ENTRIES; then
    if grep -qE '(AKIA[0-9A-Z]{16}|sk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{36}|-----BEGIN (RSA |EC )?PRIVATE KEY-----)' "$p" 2>/dev/null; then
      secret_hits=$((secret_hits + 1))
      fail "secret pattern in $p"
    fi
    if grep -qE '(186\.241\.84\.165|bblbb\.com|root@[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)' "$p" 2>/dev/null; then
      produrl_hits=$((produrl_hits + 1))
      fail "production URL in $p"
    fi
  fi
done < "$STAMP_DIR/changed"
[ "$secret_hits" -eq 0 ] || code_fails="${code_fails}secret-scan|"
[ "$produrl_hits" -eq 0 ] || code_fails="${code_fails}prod-url-scan|"

if [ -z "$code_fails" ]; then
  code_green=1
  info "code group: all checks passed"
else
  info "code group: blocked (${code_fails})"
fi

# ── 4. 安全提交 ───────────────────────────────────────────────────────────
committed=""
if [ "$docs_green" -eq 1 ]; then
  if group_has_changes $DOCS_ENTRIES; then
    if verify_unchanged; then
      git add -- $DOCS_ENTRIES
      git commit -m "docs: sync roadmap audit ($RUN_ID)" >/dev/null 2>&1 \
        && committed="docs" || fail "docs group commit failed"
    else
      fail "docs group: concurrent edit detected; skipped"
    fi
  fi
fi

if [ "$code_green" -eq 1 ]; then
  if group_has_changes $CODE_ENTRIES; then
    if verify_unchanged; then
      git add -- $CODE_ENTRIES
      if git diff --cached --check >/dev/null 2>&1; then
        git commit -m "feat: verified build baseline ($RUN_ID)" >/dev/null 2>&1 \
          && committed="${committed:+$committed }code" || fail "code group commit failed"
      else
        fail "code group: staged diff --check failed; unstaged"
        git reset -q -- . >/dev/null 2>&1 || true
      fi
    else
      fail "code group: concurrent edit detected; skipped"
    fi
  fi
fi

# ── 5. 推送（fast-forward 校验） ──────────────────────────────────────────
pushed="no"
if [ -n "$committed" ]; then
  git fetch origin main >/dev/null 2>&1 || true
  if git merge-base --is-ancestor origin/main HEAD 2>/dev/null; then
    if git push origin HEAD:main >/dev/null 2>&1; then
      pushed="yes"
      info "pushed to origin/main: ${committed}"
    else
      fail "push to origin/main FAILED (non-force; left local)"
    fi
  else
    fail "origin/main not an ancestor of HEAD; skipped push (no merge/rebase)"
  fi
fi

# ── 6. 审计记录追加 ───────────────────────────────────────────────────────
pending="$(git status --porcelain | wc -l | tr -d ' ')"
{
  echo ""
  echo "### $RUN_ID"
  echo ""
  echo "| 字段 | 值 |"
  echo "|---|---|"
  echo "| 时间 | $(date -u +%Y-%m-%dT%H:%M:%SZ) |"
  echo "| 起始 HEAD | $HEAD_BEFORE |"
  echo "| 结束 HEAD | $(git rev-parse HEAD) |"
  echo "| docs 组 | $([ "$docs_green" -eq 1 ] && echo green || echo blocked) |"
  echo "| code 组 | $([ "$code_green" -eq 1 ] && echo green || echo blocked) |"
  echo "| 失败检查 | ${code_fails:-none} |"
  echo "| 提交 | ${committed:-none} |"
  echo "| 推送 | $pushed |"
  echo "| 保留未提交 | ${pending} 项 |"
} >> "$ISSUE_FILE"

info "audit end $RUN_ID"
echo "RESULT docs=$docs_green code=$code_green pushed=$pushed"
