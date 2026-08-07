<script lang="ts">
  // M08-UI-03/04：隐私与索引设置。
  // - 逐帖退出搜索/AI 摘要的开关在每帖编辑器（/editor）；本页展示设置位置、
  //   管理员策略优先级与 robots/索引状态说明；
  // - 文案明确 robots/meta 是声明层而非安全边界，不承诺能阻止恶意抓取。
  import Card from '$lib/components/ui/Card.svelte';
  import { page } from '$app/state';
  import type { PrivacyPageData } from './+page.server';

  let { data }: { data: PrivacyPageData } = $props();

  const user = $derived(data.user);
  const error = $derived(data.error);
  const canonical = $derived(`${page.url.origin}/settings/privacy`);
</script>

<svelte:head>
  <title>隐私与索引设置 — BBLBB</title>
  <meta name="description" content="管理你的内容在搜索引擎与 AI 摘要中的索引状态" />
  <link rel="canonical" href={canonical} />
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/settings" class="breadcrumb-link">账号设置</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">隐私与索引</span>
  </nav>

  <div class="settings-layout">
    <nav class="settings-nav" aria-label="设置导航">
      <a href="/settings" class="settings-nav-item">基本资料</a>
      <a href="/me" class="settings-nav-item">我的主页</a>
      <a href="/notifications" class="settings-nav-item">通知</a>
      <a href="/settings/privacy" class="settings-nav-item is-active">隐私与索引</a>
    </nav>

    <div class="settings-content">
      {#if error && !user}
        <p class="input-hint is-error" role="alert">{error}</p>
      {/if}

      {#if user}
        <Card>
          <div class="card-header"><span class="card-title">搜索引擎与 AI 摘要（逐帖退出）</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
            <p style="margin:0;">
              你可以<strong>逐帖</strong>选择是否允许该内容进入公开搜索引擎索引
              （<code>search_index_opt_out</code>）以及是否允许生成 AI 摘要
              （<code>ai_summary_opt_out</code>）。这两个开关在
              <a href="/editor" class="text-link">发布/编辑</a>每篇帖子时设置，
              随草稿与发布内容一并保存；修改后索引 Job 会异步重建该帖索引。
            </p>
            <p style="margin:0;">
              只有<strong>明确允许索引的公开内容</strong>才会出现在搜索结果、
              摘要、OpenGraph 或 JSON-LD 投影中；隐藏、审核中、删除、付费或
              回复可见的正文永远不会进入任何公开投影。
            </p>
          </div>
        </Card>

        <Card>
          <div class="card-header"><span class="card-title">管理员策略优先级</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
            <p style="margin:0;">
              管理员可以按<strong>全站或板块</strong>强制关闭搜索索引与 AI 摘要
              （M08-INDEX-03 管理员策略优先）。在此情况下，即使你未勾选退出，
              相关内容也不会进入索引或摘要生成；你的逐帖设置不会绕过管理员策略。
            </p>
            <p style="margin:0;">
              搜索结果返回前还会重新执行实时可见性、处罚与索引退出判断
              （M08-INDEX-07）——索引只是候选集，不是授权裁决。
            </p>
          </div>
        </Card>

        <Card>
          <div class="card-header"><span class="card-title">搜索引擎索引状态</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
            <ul style="margin:0;padding-left:var(--space-4);display:flex;flex-direction:column;gap:var(--space-2);">
              <li>搜索结果页默认输出 <code>noindex,follow,noarchive</code>（不会被收录），但带 canonical 与 OpenGraph 供分享预览。</li>
              <li>完全公开且未被排除的文章会输出 canonical、OpenGraph、Twitter Card 与结构化数据（JSON-LD）。</li>
              <li>robots.txt、页面 meta 与 <code>X-Robots-Tag</code> 按当前配置动态生成，并随管理员策略变更在配置传播窗口内更新。</li>
            </ul>
          </div>
        </Card>

        <Card>
          <div class="card-header"><span class="card-title">robots 与抓取边界</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
            <p style="margin:0;">
              robots.txt、页面 meta 与响应头是<strong>协作性声明</strong>，用于告知
              合规的搜索引擎与分享抓取器如何访问；它们<strong>不能阻止恶意或
              无视规则的抓取</strong>。真正的边界是服务端授权、内容可见性过滤、
              速率限制与行为检测。
            </p>
            <p style="margin:0;">
              AI 训练类爬虫（如 GPTBot、CCBot、Google-Extended、ClaudeBot）默认被拒绝；
              普通搜索引擎只被允许索引明确允许的公开内容。验证入口与限流状态不会
              解除任何内容授权边界。
            </p>
          </div>
        </Card>
      {:else if !error}
        <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
      {/if}
    </div>
  </div>
</div>
