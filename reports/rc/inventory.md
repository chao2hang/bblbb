# BBLBB — 依赖、License、SBOM、Secret 与构建物清点（M17-FREEZE-05）

> 执行：platform/release-manager；日期：2026-08-08。

## 1. 依赖与 License

- Rust 依赖锁定：`backend/Cargo.lock`（commit 内，可复现构建）。
- 前端依赖锁定：`frontend/package-lock.json`。
- 漏洞扫描：`security/audit-2026-08-08T000643.txt`（cargo audit）——
  4 项发现（rsa 0.9.10 无上游修复、rustls-webpki 上游 pin）已按风险接受并记录于
  `security/scan-report.md`；`security/npm-audit-2026-08-08T000643.txt` 前端审计。

## 2. SBOM

- `security/sbom-2026-08-08T000643.json`（cargo 依赖 SBOM，634 组件）。
- 前端 SBOM 由 `package-lock.json` 生成（npm sbom）。

## 3. Secret 扫描

- `make check-secrets` 通过（scripts/check-secrets.rb，test-fixture-aware）。
- 生产 URL/凭据扫描：`make check` 的 check-secrets 覆盖；无 AKIA/sk-/ghp_/PRIVATE KEY 模式。

## 4. 构建物 checksum 与版本标签

- Release bundle：`deploy/scripts/build-release.sh` 生成
  `release-metadata.json`（commit、版本、依赖锁、SBOM、各产物 SHA-256）。
- 版本标签：git tag v1.0.0-rc.2（冻结基线）+ 发布 commit（待 M17-LAUNCH 打生产 tag）。
