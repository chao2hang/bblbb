# RC 报告目录说明

本目录现有文件是 2026-08-08 的 rc.2 / M16-M17 历史验收快照，部分报告仍记录 193 个 operation、783 个叶子任务和迁移 0058 等当时状态。

它们保留用于审计和追溯，不代表当前工作区状态。当前事实来源是：

- 项目进度与下一任务：[`../../TODO.md`](../../TODO.md)
- OpenAPI 覆盖：[`../../todo/openapi-operation-coverage.json`](../../todo/openapi-operation-coverage.json)
- 当前契约校验：`ruby scripts/sync-operation-coverage.rb --check`
- 当前路线图校验：`ruby scripts/check-roadmap.rb`

下一次 RC 验收应在新日期/新 RC 目录生成完整报告，或先整体重生成本目录后再引用其中的数字。
