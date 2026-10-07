<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import MeSectionTabs from '$lib/components/MeSectionTabs.svelte';
  import MeIdentityBar from '$lib/components/MeIdentityBar.svelte';
  import { LINUXDO_TRUST_LEVELS, type TrustLevelProgress, type TrustLevelMeta } from '$lib/api/types';
  import type { LevelPageData } from './+page.server';
  import { getCurrencyNameContext } from '$lib/site/currency-context.svelte';

  let { data }: { data: LevelPageData } = $props();

  const trust = $derived(data.trust);
  const error = $derived(data.error);
  const barUser = $derived(data.user);
  const currencyName = $derived(getCurrencyNameContext()?.currencyName ?? '金币');

  // 当前信任等级与下一级元数据
  const currentLevel = $derived(trust?.level ?? 0);
  const currentMeta: TrustLevelMeta = $derived(
    LINUXDO_TRUST_LEVELS.find((m) => m.level === currentLevel) ?? LINUXDO_TRUST_LEVELS[0]
  );
  const isMaxLevel = $derived(currentLevel >= 4);

  // Tab 状态：默认展示「我的等级与进度」，可切换「全站阶梯表」与「规则说明」
  let currentTab = $state<'overview' | 'tiers' | 'rules'>('overview');

  // 阶梯表选中预览项（默认为用户当前等级）
  let selectedTierLevel = $state<number>(0);
  $effect(() => {
    selectedTierLevel = currentLevel;
  });

  const selectedTierMeta = $derived(
    LINUXDO_TRUST_LEVELS.find((m) => m.level === selectedTierLevel) ?? LINUXDO_TRUST_LEVELS[0]
  );

  // 阶梯表各层状态判定
  const trustTiers = $derived(
    LINUXDO_TRUST_LEVELS.map((tier) => {
      let status: 'completed' | 'current' | 'locked' = 'locked';
      if (tier.level < currentLevel) {
        status = 'completed';
      } else if (tier.level === currentLevel) {
        status = 'current';
      }
      return {
        ...tier,
        status
      };
    })
  );

  // 计算达标指标数量
  const requirementsMetCount = $derived.by(() => {
    if (!trust?.next_level?.requirements) return 0;
    return trust.next_level.requirements.filter((r) => r.met).length;
  });

  const requirementsTotalCount = $derived(trust?.next_level?.requirements?.length ?? 0);
  const requirementsPercentage = $derived(
    requirementsTotalCount > 0 ? Math.round((requirementsMetCount / requirementsTotalCount) * 100) : 0
  );

  // 解析阶梯表各级晋升条件为结构化列表项
  function parsePromotionItems(promotion: string): { title?: string; items: string[] } {
    if (promotion.includes('：')) {
      const parts = promotion.split('：');
      const title = parts[0];
      const rest = parts.slice(1).join('：');
      const items = rest ? rest.split('、').map((s) => s.trim()).filter(Boolean) : [];
      return { title, items: items.length > 0 ? items : [promotion] };
    }
    const items = promotion.split('、').map((s) => s.trim()).filter(Boolean);
    return { items: items.length > 0 ? items : [promotion] };
  }

  // 移动端或点击切换时的展开浮窗（等级数字或 null）
  let activePopoverTier = $state<number | null>(null);
</script>

<svelte:window
  onclick={(e) => {
    if (activePopoverTier !== null && !(e.target as HTMLElement)?.closest?.('.cond-popover-wrap')) {
      activePopoverTier = null;
    }
  }}
  onkeydown={(e) => {
    if (e.key === 'Escape') activePopoverTier = null;
  }}
/>

<PageTitle title="社区信任等级" />

<div class="container page-content" id="page-level">
  <!-- 页面头部导航与标题区 -->
  <div class="level-page-header">
    <div class="header-breadcrumbs">
      <a href="/me" class="breadcrumb-link">
        <Icon name="chevron-left" size={16} />
        <span>个人中心</span>
      </a>
      <span class="breadcrumb-sep">/</span>
      <span class="breadcrumb-current">信任等级</span>
    </div>
    <div class="header-main">
      <div>
        <div style="display: flex; align-items: center; gap: var(--space-2); margin-bottom: 4px;">
          <h1 class="page-heading">社区信任等级</h1>
          <span class="badge badge-success" style="font-size: var(--text-xs);">社区行为标准</span>
        </div>
        <p class="page-subheading">基于阅读深度、长期活跃与行为可信度评估，构建健康友善的社区自治氛围</p>
      </div>
      <div class="header-actions">
        <Button text="积分明细与签到" variant="secondary" size="sm" icon="coins" href="/me/balance" />
        <Button text="去商城兑换" variant="secondary" size="sm" icon="shopping-bag" href="/shop" />
      </div>
    </div>
  </div>

  {#if barUser}
    <!-- 固定身份条（≤767px 吸顶）：与 /me、/me/security 共用，身份恒定可见 -->
    <MeIdentityBar
      name={barUser.display_name || barUser.username}
      username={barUser.username}
      level={trust?.level ?? barUser.level ?? 0}
      avatarAttachmentId={barUser.avatar_attachment_id}
      seed={barUser.username ?? barUser.id}
    />
  {/if}

  <!-- 分区 tag 栏（tag = 独立路由页面）：本页为「信任等级」tag -->
  <MeSectionTabs />

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  <!-- 顶部当前等级卡 (TL0–TL4) -->
  <div class="current-level-hero">
    <div class="hero-left">
      <div class="badge-ring-container">
        <div class="badge-ring">
          <span class="badge-tl-code">{currentMeta.code}</span>
        </div>
      </div>
      <div class="hero-meta">
        <div class="hero-title-row">
          <h2 class="hero-title">{currentMeta.code} · {currentMeta.name}</h2>
          <span class="badge badge-success">当前生效</span>
          {#if isMaxLevel}
            <span class="badge badge-warning">巅峰满级</span>
          {/if}
        </div>
        <p class="hero-desc">{trust?.summary || currentMeta.summary}</p>
      </div>
    </div>

    <!-- 右侧简报/快捷指标 -->
    <div class="hero-right-stat">
      {#if trust?.grace_until}
        <div class="grace-badge-pill">
          <Icon name="alert-triangle" size={14} />
          <span>考核宽限至 {new Date(trust.grace_until).toLocaleDateString()}</span>
        </div>
      {/if}
      {#if !isMaxLevel && trust?.next_level}
        <div class="progress-preview-box">
          <div class="progress-preview-header">
            <span>升至 TL{trust.next_level.level}（{trust.next_level.name}）</span>
            <strong>{requirementsMetCount} / {requirementsTotalCount} 项达标</strong>
          </div>
          <div class="progress-preview-track">
            <div
              class="progress-preview-fill"
              style="width: {requirementsPercentage}%"
            ></div>
          </div>
        </div>
      {:else if isMaxLevel}
        <div class="max-level-box">
          <Icon name="award" size={16} />
          <span>已享有全站最高信任特权</span>
        </div>
      {/if}
    </div>
  </div>

  <!-- 分页导航标签 (Tabs)：清晰划分「我的晋升要求」、「全站阶梯表」与「规则说明」 -->
  <div class="level-nav-tabs" role="tablist" aria-label="信任等级内容分页">
    <button
      type="button"
      id="level-tab-overview"
      role="tab"
      aria-selected={currentTab === 'overview'}
      aria-controls="level-panel-overview"
      class="tab-btn {currentTab === 'overview' ? 'is-active' : ''}"
      onclick={() => (currentTab = 'overview')}
    >
      <Icon name="trending-up" size={16} />
      <span>我的进度与下一级要求</span>
      {#if !isMaxLevel && trust?.next_level?.eligible}
        <span class="tab-badge-dot" title="已满足晋升条件"></span>
      {/if}
    </button>

    <button
      type="button"
      id="level-tab-tiers"
      role="tab"
      aria-selected={currentTab === 'tiers'}
      aria-controls="level-panel-tiers"
      class="tab-btn {currentTab === 'tiers' ? 'is-active' : ''}"
      onclick={() => (currentTab = 'tiers')}
    >
      <Icon name="list" size={16} />
      <span>全站信任等级阶梯表</span>
    </button>

    <button
      type="button"
      id="level-tab-rules"
      role="tab"
      aria-selected={currentTab === 'rules'}
      aria-controls="level-panel-rules"
      class="tab-btn {currentTab === 'rules' ? 'is-active' : ''}"
      onclick={() => (currentTab = 'rules')}
    >
      <Icon name="help-circle" size={16} />
      <span>信任体系规则解读</span>
    </button>
  </div>

  <!-- TAB 1: 我的进度与下一级晋升要求 -->
  {#if currentTab === 'overview'}
    <div class="tab-pane" id="level-panel-overview" role="tabpanel" aria-labelledby="level-tab-overview">
      {#if !isMaxLevel}
        <div class="card next-level-card">
          <div class="card-header">
            <div style="display: flex; align-items: center; gap: var(--space-2);">
              <Icon name="sparkles" size={18} />
              <span class="card-title">
                升至 {trust?.next_level ? `TL${trust.next_level.level}（${trust.next_level.name}）` : `TL${currentLevel + 1}`} 晋升要求
              </span>
            </div>
            {#if trust?.next_level?.eligible}
              <span class="badge badge-success">全部达标，系统评估晋升中</span>
            {:else}
              <span class="badge badge-neutral">达成进度 {requirementsPercentage}%</span>
            {/if}
          </div>
          <div class="card-body">
            {#if trust?.next_level?.manual_only}
              <div class="manual-notice-box">
                <Icon name="shield" size={24} />
                <div>
                  <strong>人工审核授予等级</strong>
                  <p>TL4 领导者属于社区核心治理等级，仅可由工作人员在满足长期卓越贡献后人工审核手动授予。</p>
                </div>
              </div>
            {:else if trust?.next_level && trust.next_level.requirements.length > 0}
              <p class="next-level-intro">
                完成以下全部指标要求，系统将在您下一次访问、阅读或互动时自动为您晋升：
              </p>
              <div class="requirements-grid">
                {#each trust.next_level.requirements as req (req.key)}
                  <div class="req-card {req.met ? 'is-met' : 'is-unmet'}">
                    <div class="req-card-top">
                      <div class="req-label-group">
                        <div class="req-status-icon">
                          <Icon name={req.met ? 'check' : 'clock'} size={16} />
                        </div>
                        <span class="req-name">{req.label}</span>
                      </div>
                      <span class="req-badge {req.met ? 'badge-met' : 'badge-unmet'}">
                        {req.met ? '已达标' : '未达标'}
                      </span>
                    </div>
                    <div class="req-progress-row">
                      <span class="req-current">{req.current}</span>
                      <span class="req-divider">/</span>
                      <span class="req-target">{req.required}</span>
                    </div>
                    <div class="req-bar-track">
                      <div
                        class="req-bar-fill {req.met ? 'is-complete' : ''}"
                        style="width: {Math.min(100, Math.round((Number(req.current) / Math.max(1, Number(req.required))) * 100))}%"
                      ></div>
                    </div>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="fallback-req-box">
                <p>持续保持真诚阅读、参与主题回复与日常访问，系统将实时评估您的信任等级指标。</p>
              </div>
            {/if}
          </div>
        </div>
      {:else}
        <div class="card" style="padding: var(--space-5); text-align: center;">
          <div class="max-congrats-wrap">
            <div class="max-trophy-icon">
              <Icon name="award" size={40} />
            </div>
            <h3 style="margin: var(--space-2) 0 4px; font-size: var(--text-lg);">已达成社区最高信任层级</h3>
            <p class="text-secondary" style="margin: 0; font-size: var(--text-sm); max-width: 500px; margin: 0 auto;">
              感谢您为社区建设与良好讨论氛围做出的突出贡献。您享有全站全部社区自治、话题整理与附件特权。
            </p>
          </div>
        </div>
      {/if}

      <!-- 当前等级专属特权 -->
      <div class="card perks-card" style="margin-top: var(--space-4);">
        <div class="card-header">
          <span class="card-title">{currentMeta.code} · {currentMeta.name} 享有的核心特权</span>
          <span class="badge badge-success">生效中</span>
        </div>
        <div class="card-body">
          <div class="perks-pills-grid">
            {#each currentMeta.perks as perk}
              <div class="perk-pill-item">
                <Icon name="check-circle" size={16} />
                <span>{perk}</span>
              </div>
            {/each}
          </div>
        </div>
      </div>
    </div>
  {/if}

  <!-- TAB 2: 全站信任等级阶梯表 (TL0–TL4) -->
  {#if currentTab === 'tiers'}
    <div class="tab-pane" id="level-panel-tiers" role="tabpanel" aria-labelledby="level-tab-tiers">
      <div class="card roadmap-card">
        <div class="card-header">
          <div>
            <div style="display: flex; align-items: center; gap: var(--space-2);">
              <span class="card-title">全站信任等级阶梯表</span>
              <span class="badge badge-neutral" style="font-size: var(--text-xs);">社区信任标准</span>
            </div>
            <p class="text-secondary" style="margin: 4px 0 0; font-size: var(--text-xs);">
              基于阅读深度、长期活跃度与行为可信度持续评估，全站唯一等级标准
            </p>
          </div>
        </div>
        <div class="card-body" style="padding: 0;">
          <div class="table-container">
            <table class="roadmap-table">
              <thead>
                <tr>
                  <th style="width: 72px;">等级</th>
                  <th style="width: 96px;">称谓</th>
                  <th style="width: 120px;">晋升条件</th>
                  <th>等级特权摘要</th>
                  <th style="width: 112px; text-align: right;">达成状态</th>
                </tr>
              </thead>
              <tbody>
                {#each trustTiers as tier (tier.level)}
                  {@const parsed = parsePromotionItems(tier.promotion)}
                  <tr class="tier-row is-{tier.status}">
                    <td>
                      <div class="tier-badge-cell">
                        <span class="badge {tier.status === 'current' ? 'badge-level' : 'badge-neutral'}">
                          {tier.code}
                        </span>
                      </div>
                    </td>
                    <td>
                      <div class="tier-name-cell">
                        <strong>{tier.name}</strong>
                      </div>
                    </td>
                    <td>
                      <div class="tier-condition-cell" data-label="晋升条件">
                        <div
                          class="cond-popover-wrap {activePopoverTier === tier.level ? 'is-active' : ''}"
                        >
                          <button
                            type="button"
                            class="cond-trigger"
                            onclick={(e) => {
                              e.stopPropagation();
                              activePopoverTier = activePopoverTier === tier.level ? null : tier.level;
                            }}
                            aria-expanded={activePopoverTier === tier.level}
                            aria-label="{tier.code} {tier.name} 晋升条件"
                          >
                            <Icon name="list" size={13} />
                            <span>查看条件</span>
                            <Icon name="chevron-down" size={11} class="cond-trigger-arrow" />
                          </button>

                          <div
                            class="cond-popover {tier.level >= 2 ? 'popover-up' : 'popover-down'}"
                            role="tooltip"
                          >
                            <div class="cond-popover-head">
                              <div class="cond-popover-title-row">
                                <span class="badge {tier.status === 'current' ? 'badge-level' : 'badge-neutral'}">{tier.code}</span>
                                <strong class="cond-popover-title">{tier.name} · 晋升要求</strong>
                              </div>
                              {#if tier.level === 0}
                                <span class="badge badge-neutral cond-head-tag">默认达到</span>
                              {:else if tier.level === 3}
                                <span class="badge badge-warning cond-head-tag">100天滚动</span>
                              {:else if tier.level === 4}
                                <span class="badge badge-warning cond-head-tag">人工审核</span>
                              {:else}
                                <span class="badge badge-neutral cond-head-tag">共 {parsed.items.length} 项</span>
                              {/if}
                            </div>

                            {#if parsed.title}
                              <div class="cond-popover-note">
                                <Icon name="refresh-cw" size={12} />
                                <span>{parsed.title}考核标准</span>
                              </div>
                            {/if}

                            <ul class="cond-popover-list">
                              {#each parsed.items as item}
                                <li class="cond-popover-item">
                                  <span class="cond-item-bullet">
                                    {#if tier.level === 0}
                                      <Icon name="check" size={12} />
                                    {:else if tier.level === 4}
                                      <Icon name="shield" size={12} />
                                    {:else if tier.level === 3}
                                      <Icon name="refresh-cw" size={12} />
                                    {:else}
                                      <Icon name="check" size={12} />
                                    {/if}
                                  </span>
                                  <span class="cond-item-text">{item}</span>
                                </li>
                              {/each}
                            </ul>
                          </div>
                        </div>
                      </div>
                    </td>
                    <td>
                      <div class="tier-perks-cell" data-label="等级特权">
                        {#each tier.perks as perk}
                          <span class="tier-perk-badge">{perk}</span>
                        {/each}
                      </div>
                    </td>
                    <td style="text-align: right;">
                      {#if tier.status === 'completed'}
                        <span class="status-tag is-completed">
                          <Icon name="check" size={14} />
                          <span>已达成</span>
                        </span>
                      {:else if tier.status === 'current'}
                        <span class="status-tag is-current">
                          <span class="pulse-dot"></span>
                          <span>当前等级</span>
                        </span>
                      {:else if tier.level === 4}
                        <span class="status-tag is-locked">
                          <Icon name="shield" size={12} />
                          <span>人工审核</span>
                        </span>
                      {:else}
                        <span class="status-tag is-locked">
                          <Icon name="lock" size={12} />
                          <span>待达成</span>
                        </span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  {/if}

  <!-- TAB 3: 信任体系规则解读 FAQ -->
  {#if currentTab === 'rules'}
    <div class="tab-pane" id="level-panel-rules" role="tabpanel" aria-labelledby="level-tab-rules">
      <div class="faq-grid">
        <div class="card faq-card">
          <div class="card-body">
            <div class="faq-icon-wrap">
              <Icon name="shield-check" size={20} />
            </div>
            <h3 class="faq-title">为什么等级只有 5 级？</h3>
            <p class="faq-text">
              社区全面采用五级信任标准制度（TL0 至 TL4）。
              等级用于衡量社区成员的行为可信度与阅读深度，不再划分复杂的刷分数值，告别水贴刷级，鼓励真诚沉淀。
            </p>
          </div>
        </div>

        <div class="card faq-card">
          <div class="card-body">
            <div class="faq-icon-wrap">
              <Icon name="book-open" size={20} />
            </div>
            <h3 class="faq-title">如何提升信任等级？</h3>
            <p class="faq-text">
              坚持真诚阅读感兴趣的话题、浏览楼层、访问社区、给优秀作者点赞并友好回复。
              系统会根据您的真实阅读时长与行为自动评估，满足阈值后即刻晋升，无需额外申请。
            </p>
          </div>
        </div>

        <div class="card faq-card">
          <div class="card-body">
            <div class="faq-icon-wrap">
              <Icon name="refresh-cw" size={20} />
            </div>
            <h3 class="faq-title">TL3 滚动考核与降级规则</h3>
            <p class="faq-text">
              TL3 是核心活跃用户等级，基于近 100 天滚动窗口数据进行复核。
              若长期不活跃可能自动降回 TL2（享有 2 周宽限保护）；重新达标后系统将再次自动晋升。
            </p>
          </div>
        </div>

        <div class="card faq-card">
          <div class="card-body">
            <div class="faq-icon-wrap">
              <Icon name="coins" size={20} />
            </div>
            <h3 class="faq-title">积分与签到的作用</h3>
            <p class="faq-text">
              日常签到与活跃所获得的 {currencyName} 与积分作为社区经济资产独立存在，可用于商城道具兑换与装扮购买，
              不与信任等级挂钩，真正做到经济系统与行为信任体系清晰解耦。
            </p>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .level-page-header {
    margin-bottom: var(--space-4);
  }

  .header-breadcrumbs {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    margin-bottom: var(--space-2);
  }

  .breadcrumb-link {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--color-text-secondary);
    text-decoration: none;
    transition: color 0.15s ease;
  }

  .breadcrumb-link:hover {
    color: var(--color-text);
  }

  .breadcrumb-sep {
    opacity: 0.5;
  }

  .breadcrumb-current {
    color: var(--color-text);
    font-weight: 500;
  }

  .header-main {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .page-heading {
    margin: 0;
    font-size: var(--text-2xl, 24px);
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .page-subheading {
    margin: 4px 0 0;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .header-actions {
    display: flex;
    gap: var(--space-2);
  }

  /* ── 顶部精巧横幅 ── */
  .current-level-hero {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg, 8px);
    margin-bottom: var(--space-4);
  }

  .hero-left {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
    min-width: 240px;
  }

  .badge-ring-container {
    flex-shrink: 0;
  }

  .badge-ring {
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: var(--color-brand-soft);
    border: 2px solid color-mix(in srgb, var(--color-brand) 40%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .badge-tl-code {
    font-size: var(--text-base, 16px);
    font-weight: 800;
    color: var(--color-brand);
    letter-spacing: -0.02em;
  }

  .hero-meta {
    flex: 1;
    min-width: 200px;
  }

  .hero-title-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .hero-title {
    margin: 0;
    font-size: var(--text-lg, 18px);
    font-weight: 700;
    line-height: 1.3;
  }

  .hero-desc {
    margin: var(--space-1) 0 0;
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  .hero-right-stat {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--space-2);
    min-width: 220px;
  }

  .grace-badge-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border-radius: 9999px;
    background: var(--color-warning-soft);
    border: 1px solid color-mix(in srgb, var(--color-warning) 30%, transparent);
    color: var(--color-warning);
    font-size: var(--text-xs);
  }

  .progress-preview-box {
    width: 100%;
    max-width: 260px;
  }

  .progress-preview-header {
    display: flex;
    justify-content: space-between;
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    margin-bottom: 6px;
  }

  .progress-preview-header strong {
    color: var(--color-text);
  }

  .progress-preview-track {
    height: 6px;
    background: var(--color-bg-subtle);
    border-radius: 3px;
    overflow: hidden;
  }

  .progress-preview-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--color-brand), var(--color-success));
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  .max-level-box {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--color-success);
    font-size: var(--text-xs);
    font-weight: 500;
  }

  /* ── Tab 选项卡导航 ── */
  .level-nav-tabs {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    border-bottom: 1px solid var(--color-border);
    margin-bottom: var(--space-4);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab-btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color 0.15s ease, border-color 0.15s ease;
    white-space: nowrap;
  }

  .tab-btn:hover {
    color: var(--color-text);
  }

  .tab-btn.is-active {
    color: var(--color-brand);
    border-bottom-color: var(--color-brand);
  }

  .tab-badge-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-success);
  }

  .tab-pane {
    animation: fadeIn 0.2s ease-in-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  /* ── 晋升进度卡 ── */
  .next-level-intro {
    margin: 0 0 var(--space-3);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .requirements-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: var(--space-3);
  }

  .req-card {
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .req-card.is-met {
    border-color: color-mix(in srgb, var(--color-success) 35%, var(--color-border));
    background: var(--color-success-soft);
  }

  .req-card-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .req-label-group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .req-status-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary);
  }

  .req-card.is-met .req-status-icon {
    color: var(--color-success);
  }

  .req-name {
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .req-badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 9999px;
    font-weight: 500;
  }

  .badge-met {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .badge-unmet {
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
  }

  .req-progress-row {
    display: flex;
    align-items: baseline;
    gap: 4px;
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .req-current {
    font-weight: 700;
    color: var(--color-text);
  }

  .req-card.is-met .req-current {
    color: var(--color-success);
  }

  .req-divider {
    color: var(--color-text-secondary);
    opacity: 0.6;
  }

  .req-target {
    color: var(--color-text-secondary);
  }

  .req-bar-track {
    height: 6px;
    border-radius: 3px;
    background: var(--color-bg-subtle);
    overflow: hidden;
  }

  .req-bar-fill {
    height: 100%;
    border-radius: 3px;
    background: var(--color-brand);
    transition: width 0.3s ease;
  }

  .req-bar-fill.is-complete {
    background: var(--color-success);
  }

  .manual-notice-box {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-brand-soft);
    border: 1px solid color-mix(in srgb, var(--color-brand) 25%, transparent);
    color: var(--color-brand);
  }

  .manual-notice-box p {
    margin: 4px 0 0;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .fallback-req-box {
    padding: var(--space-4);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .max-congrats-wrap {
    padding: var(--space-4) 0;
  }

  .max-trophy-icon {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    background: var(--color-success-soft);
    color: var(--color-success);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--space-2);
  }

  /* ── 特权列表卡 ── */
  .perks-pills-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: var(--space-2);
  }

  .perk-pill-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
    font-size: var(--text-sm);
    color: var(--color-text);
  }

  .perk-pill-item :global(svg) {
    color: var(--color-success);
    flex-shrink: 0;
  }

  /* ── 阶梯一览表 ── */
  .roadmap-card {
    overflow: visible !important;
  }

  /* card-body 全局层有 padding !important，会吃掉内联 padding:0；
     这里以同优先级 !important 恢复「表格贴卡边」的设计意图，并放开 overflow 避免浮窗被父卡片裁切。 */
  .roadmap-card .card-body {
    padding: 0 !important;
    overflow: visible !important;
  }

  .table-container {
    width: 100%;
    overflow: visible !important;
  }

  .roadmap-table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    font-size: var(--text-sm);
    text-align: left;
  }

  /* 平板及以下（>767px）给表格保底宽度：宁可横向滚动也不压缩列
     （窄屏 ≤767px 由 mobile.css 改排为逐级卡片，不经过这里）。 */
  @media (min-width: 768px) {
    .roadmap-table {
      min-width: 700px;
    }
  }

  .roadmap-table th {
    padding: var(--space-3) var(--space-4);
    background: var(--color-bg-subtle);
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }

  .roadmap-table td {
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
    vertical-align: middle;
  }

  .tier-row {
    transition: background 0.15s ease;
  }

  .tier-row:hover {
    background: var(--color-surface-hover);
  }

  .tier-row.is-current {
    background: var(--color-brand-soft);
  }

  .tier-badge-cell {
    display: flex;
    align-items: center;
  }

  .tier-name-cell strong {
    display: block;
    white-space: nowrap;
  }

  .tier-condition-cell {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  /* ── 阶梯表晋升条件浮窗 ── */
  .cond-popover-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .tier-row:has(.cond-popover-wrap:hover),
  .tier-row:has(.cond-popover-wrap:focus-within),
  .tier-row:has(.cond-popover-wrap.is-active) {
    position: relative;
    z-index: 50;
  }

  .cond-popover-wrap:hover,
  .cond-popover-wrap:focus-within,
  .cond-popover-wrap.is-active {
    z-index: 60;
  }

  .cond-trigger {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    font-size: 11px;
    font-weight: 500;
    color: var(--color-text-secondary);
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full, 9999px);
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    user-select: none;
    line-height: 1.4;
  }

  .cond-trigger:hover,
  .cond-popover-wrap:hover .cond-trigger,
  .cond-popover-wrap:focus-within .cond-trigger,
  .cond-popover-wrap.is-active .cond-trigger {
    color: var(--color-brand);
    background: var(--color-brand-soft);
    border-color: color-mix(in srgb, var(--color-brand) 35%, transparent);
  }

  :global(.cond-trigger-arrow) {
    transition: transform 0.15s ease;
    opacity: 0.6;
  }

  .cond-popover-wrap:hover :global(.cond-trigger-arrow),
  .cond-popover-wrap:focus-within :global(.cond-trigger-arrow),
  .cond-popover-wrap.is-active :global(.cond-trigger-arrow) {
    transform: rotate(180deg);
    opacity: 1;
  }

  .cond-popover {
    position: absolute;
    left: 0;
    z-index: 70;
    width: max-content;
    min-width: 250px;
    max-width: 360px;
    padding: var(--space-3) var(--space-4);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg, 10px);
    box-shadow: var(--shadow-modal);
    backdrop-filter: blur(16px);
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition: opacity 0.15s ease, transform 0.15s ease, visibility 0.15s ease;
  }

  .cond-popover.popover-down {
    top: calc(100% + 6px);
    bottom: auto;
    transform: translateY(-4px);
  }

  .cond-popover.popover-down::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: -8px;
    height: 8px;
  }

  .cond-popover.popover-up {
    bottom: calc(100% + 6px);
    top: auto;
    transform: translateY(4px);
  }

  .cond-popover.popover-up::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: -8px;
    height: 8px;
  }

  .cond-popover-wrap:hover .cond-popover.popover-down,
  .cond-popover-wrap:focus-within .cond-popover.popover-down,
  .cond-popover-wrap.is-active .cond-popover.popover-down {
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
    transform: translateY(0);
  }

  .cond-popover-wrap:hover .cond-popover.popover-up,
  .cond-popover-wrap:focus-within .cond-popover.popover-up,
  .cond-popover-wrap.is-active .cond-popover.popover-up {
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
    transform: translateY(0);
  }

  .cond-popover-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding-bottom: var(--space-2);
    margin-bottom: var(--space-2);
    border-bottom: 1px solid var(--color-border);
  }

  .cond-popover-title-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .cond-popover-title {
    font-size: var(--text-xs, 12px);
    font-weight: 700;
    color: var(--color-text);
    white-space: nowrap;
  }

  .cond-head-tag {
    font-size: 10px;
    padding: 1px 6px;
    font-weight: 500;
  }

  .cond-popover-note {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--color-warning);
    background: var(--color-warning-soft);
    border: 1px solid color-mix(in srgb, var(--color-warning) 25%, transparent);
    border-radius: var(--radius-sm, 4px);
    padding: 3px 8px;
    margin-bottom: var(--space-2);
  }

  .cond-popover-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cond-popover-item {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: var(--text-xs, 12px);
    color: var(--color-text);
    line-height: 1.4;
  }

  .cond-item-bullet {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--color-brand-soft);
    color: var(--color-brand);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-top: 1px;
  }

  .cond-item-text {
    flex: 1;
    word-break: break-word;
  }

  .tier-perks-cell {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tier-perk-badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
    font-size: 11px;
    color: var(--color-text-secondary);
    white-space: nowrap;
  }

  .tier-row.is-current .tier-perk-badge {
    background: var(--color-brand-soft);
    color: var(--color-brand);
  }

  .status-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-xs);
    font-weight: 500;
    white-space: nowrap;
  }

  .status-tag.is-completed {
    color: var(--color-success);
  }

  .status-tag.is-current {
    color: var(--color-brand);
  }

  .pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-brand);
    animation: pulse 1.5s infinite;
  }

  @keyframes pulse {
    0%, 100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.4;
      transform: scale(1.4);
    }
  }

  .status-tag.is-locked {
    color: var(--color-text-secondary);
    opacity: 0.6;
  }

  /* ── FAQ 卡片组 ── */
  .faq-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: var(--space-3);
  }

  .faq-card {
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
  }

  .faq-icon-wrap {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    background: var(--color-brand-soft);
    color: var(--color-brand);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--space-2);
  }

  .faq-title {
    margin: 0 0 var(--space-1);
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-text);
  }

  .faq-text {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
</style>
