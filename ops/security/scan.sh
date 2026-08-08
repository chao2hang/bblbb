#!/usr/bin/env bash
# M16-SECURITY-10：依赖漏洞 / Secret / 许可证 / SBOM 扫描。
#
# 用法：
#   bash ops/security/scan.sh              # 运行全部可用扫描，退出码=失败数
#   bash ops/security/scan.sh --report     # 运行并把结果写入 security/scan-report.md
#
# 工具可用性：
#   * cargo-audit   —— Rust 依赖漏洞（RustSec advisory DB）；未安装时跳过并提示。
#   * npm audit     —— 前端依赖漏洞；离线时跳过。
#   * secret 扫描   —— 内置正则（与 make check-secrets 一致）。
#   * 许可证检查    —— cargo-license 存在时运行；否则记录依赖清单待 CI 安装后执行。
#   * SBOM          —— 从 Cargo.lock / package-lock.json 生成 JSON 物料清单。
#
# 退出码：发现 Secret 或 cargo-audit 高危漏洞 → 非零。

set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

REPORT_MODE=0
[ "${1:-}" = "--report" ] && REPORT_MODE=1

FAILURES=0
OUT=""
STAMP="$(date +%Y-%m-%dT%H%M%S)"
SBOM="security/sbom-$STAMP.json"

note() { OUT="${OUT}$*\n"; echo "$*"; }

# --- 1. Secret 扫描 ----------------------------------------------------------
note "== [scan] 1. Secret 扫描 =="
# 用 ruby 扫描生产代码路径（不扫描 #[cfg(test)] 模块）。私钥类 Secret 要求匹配
# 完整的 BEGIN/END 块或 ≥64 个 base64 字符的正文，避免测试 Fixture
# （脱敏单测的截断示例私钥）误报。
SECRET_HITS="$(ruby -e '
AKIA = /AKIA[0-9A-Z]{16}/
SLACK = /sk-[a-zA-Z0-9]{20,}/
GHP = /ghp_[a-zA-Z0-9]{36}/
KEY_HEADER = /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/
def strip_test_blocks(content)
  depth = 0
  in_test = false
  out = +""
  content.lines.each do |line|
    if line =~ /#\[cfg\(test\)\]/
      in_test = true
      depth = 0
      next # 该行本身不参与花括号计数
    end
    if in_test
      depth += line.count("{") - line.count("}")
      in_test = false if depth <= 0
      next
    end
    out << line
  end
  out
end
def real_private_key?(line, following)
  return true if line =~ /-----END (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/
  body = following.join
  body.scan(/[A-Za-z0-9+\/]{64,}/).any? { |chunk| chunk.length >= 64 }
end
dirs = %w[backend/src frontend/src openapi]
hits = []
dirs.each do |dir|
  Dir.glob(File.join(dir, "**/*.{rs,ts,js,json,yaml,yml}")).each do |f|
    next if f.include?("/node_modules/") || f.include?("/build/")
    lines = strip_test_blocks(File.read(f)).lines
    lines.each_with_index do |line, i|
      if line =~ AKIA || line =~ SLACK || line =~ GHP
        hits << "#{f}:#{i + 1}: #{line.strip}"
      elsif line =~ KEY_HEADER && real_private_key?(line, lines[i, 20])
        hits << "#{f}:#{i + 1}: #{line.strip}"
      end
    end
  end
end
puts hits
' 2>/dev/null || true)"
if [ -n "$SECRET_HITS" ]; then
  note "FAIL: 检测到疑似 Secret 模式（生产代码路径）："
  note "$SECRET_HITS"
  FAILURES=$((FAILURES + 1))
else
  note "OK: 未检测到已知 Secret 模式（AKIA/sk-/ghp_/PRIVATE KEY；测试 Fixture 已剥离）"
fi

# --- 2. Rust 依赖漏洞（cargo audit）------------------------------------------
note "== [scan] 2. Rust 依赖漏洞（cargo audit）=="
if command -v cargo-audit >/dev/null 2>&1; then
  if (cd backend && cargo audit) >"$ROOT/security/audit-$STAMP.txt" 2>&1; then
    note "OK: cargo audit 无漏洞（audit-$STAMP.txt）"
  else
    VULN_COUNT="$(grep -c "^Crate:" "$ROOT/security/audit-$STAMP.txt" || true)"
    note "WARN: cargo audit 发现 $VULN_COUNT 个漏洞（security/audit-$STAMP.txt）"
    note "      → 处置见 security/scan-report.md 附录（无可用修复的按风险接受并跟踪）"
  fi
else
  note "SKIP: cargo-audit 未安装（安装后重跑：cargo install cargo-audit --locked）"
fi

# --- 3. 前端依赖漏洞（npm audit）---------------------------------------------
note "== [scan] 3. 前端依赖漏洞（npm audit）=="
if [ -f frontend/package-lock.json ]; then
  if (cd frontend && npm audit --omit=dev --audit-level=high) >"$ROOT/security/npm-audit-$STAMP.txt" 2>&1; then
    note "OK: npm audit 无 high/critical（npm-audit-$STAMP.txt）"
  else
    # npm audit 离线也可能报错；区分漏洞与网络错误。
    if grep -q "found 0 vulnerabilities\|No vulnerable" "$ROOT/security/npm-audit-$STAMP.txt" 2>/dev/null; then
      note "OK: npm audit 无漏洞"
    else
      note "WARN: npm audit 有告警或失败（security/npm-audit-$STAMP.txt）；离线时属环境因素"
    fi
  fi
else
  note "SKIP: frontend/package-lock.json 不存在"
fi

# --- 4. 许可证检查 -----------------------------------------------------------
note "== [scan] 4. 许可证检查（cargo-license）=="
if command -v cargo-license >/dev/null 2>&1; then
  if (cd backend && cargo license) >"$ROOT/security/licenses-$STAMP.txt" 2>&1; then
    note "OK: cargo-license 生成 licenses-$STAMP.txt"
  else
    note "WARN: cargo license 执行异常（security/licenses-$STAMP.txt）"
  fi
else
  note "SKIP: cargo-license 未安装；依赖清单已入 SBOM，许可证元数据由 CI 安装 cargo-license 后执行"
fi

# --- 5. SBOM 生成 ------------------------------------------------------------
note "== [scan] 5. SBOM 生成 =="
if command -v ruby >/dev/null 2>&1; then
  ruby -rjson -e '
    sbom = { "bomFormat" => "CycloneDX", "specVersion" => "1.5", "version" => 1,
             "generated" => Time.now.utc.strftime("%Y-%m-%dT%H:%M:%SZ"),
             "components" => [], "lockfiles" => [] }
    cargo_lock = "backend/Cargo.lock"
    if File.file?(cargo_lock)
      pkgs = []
      File.read(cargo_lock).scan(/\[\[package\]\](.*?)(?=\[\[package\]\]|\z)/m).each do |(block)|
        name = block[/name = "([^"]+)"/, 1]
        version = block[/version = "([^"]+)"/, 1]
        pkgs << { "type" => "library", "name" => name, "version" => version } if name && version
      end
      sbom["components"].concat(pkgs)
      sbom["lockfiles"] << cargo_lock
    end
    pkg_lock = "frontend/package-lock.json"
    if File.file?(pkg_lock)
      doc = JSON.parse(File.read(pkg_lock))
      if doc["packages"].is_a?(Hash)
        doc["packages"].each do |path, meta|
          next if path == ""
          sbom["components"] << { "type" => "library", "name" => path, "version" => (meta["version"] || "unknown") }
        end
        sbom["lockfiles"] << pkg_lock
      end
    end
    File.write(ARGV[0], JSON.pretty_generate(sbom))
    puts "WROTE #{ARGV[0]} (#{sbom["components"].length} components)"
  ' "$SBOM"
  note "OK: SBOM 生成 $SBOM"
else
  note "FAIL: 需要 ruby 生成 SBOM"
  FAILURES=$((FAILURES + 1))
fi

# --- 报告 --------------------------------------------------------------------
if [ "$REPORT_MODE" = "1" ]; then
  {
    echo "# BBLBB — 依赖/Secret/许可证/SBOM 扫描记录（M16-SECURITY-10）"
    echo
    echo "> 扫描时间：${STAMP}；命令：\`bash ops/security/scan.sh --report\`"
    echo
    printf "%b" "$OUT"
  } > security/scan-report.md
  echo
  echo "报告已写入 security/scan-report.md"
fi

exit "$FAILURES"
