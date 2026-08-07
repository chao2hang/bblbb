// M04-MARKDOWN-08 / M08-FEEDS-05：JSON-LD 注入的 `</script` 转义。
//
// 安全属性：把 `</script` 转义为 `<\/script`——`\/` 是合法 JSON 转义
// （JSON.parse 后语义不变），同时任何字符串字段都无法以 `</script>` 提前
// 闭合 `<script type="application/ld+json">` 造成注入。
// 独立 .ts 模块：避免在 Svelte 模板的 `<script>` 块内出现 `</` 字节序列
// 触发编译器误判。
export function escapeJsonLdScript(raw: string): string {
  // 只匹配 `<` + `/` + `script`（大小写不敏感）；`<\\/$1` 在源码里是
  // `<\/script`（反斜杠 + / + 匹配组）。
  return raw.replace(/<\/(script)/gi, '<\\/$1');
}
