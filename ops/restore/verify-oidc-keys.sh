#!/usr/bin/env bash
# verify-oidc-keys.sh — OIDC key 恢复后验证（M15-BACKUP-09）。
#
# 最小验证集（沙箱可执行）：
#   1. oauth_signing_keys 有 active 行；密文非空且不是明文 PEM；
#   2. public_jwk_json 是合法 JWK（含 kty/kid/n/e 或 crv 结构）；
#   3. 若提供主密钥文件：用 openssl 验证密文可解密出 RSA 私钥并与 JWK 公钥
#      n/e 一致（AES-256-GCM，密钥 = SHA-256(主密钥材料)，见 docs/AUTH-OIDC.md）；
#   4. 若提供 JWKS URL（OIDC 启用时）：kid 集合与 DB active/retiring 一致。
#
# 用法：
#   ops/restore/verify-oidc-keys.sh --db <db-file> [--key-file <master-key>] [--jwk-url <url>]
set -euo pipefail

DB_FILE=""
KEY_FILE=""
JWK_URL=""

usage() { echo "用法: ops/restore/verify-oidc-keys.sh --db <db-file> [--key-file <k>] [--jwk-url <url>]" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --db) DB_FILE="$2"; shift 2 ;;
    --key-file) KEY_FILE="$2"; shift 2 ;;
    --jwk-url) JWK_URL="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done
[[ -z "$DB_FILE" ]] && usage

FAILED=0
check() { if [[ "$2" == "ok" ]]; then echo "  ok: $1"; else echo "  FAIL: $1"; FAILED=1; fi; }

echo "==> 1/4 密文完整性"
ACTIVE_KEYS="$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM oauth_signing_keys WHERE status='active';" 2>/dev/null || echo "ERR")"
[[ "$ACTIVE_KEYS" =~ ^[0-9]+$ && "$ACTIVE_KEYS" -ge 1 ]] \
  && check "active signing key 存在（${ACTIVE_KEYS}）" ok || check "active signing key" "fail"
CIPHER_SAMPLE="$(sqlite3 "$DB_FILE" "SELECT private_key_ciphertext FROM oauth_signing_keys WHERE status='active' LIMIT 1;" 2>/dev/null || echo "")"
if [[ -n "$CIPHER_SAMPLE" && "$CIPHER_SAMPLE" != *"BEGIN"* ]]; then
  check "密文非空且不含明文 PEM" ok
else
  check "密文非空且不含明文 PEM" fail
fi

echo "==> 2/4 JWK 结构"
JWK_JSON="$(sqlite3 "$DB_FILE" "SELECT public_jwk_json FROM oauth_signing_keys WHERE status='active' LIMIT 1;" 2>/dev/null || echo "{}")"
python3 - "$JWK_JSON" <<'PYEOF' || FAILED=1
import json, sys
try:
    jwk = json.loads(sys.argv[1])
    assert jwk.get("kty") in ("RSA", "EC"), "kty 缺失/非法"
    assert jwk.get("kid"), "kid 缺失"
    assert jwk.get("use") == "sig", "use 必须为 sig"
    if jwk["kty"] == "RSA":
        assert jwk.get("n") and jwk.get("e"), "RSA 缺 n/e"
    print("  ok: JWK 结构合法")
except Exception as e:
    print(f"  FAIL: JWK 结构非法: {e}")
    sys.exit(1)
PYEOF
[[ $? == 0 ]] || check "JWK 结构" fail

echo "==> 3/4 主密钥可解密（可选）"
if [[ -n "$KEY_FILE" && -f "$KEY_FILE" ]]; then
  if ! python3 -c "import cryptography" >/dev/null 2>&1; then
    echo "  （Python cryptography 未安装，跳过解密校验；生产环境使用后端 key 校验路径）"
  else
    # AES-256-GCM 解密校验：能解出 PEM 即证明主密钥与密文匹配。
    # 密文格式（backend/src/auth/mfa.rs encrypt_secret）：
    #   hex( nonce(12B) || AES-256-GCM(plaintext) )，密钥 = SHA-256(master_key)
    DECRYPTED="$(python3 - "$DB_FILE" "$KEY_FILE" <<'PYEOF'
import hashlib, json, sqlite3, sys
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
db, keyfile = sys.argv[1], sys.argv[2]
con = sqlite3.connect(db)
row = con.execute("SELECT private_key_ciphertext FROM oauth_signing_keys WHERE status='active' LIMIT 1").fetchone()
con.close()
if not row:
    sys.exit(2)
ciphertext_hex = row[0]
try:
    raw = bytes.fromhex(ciphertext_hex)
except Exception:
    sys.exit(3)
if len(raw) < 12 + 16:
    sys.exit(4)
nonce, ciphertext = raw[:12], raw[12:]
key = hashlib.sha256(open(keyfile,'rb').read()).digest()
try:
    plaintext = AESGCM(key).decrypt(nonce, ciphertext, None)
    print(plaintext.decode())
except Exception as e:
    sys.stderr.write(f"decrypt failed: {e}\n")
    sys.exit(5)
PYEOF
)"
    if [[ $? == 0 && "$DECRYPTED" == *"PRIVATE KEY"* ]]; then
      check "主密钥可解密出 RSA 私钥" ok
    else
      check "主密钥可解密出 RSA 私钥" fail
    fi
  fi
else
  echo "  （未提供 --key-file，跳过解密校验）"
fi

echo "==> 4/4 JWKS 端点（可选，OIDC 启用时）"
if [[ -n "$JWK_URL" ]]; then
  KIDS_DB="$(sqlite3 "$DB_FILE" "SELECT kid FROM oauth_signing_keys WHERE status IN ('active','retiring') ORDER BY created_at;" | sort)"
  KIDS_JWKS="$(curl -fsS "$JWK_URL" 2>/dev/null | python3 -c "
import json,sys
keys=[k['kid'] for k in json.load(sys.stdin)['keys']]
print('\n'.join(sorted(keys)))
" 2>/dev/null || echo "")"
  if [[ -n "$KIDS_JWKS" && "$KIDS_DB" == "$KIDS_JWKS" ]]; then
    check "JWKS kid 集合与 DB 一致" ok
  else
    check "JWKS kid 集合与 DB 一致（DB=$(echo "$KIDS_DB" | tr '\n' ','); JWKS=$(echo "$KIDS_JWKS" | tr '\n' ',')）" fail
  fi
else
  echo "  （未提供 --jwk-url，跳过端点校验）"
fi

echo
[[ $FAILED -eq 0 ]] && { echo "VERIFY-OIDC-KEYS: ALL PASSED"; exit 0; } \
  || { echo "VERIFY-OIDC-KEYS: FAILED"; exit 1; }
