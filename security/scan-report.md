# BBLBB — 依赖/Secret/许可证/SBOM 扫描记录（M16-SECURITY-10）

> 扫描时间：2026-08-08T000643；命令：`bash ops/security/scan.sh --report`

== [scan] 1. Secret 扫描 ==
OK: 未检测到已知 Secret 模式（AKIA/sk-/ghp_/PRIVATE KEY；测试 Fixture 已剥离）
== [scan] 2. Rust 依赖漏洞（cargo audit）==
WARN: cargo audit 发现 4 个漏洞（security/audit-2026-08-08T000643.txt）
      → 处置见 security/scan-report.md 附录（无可用修复的按风险接受并跟踪）
== [scan] 3. 前端依赖漏洞（npm audit）==
OK: npm audit 无 high/critical（npm-audit-2026-08-08T000643.txt）
== [scan] 4. 许可证检查（cargo-license）==
SKIP: cargo-license 未安装；依赖清单已入 SBOM，许可证元数据由 CI 安装 cargo-license 后执行
== [scan] 5. SBOM 生成 ==
OK: SBOM 生成 security/sbom-2026-08-08T000643.json

## 附录 A：cargo audit 漏洞处置记录（误报/无修复项跟踪）

| ID | Crate/Version | 严重度 | 影响面评估 | 处置 | 负责人/复查 |
|---|---|---|---|---|---|
| RUSTSEC-2023-0071 | rsa 0.9.10（Marvin 时序侧信道） | medium 5.9 | BBLBB 仅用 rsa 做 OIDC RS256 **签名/验签**，不使用 RSA 解密（Marvin 攻击面是 RSA-CRT 解密时序）；`No fixed upgrade available`（0.9 线无修复版，需升级 0.10 大版本） | 风险接受并跟踪；OIDC 专项 Gate 升级 rsa≥0.10 | platform/application-security · 2026-09-07 |
| RUSTSEC-2026-0104 | rustls-webpki 0.101.7（CRL 解析可达 panic） | medium | aws-sdk（S3）经 rustls 0.21 传递引入；BBLBB 出站不配置 CRL 校验，未触达该路径 | 上游固定（aws-sdk 需升级 rustls）；跟踪 aws-sdk 发版后 `cargo update` | platform/application-security · 2026-09-07 |
| RUSTSEC-2026-0098 | rustls-webpki 0.101.7（URI name constraints） | medium | aws-sdk 出站 TLS 证书校验；BBLBB 仅访问固定白名单 Host（S3/MinIO 端点），证书链由系统信任根校验 | 上游固定；跟踪升级 | platform/application-security · 2026-09-07 |
| RUSTSEC-2026-0099 | rustls-webpki 0.101.7（wildcard name constraints） | medium | 同上 | 上游固定；跟踪升级 | platform/application-security · 2026-09-07 |

处置规则：所有"上游固定/无可用修复"项由 CI `cargo audit` 持续监控（nightly 层），
出现修复版本后必须升级并更新本表；升级属于依赖变更，需跑全量验证。

## 附录 B：误报记录

| 模式 | 误报场景 | 处置 |
|---|---|---|
| `-----BEGIN ... PRIVATE KEY-----` 短文本 | `backend/src/observability/mod.rs` 脱敏单测的截断示例私钥（`MIIEowIBAA`，10 字节） | 扫描剥离 `#[cfg(test)]` 块 + 私钥正文 ≥64 base64 字符才告警；例如此处不再告警 |

## 附录 C：SBOM

`security/sbom-2026-08-08T000643.json`：634 个组件（Rust Cargo.lock + 前端 package-lock.json），
CycloneDX 1.5 结构。许可证元数据（cargo-license）与真实 SBOM 附件在 CI 层生成后随发布保留。
