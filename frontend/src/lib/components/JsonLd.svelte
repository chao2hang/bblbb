<script lang="ts">
  // M04-MARKDOWN-08 白名单组件：仅输出 JSON-LD script 标签。
  //
  // 安全约束：
  // - `data` 必须是服务端构造、已 `JSON.stringify` 的对象（不可含用户自由文本
  //   未经转义直接拼接）；
  // - `escapeJsonLdScript` 把 script 闭合序列转义为 `<\/script`（合法 JSON
  //   转义，语义不变），防止任何字符串字段提前闭合标签造成注入；
  // - 标签名用字符串拼接构造，避免源码中出现 script 闭合字节序列干扰
  //   Svelte 模板解析（含注释）。
  import { escapeJsonLdScript } from './jsonLd';
  let { data }: { data: string } = $props();
  const markup = $derived.by(() => {
    // 标签名用字符串拼接构造，避免源码中出现 script 闭合字节序列。
    const open = '<scr' + 'ipt type="application/ld+json">';
    const close = '<' + '/scr' + 'ipt>';
    return open + escapeJsonLdScript(data) + close;
  });
</script>

{@html markup}
