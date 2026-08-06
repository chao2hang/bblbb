<script lang="ts">
  /**
   * M04-MARKDOWN-08：全应用**唯一**允许使用 `{@html}` 的组件。
   *
   * - 只接收**后端已清洗**的 HTML 渲染产物（post_contents.body_html /
   *   post_revisions.body_html，M04-MARKDOWN-02/03：CommonMark 渲染 → 原始
   *   HTML 剥离 → ammonia allowlist 清洗），前端不做任何 HTML 解析或再清洗；
   * - 前端不做客户端重清洗：会引入新的 sink（DOMParser 等），并可能破坏
   *   后端生成的确定性锚点 id；
   * - 静态检查 `scripts/check-html-sinks.rb` 保证本文件是唯一 `{@html}`
   *   使用点，其余任何生产源文件出现 `{@html}` 或 DOM HTML sink 即失败。
   */
  let { html }: { html: string } = $props();
</script>

{@html html}
