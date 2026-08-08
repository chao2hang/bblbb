#!/usr/bin/env bash
# manifest.sh — 备份清单（M15-BACKUP-03）。
#
# 备份内容清单：
#   1. 迁移版本与 checksum（schema_migrations 全表）；
#   2. 数据型主题/插件配置（themes/theme_revisions/plugins 行 hash）；
#   3. 配置版本（非 Secret 配置摘要；Secret 只登记名称不取值）；
#   4. 附件 manifest（本地对象逐个 sha256；S3 bucket 用 list-object-versions，
#      真实 S3 演练由 M15-BACKUP-03 [!] 阻塞项跟踪）。
#
# 用法：
#   ops/backup/manifest.sh --db <db-file> --storage <dir> --out <manifest.json>
#   ops/backup/manifest.sh --db <db-file> --s3-bucket <bucket> --out <manifest.json>
set -euo pipefail

DB_FILE=""
STORAGE_DIR=""
S3_BUCKET=""
OUT_FILE=""
CONFIG_FILES=("/etc/bblbb/backend.env")

usage() { echo "用法见脚本头部注释" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db) DB_FILE="$2"; shift 2 ;;
    --storage) STORAGE_DIR="$2"; shift 2 ;;
    --s3-bucket) S3_BUCKET="$2"; shift 2 ;;
    --out) OUT_FILE="$2"; shift 2 ;;
    *) echo "未知参数: $1"; usage ;;
  esac
done

[[ -z "$DB_FILE" || -z "$OUT_FILE" ]] && usage
WORK="$(mktemp -d)"
MANIFEST="$WORK/manifest-partial.json"

# 1) 迁移版本与 checksum
echo "==> 迁移版本/checksum"
sqlite3 -json "$DB_FILE" \
  "SELECT version, name, checksum, applied_at FROM schema_migrations ORDER BY version;" \
  > "$WORK/migrations.json"

# 2) 主题/插件行 hash
echo "==> 主题/插件配置摘要"
sqlite3 -json "$DB_FILE" \
  "SELECT 'themes' AS metric, COUNT(*) AS value FROM themes
   UNION ALL
   SELECT 'plugins', COUNT(*) FROM plugins;" \
  > "$WORK/theme-summary.json" || echo '[{"metric":"themes","value":0},{"metric":"plugins","value":0}]' > "$WORK/theme-summary.json"
sqlite3 -json "$DB_FILE" \
  "SELECT id, name, status, created_at, updated_at FROM themes ORDER BY created_at;" \
  > "$WORK/themes.json" || echo "[]" > "$WORK/themes.json"

# 3) 配置版本（去 Secret 的摘要）
echo "==> 配置摘要"
CONFIG_SUM=""
for f in "${CONFIG_FILES[@]}"; do
  if [[ -f "$f" ]]; then
    # 只取非 Secret 行（去掉 BBLBB__*_KEY / _SECRET / _PASSWORD / DSN 含密码行）
    SUM="$(grep -E '^[A-Z_]+=' "$f" | grep -vE '(KEY|SECRET|PASSWORD|TOKEN|DATABASE_URL)=.*(pass|secret|token)' | shasum -a 256 | awk '{print $1}')"
    CONFIG_SUM="$CONFIG_SUM $f=$SUM"
  fi
done

# 4) 附件 manifest
echo "==> 附件 manifest"
if [[ -n "$STORAGE_DIR" && -d "$STORAGE_DIR" ]]; then
  ( cd "$STORAGE_DIR" && find . -type f | sort | while read -r f; do
      printf '{"path":"%s","size":%s,"sha256":"%s"}\n' \
        "$f" "$(stat -f%z "$f")" "$(shasum -a 256 "$f" | awk '{print $1}')"
    done | python3 -c "import sys,json;rows=[json.loads(l) for l in sys.stdin];print(json.dumps(rows))" \
  ) > "$WORK/attachments.json"
  ATTACH_COUNT="$(python3 -c "import json;print(len(json.load(open('$WORK/attachments.json'))))")"
else
  echo "[]" > "$WORK/attachments.json"
  ATTACH_COUNT=0
fi

# S3：list-object-versions（真实 S3 演练 [!]；脚本就绪供生产使用）
if [[ -n "$S3_BUCKET" ]]; then
  if command -v aws >/dev/null 2>&1; then
    aws s3api list-object-versions --bucket "$S3_BUCKET" --output json \
      > "$WORK/s3-versions.json" || echo "S3 清单获取失败（真实演练阻塞项）" > "$WORK/s3-versions.json"
  else
    echo '{"error":"aws cli 未安装"}' > "$WORK/s3-versions.json"
  fi
fi

# 组装
python3 - "$WORK" "$DB_FILE" "$STORAGE_DIR" "$S3_BUCKET" "$CONFIG_SUM" "$ATTACH_COUNT" <<'PYEOF' > "$OUT_FILE"
import json, sys
work, db, storage, s3, config_sum, attach_count = sys.argv[1:]
out = {
  "tool": "ops/backup/manifest.sh",
  "generated_at": __import__("datetime").datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ"),
  "database": db,
  "migrations": json.load(open(f"{work}/migrations.json")),
  "theme_summary": json.load(open(f"{work}/theme-summary.json")),
  "themes": json.load(open(f"{work}/themes.json")),
  "config_sha256_nonsecret": config_sum,
  "attachments": {
    "count": attach_count,
    "storage_dir": storage,
    "objects": json.load(open(f"{work}/attachments.json")),
  },
  "s3": s3 if s3 else None,
}
print(json.dumps(out, indent=2, ensure_ascii=False))
PYEOF

echo "==> 完成: $OUT_FILE"
echo "    迁移记录: $(python3 -c "import json;print(len(json.load(open('$WORK/migrations.json'))))") 条"
echo "    附件对象: $ATTACH_COUNT"
rm -rf "$WORK"
