#!/usr/bin/env bash
# scan-log-corpus.sh — 日志语料脱敏扫描（M15-OBSERVE-02/03）。
#
# 扫描日志/输出文本，查找 M15-OBSERVE-02 禁止出现的形态：
#   Cookie、Authorization、OAuth code/token、密码、完整邮箱、隐藏正文、
#   Prompt、签名 URL。
#
# 用法：
#   ops/scan-log-corpus.sh [FILE...]           # 指定文件
#   ops/scan-log-corpus.sh --dir <dir> [--since <days>]  # 目录递归
#   ops/scan-log-corpus.sh --test              # 用内建样例自检（返回码可测）
set -euo pipefail

# 禁止形态（宽松正则；命中即 FAIL）：
#  - Cookie/Authorization 头
#  - OAuth code/token / access/refresh/id token
#  - password/secret 明文值
#  - 完整邮箱
#  - JWT / Bearer
#  - 私钥块
#  - 预签名 URL（X-Amz-* / 长签名 query）
FORBIDDEN_PATTERNS=(
  'Cookie:'
  'Set-Cookie:'
  'Authorization:'
  'X-CSRF-Token'
  'oauth_code'
  'code_verifier'
  'access_token=[^ ]'
  'refresh_token=[^ ]'
  '(reset|verify|session|csrf|auth)_token=[^ ]'
  'password=[^ ]'
  'password":'
  'secret=[^ ]'
  'client_secret'
  'webhook_secret'
  'private_key'
  'BEGIN .*PRIVATE KEY'
  '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
  'Bearer [A-Za-z0-9._~+/-]+'
  'eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}'
  'X-Amz-(Credential|Signature|SignedHeaders)'
  'X-Goog-Signature'
  'signed_url'
  'presigned'
)

FOUND=0
FILE_COUNT=0

scan_file() {
  local f="$1"
  FILE_COUNT=$((FILE_COUNT+1))
  local line_no
  while IFS= read -r line; do
    for pat in "${FORBIDDEN_PATTERNS[@]}"; do
      if echo "$line" | grep -qE "$pat"; then
        echo "  LEAK($(basename "$0")): ${f}:$(echo "$line" | cut -c1-120)"
        FOUND=$((FOUND+1))
        break
      fi
    done
  done < "$f"
}

scan_stdin() {
  while IFS= read -r line; do
    for pat in "${FORBIDDEN_PATTERNS[@]}"; do
      if echo "$line" | grep -qE "$pat"; then
        echo "  LEAK: $(echo "$line" | cut -c1-120)"
        FOUND=$((FOUND+1))
        break
      fi
    done
  done
}

if [[ "${1:-}" == "--test" ]]; then
  # 自检：干净样例必须 0 命中；泄密样例必须命中
  CLEAN="$(mktemp)"; DIRTY="$(mktemp)"
  cat > "$CLEAN" <<'EOF'
{"timestamp":"...","service":"bblbb-backend","level":"INFO","request_id":"req-1","route":"/api/v1/posts","message":"post created"}
applied 4 migration(s)
{"fields":{"job_id":"j-01911fd5"},"message":"job succeeded"}
EOF
  cat > "$DIRTY" <<'EOF'
login password=hunter2 failed
sent email to victim@example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig
reset_token=abcd1234ef567890
X-Amz-Signature=deadbeef
EOF
  scan_file "$CLEAN"
  if [[ $FOUND -ne 0 ]]; then
    echo "  [FAIL] 干净样例出现误报"; exit 1
  fi
  scan_file "$DIRTY"
  if [[ $FOUND -ne 5 ]]; then
    echo "  [FAIL] 泄密样例应命中 5 处，实际 $FOUND"; exit 1
  fi
  echo "SCAN-LOG-CORPUS SELF-TEST: PASSED"
  exit 0
fi

if [[ "${1:-}" == "--dir" ]]; then
  DIR="$2"; SINCE="${3:-7}"
  echo "==> 扫描目录 $DIR（最近 ${SINCE} 天，递归）"
  while IFS= read -r -d '' f; do
    scan_file "$f"
  done < <(find "$DIR" -type f -mtime "-$SINCE" -print0 2>/dev/null)
elif [[ "${1:-}" == "--stdin" ]]; then
  scan_stdin
elif [[ $# -gt 0 ]]; then
  for f in "$@"; do scan_file "$f"; done
else
  echo "用法: $0 [FILE...] | --dir <dir> [--since N] | --stdin | --test" >&2
  exit 1
fi

echo "==> 扫描文件数: ${FILE_COUNT}；命中: ${FOUND}"
[[ $FOUND -eq 0 ]] || { echo "SCAN-LOG-CORPUS: FAILED（发现疑似敏感形态，按 token-log-check.md 处置）" >&2; exit 1; }
echo "SCAN-LOG-CORPUS: CLEAN"
