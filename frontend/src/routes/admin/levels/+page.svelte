<script lang="ts">
  // 2026-09 等级合并单轨：/admin/levels（等级管理）= LinuxDo 信任等级唯一入口。
  // - 每级规则表：TL / 名称 / 摘要 / 用户数 / 阈值条目（requirements 键值）
  //   （GET /api/v1/admin/trust-levels，level.manage）；
  // - 手动授予卡：user_id + TL0–TL4 + 原因（POST ?/setLevel，写审计）；
  // - 附件空间配额（M06-QUOTA）：档位键 = users.trust_level（TL0–4）+ 行内编辑
  //   （PATCH /admin/levels/{id}/attachment-quota，If-Match=policy_version +
  //   reason 审计 + step-up 重新验证；以新 policy_version 落库，仅影响新上传）。
  // 原 /admin/trust-levels 独立路由已并入本页。
  // 体系对照见 docs/TRUST-LEVELS.md §1.1。
  // 约定 D：每级「设置配额」入口 = 「⋮」三点菜单（Dialog 表单见页尾）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import { show as showToast } from '$lib/ui/toast';
  import { toastActionResult } from '$lib/ui/action-toast';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminLevelsActionData, AdminLevelsPageData, AdminTrustLevelItem } from './+page.server';

  let { data, form }: { data: AdminLevelsPageData; form?: AdminLevelsActionData | null } = $props();

  const items = $derived(data.items ?? []);
  const totalUsers = $derived(items.reduce((sum, l) => sum + (l.user_count || 0), 0));

  let refreshing = $state(false);
  async function handleRefreshCounts() {
    refreshing = true;
    try {
      await invalidateAll();
      showToast('各级用户数实时联查已完成', 'success');
    } catch {
      showToast('联查失败，请重试', 'danger');
    } finally {
      refreshing = false;
    }
  }

  // ── 阈值条目（requirements）投影 ────────────────────────────────────────

  /** 阈值键中文标签（与后端 RequirementItem label 对齐）。 */
  const REQUIREMENT_LABELS: Record<string, string> = {
    window_days: '滚动窗口（天）',
    topics_entered: '进入话题数',
    posts_read: '阅读楼层数',
    time_read_seconds: '累计阅读时长（秒）',
    days_visited: '访问天数',
    likes_given: '送出的赞',
    likes_received: '收到的赞',
    topics_replied_to: '回复不同话题',
    visit_ratio: '窗口内访问天数比例',
    replied_topics_window: '窗口内回复不同话题',
    viewed_ratio: '浏览窗口期新话题比例',
    viewed_cap: '浏览数量上限',
    read_ratio: '阅读窗口期新楼层比例',
    read_cap: '阅读数量上限',
    likes_received_window: '窗口内收到的赞',
    likes_given_window: '窗口内送出的赞',
    like_distinct_user_divisor: '点赞用户多样性（1/N）',
    like_distinct_day_divisor: '点赞天数多样性（1/N）',
    max_flags: '被确认标记上限',
    no_sanction_months: '无禁言/封禁（月）',
    manual_only: '仅手动授予'
  };

  function requirementLabel(key: string): string {
    return REQUIREMENT_LABELS[key] ?? key;
  }

  /** 布尔/比例友好投影：manual_only → '是'；ratio → 百分比；其余原样。 */
  function requirementValue(key: string, value: unknown): string {
    if (key === 'manual_only') return value ? '是' : '否';
    if (key.endsWith('_ratio') && typeof value === 'number') {
      return `${Math.round(value * 1000) / 10}%`;
    }
    if (value === null || value === undefined) return '—';
    return String(value);
  }

  function requirementsOf(item: AdminTrustLevelItem): [string, unknown][] {
    if (!item.requirements || typeof item.requirements !== 'object') return [];
    return Object.entries(item.requirements);
  }

  // ── 手动授予表单态 ─────────────────────────────────────────────────────

  let setUserId = $state('');
  let setLevel = $state(0);
  let setReason = $state('');

  const SET_OPTIONS = [
    { value: 0, label: 'TL0 新用户' },
    { value: 1, label: 'TL1 基本用户' },
    { value: 2, label: 'TL2 成员' },
    { value: 3, label: 'TL3 活跃用户' },
    { value: 4, label: 'TL4 领导者（手动授予）' }
  ] as const;

  // ── 附件空间配额（M06-QUOTA，档位键 = 信任等级）─────────────────────────

  /** 配额卡等级清单：信任规则等级；无规则时回退 TL0–TL4 全档。 */
  const quotaLevels = $derived.by(() => {
    const fromItems = items.map((l) => l.level).filter((n) => Number.isInteger(n));
    if (fromItems.length > 0) return fromItems;
    return Object.keys(data.quotas ?? {})
      .map(Number)
      .filter((n) => Number.isInteger(n))
      .sort((a, b) => a - b);
  });

  // ── 等级规则编辑器（2026-09 可配置化）──────────────────────────────────

  /** 百分比口径键（存储为 0..1 小数，输入按 % 展示）。 */
  const PERCENT_KEYS = ['visit_ratio', 'viewed_ratio', 'read_ratio'];
  /** 可添加的阈值键（TL0 无条件；TL4 仅 manual_only 且不可移除）。 */
  const ADDABLE_KEYS = [
    'topics_entered',
    'posts_read',
    'time_read_seconds',
    'days_visited',
    'likes_given',
    'likes_received',
    'topics_replied_to',
    'window_days',
    'visit_ratio',
    'replied_topics_window',
    'viewed_ratio',
    'viewed_cap',
    'read_ratio',
    'read_cap',
    'likes_received_window',
    'likes_given_window',
    'like_distinct_user_divisor',
    'like_distinct_day_divisor',
    'max_flags',
    'no_sanction_months'
  ];

  let editorLevel = $state<number | null>(null);
  let editVersion = $state(1);
  let editName = $state('');
  let editSummary = $state('');
  let editEnabled = $state(true);
  let editReqs = $state<{ key: string; value: number | boolean }[]>([]);
  let addKey = $state('');

  function currentRule(level: number): AdminTrustLevelItem | null {
    return items.find((i) => i.level === level) ?? null;
  }

  function openEditor(level: number): void {
    const item = currentRule(level);
    if (!item) return;
    editorLevel = level;
    editVersion = item.version;
    editName = item.name;
    editSummary = item.summary ?? '';
    editEnabled = item.is_enabled;
    const reqs = (item.requirements ?? {}) as Record<string, unknown>;
    editReqs = Object.entries(reqs).map(([key, value]) => ({
      key,
      value: key === 'manual_only'
        ? Boolean(value)
        : isPercent(key) && typeof value === 'number'
          ? Math.round(value * 100000) / 1000 // 0.25 → 25（%）
          : Number(value ?? 0)
    }));
    addKey = '';
  }

  function closeEditor(): void {
    editorLevel = null;
  }

  function isPercent(key: string): boolean {
    return PERCENT_KEYS.includes(key);
  }

  function removeReq(key: string): void {
    if (editorLevel === 4 && key === 'manual_only') return;
    editReqs = editReqs.filter((r) => r.key !== key);
  }

  function addReq(): void {
    if (!addKey || editReqs.some((r) => r.key === addKey)) return;
    editReqs = [...editReqs, { key: addKey, value: 0 }];
    addKey = '';
  }

  function addableKeys(level: number): string[] {
    if (level <= 0 || level === 4) return [];
    const present = new Set(editReqs.map((r) => r.key));
    return ADDABLE_KEYS.filter((k) => !present.has(k));
  }

  /** 组装 requirements_json 提交体（百分比 → 小数；TL0 恒空；TL4 恒 manual_only）。 */
  function buildRequirementsJson(level: number): string {
    if (level === 0) return '{}';
    if (level === 4) return JSON.stringify({ manual_only: true });
    const out: Record<string, unknown> = {};
    for (const { key, value } of editReqs) {
      if (key === 'manual_only') continue;
      out[key] = isPercent(key)
        ? Math.round(Number(value) * 1000) / 100000
        : Number(value) || 0;
    }
    return JSON.stringify(out);
  }

  /** 字节 → MB 展示（整除取整，否则保留最多 2 位小数）。 */
  function formatMb(bytes: number | undefined | null): string {
    if (typeof bytes !== 'number' || !Number.isFinite(bytes) || bytes <= 0) return '—';
    const mb = bytes / 1048576;
    return Number.isInteger(mb) ? String(mb) : String(Math.round(mb * 100) / 100);
  }

  function quotaOf(level: number) {
    return data.quotas?.[String(level)] ?? null;
  }

  // ── 配额批量选择（约定 B：行复选框 + BatchBar → 批量设置配额 Dialog）──────

  let selectedQuotaLevels = $state<number[]>([]);

  const allQuotaSelected = $derived(
    quotaLevels.length > 0 && selectedQuotaLevels.length === quotaLevels.length
  );

  function toggleQuota(level: number): void {
    selectedQuotaLevels = selectedQuotaLevels.includes(level)
      ? selectedQuotaLevels.filter((l) => l !== level)
      : [...selectedQuotaLevels, level].sort((a, b) => a - b);
  }

  function toggleAllQuotas(): void {
    selectedQuotaLevels = allQuotaSelected ? [] : [...quotaLevels];
  }

  // 打开批量 Dialog 时快照各档当前 policy_version（If-Match 逐档乐观锁）。
  let batchOpen = $state(false);
  let batchVersions = $state<Record<string, number>>({});
  function openBatchQuota(): void {
    const snapshot: Record<string, number> = {};
    for (const level of selectedQuotaLevels) {
      snapshot[String(level)] = quotaOf(level)?.policy_version ?? 0;
    }
    batchVersions = snapshot;
    batchOpen = true;
  }

  // action 返回后 toast 反馈（成功/失败均提示），成功后刷新配额与列表。
  $effect(() => {
    if (!form?.message) return;
    if (form.messageKind === 'error' || form.stepUpRequired) {
      showToast(form.message, 'danger');
      return;
    }
    showToast(form.message, 'success');
    invalidateAll();
    selectedQuotaLevels = [];
    batchOpen = false;
  });

  // step-up 重新验证弹窗（M02-MFA-07）：updateQuota 命中 403 时展示。
  let reauthCancelled = $state(false);
  $effect(() => {
    if (form?.stepUpRequired) reauthCancelled = false;
  });

  // 附件配额编辑弹窗（约定 A）：一个 Dialog 服务一类操作，target 为信任等级号。
  let quotaTarget = $state<number | null>(null);
  function openQuota(level: number): void {
    quotaTarget = level;
  }
</script>

<svelte:head>
  <title>等级管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="等级管理" />

{#if data.state === 'forbidden' || data.state === 'error' || data.state === 'not_implemented'}
  <section class="app-card">
    <div class="app-card__body">
      <div class="app-notice" role="alert">
        {data.state === 'forbidden'
          ? '无权查看等级管理（需要 level.manage 权限）'
          : data.state === 'not_implemented'
            ? '信任等级功能未启用'
            : '等级数据加载失败'}{data.error ? `：${data.error}` : ''}
      </div>
    </div>
  </section>
{:else}
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:flex-start;flex-wrap:wrap;gap:10px;">
      <div>
        <h2 style="margin:0;">等级体系（TL0–TL4）</h2>
        <p class="text-secondary" style="margin:4px 0 0;font-size:13px;">
          行为可信度体系：阅读/访问/点赞统计自动晋升；TL3 滚动窗口考核不达标降级（2 周宽限）；TL4 仅手动授予。
          附件配额与保留期按本页信任等级取档。
        </p>
      </div>
      <div style="display:flex;align-items:center;gap:10px;">
        <span class="sbadge sb-brand">
          <Icon name="shield-check" size={13} /> {items.length} 级
        </span>
        <button type="button" class="btn secondary sm" disabled={refreshing} onclick={handleRefreshCounts}>
          {refreshing ? '联查中…' : '用户数实时联查'}
        </button>
      </div>
    </header>
    <div class="app-card__body" style="padding:0;">
      <div class="app-table-wrap">
        <table class="app-table" style="min-width:720px;">
          <thead>
            <tr>
              <th style="width:120px;min-width:120px;">等级</th>
              <th style="min-width:180px;">摘要</th>
              <th style="width:75px;min-width:75px;">用户数</th>
              <th style="min-width:240px;">晋升条件</th>
              <th style="width:64px;min-width:64px;">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each items as item (item.level)}
              <tr>
                <td class="tl-cell">
                  <span class="lvbadge">TL{item.level} {item.name}</span>
                  {#if !item.is_enabled}
                    <span class="sbadge sb-danger">停用</span>
                  {/if}
                </td>
                <td>
                  <span class="text-secondary" style="font-size:13px;">{item.summary ?? '—'}</span>
                </td>
                <td>
                  <span style="font-variant-numeric:tabular-nums;">{item.user_count}</span>
                </td>
                <td>
                  {#if requirementsOf(item).length === 0}
                    <span class="text-secondary" style="font-size:13px;">无（默认等级）</span>
                  {:else}
                    <div style="display:flex;flex-wrap:wrap;gap:6px;">
                      {#each requirementsOf(item) as [key, value] (key)}
                        <span class="sbadge sb-gray" style="font-weight:400;">
                          {requirementLabel(key)}: {requirementValue(key, value)}
                        </span>
                      {/each}
                    </div>
                  {/if}
                </td>
                <td>
                  <!-- 约定 D：写操作 = 「⋮」菜单 → 弹层（规则编辑表单在 Dialog，见页尾）。 -->
                  <RowActionsMenu
                    label="更多操作：等级规则 TL{item.level}"
                    actions={[{ label: '编辑规则', run: () => openEditor(item.level) }]}
                  />
                </td>
              </tr>
            {:else}
              <tr>
                <td colspan="5" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                  暂无信任等级规则行
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <div style="font-size:12px;color:var(--color-text-secondary);padding:10px 16px;">
        已纳管用户（活跃）总量：<b>{totalUsers}</b> 名
      </div>
      {#if data.error}
        <div class="app-notice" role="alert" style="margin:0 16px 12px;">配额信息加载失败：{data.error}</div>
      {/if}
    </div>
  </section>

  <!-- 手动授予（TL4 唯一入口；原因写审计 admin.trust_level.set） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2 style="margin:0;font-size:16px;">手动设置信任等级</h2>
    </header>
    <div class="app-card__body">
      <p class="text-secondary" style="margin:0 0 12px;font-size:13px;">
        填写目标用户的 user_id（UUID，可在「用户管理」列表复制）。管理员可设置 0–4 级；TL4（领导者）只能在此授予。变更写入 trust_level_events 与审计日志。
      </p>
      <form
        method="POST"
        action="?/setLevel"
        use:enhance={() => {
          return async ({ result, update }) => {
            toastActionResult(result);
            if (result.type === 'success') {
              setUserId = '';
              setReason = '';
            }
            await update();
          };
        }}
        style="display:flex;flex-wrap:wrap;gap:10px;align-items:flex-end;"
      >
        <div class="input-wrapper" style="flex:2 1 260px;margin:0;">
          <label class="input-label" for="trust-user-id">目标 user_id</label>
          <input
            id="trust-user-id"
            name="user_id"
            class="input-field"
            required
            bind:value={setUserId}
            placeholder="UUID"
          />
        </div>
        <div class="input-wrapper" style="flex:1 1 180px;margin:0;">
          <label class="input-label" for="trust-set-level">信任等级</label>
          <select id="trust-set-level" name="level" class="input-field" bind:value={setLevel}>
            {#each SET_OPTIONS as opt (opt.value)}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </div>
        <div class="input-wrapper" style="flex:2 1 220px;margin:0;">
          <label class="input-label" for="trust-set-reason">操作原因（写审计）</label>
          <input
            id="trust-set-reason"
            name="reason"
            class="input-field"
            required
            bind:value={setReason}
            placeholder="必填"
          />
        </div>
        <Button text="保存" variant="primary" size="sm" type="submit" />
      </form>
    </div>
  </section>

  <!-- 附件空间配额（按信任等级）：摘要行 + 「⋮」菜单 → Dialog 编辑（reason 审计 + step-up 重新验证） -->
  <section class="app-card">
    <header class="app-card__head">
      <div style="display:flex;align-items:flex-start;justify-content:space-between;gap:12px;flex-wrap:wrap;width:100%;">
        <div>
          <h2 style="margin:0;">附件空间配额（按信任等级）</h2>
          <p style="margin:4px 0 0;font-size:12px;color:var(--color-text-secondary);">
            单文件上限 / 总容量 / 每日上传量以 MB 填写；保存创建新策略版本（If-Match 乐观锁）并写入审计，
            仅影响之后的新上传。单文件上限 ≤ 总容量 ≤ 站点硬上限（8 GiB）。
          </p>
        </div>
        <ExportButton
          label="导出配额 CSV"
          filename="trust-level-quotas"
          format="csv"
          columns={[
            { key: 'level', label: '等级' },
            { key: 'single_max_mb', label: '单文件上限(MB)' },
            { key: 'total_mb', label: '总容量(MB)' },
            { key: 'daily_mb', label: '每日上传量(MB)' },
            { key: 'retention_days', label: '保留天数' }
          ]}
          getData={() =>
            quotaLevels.map((level) => {
              const policy = data.quotas?.[String(level)];
              return {
                level: `TL${level}`,
                single_max_mb: policy ? Math.round(policy.single_file_max_bytes / 1024 / 1024) : '',
                total_mb: policy ? Math.round(policy.total_bytes / 1024 / 1024) : '',
                daily_mb: policy ? Math.round(policy.daily_upload_bytes / 1024 / 1024) : '',
                retention_days: policy?.retention_days ?? ''
              };
            })
          }
        />
      </div>
    </header>
    <div class="app-card__body" style="display:flex;flex-direction:column;gap:10px;">
      {#if quotaLevels.length === 0}
        <div class="app-notice">暂无等级配额数据。</div>
      {:else}
        <!-- 批量工具条（约定 B：选中后渲染；批量参数在 Dialog 内填写） -->
        <BatchBar count={selectedQuotaLevels.length} noun="个等级档位" onclear={() => (selectedQuotaLevels = [])}>
          <Button text="批量设置配额" variant="secondary" size="sm" onclick={openBatchQuota} />
        </BatchBar>
        <div style="display:flex;align-items:center;gap:10px;padding:0 4px;">
          <input
            type="checkbox"
            checked={allQuotaSelected}
            onchange={toggleAllQuotas}
            aria-label="全选等级档位"
          />
          <span style="font-size:12px;color:var(--color-text-secondary);">全选（共 {quotaLevels.length} 档）</span>
        </div>
        {#each quotaLevels as level (level)}
          {@const policy = quotaOf(level)}
          <div class="quota-level">
            <div class="quota-level__summary">
              <input
                type="checkbox"
                checked={selectedQuotaLevels.includes(level)}
                onchange={() => toggleQuota(level)}
                aria-label="选择信任等级 TL{level}"
              />
              <b>TL{level}</b>
              {#if policy}
                <span style="font-size:12px;color:var(--color-text-secondary);">
                  总容量 {formatMb(policy.total_bytes)} MB · 单文件 {formatMb(policy.single_file_max_bytes)} MB ·
                  每日 {formatMb(policy.daily_upload_bytes)} MB · 保留 {policy.retention_days} 天 ·
                  策略版本 v{policy.policy_version}
                </span>
              {:else}
                <span style="font-size:12px;color:var(--color-text-secondary);">暂无策略（保存后创建 v1）</span>
              {/if}
              <!-- 约定 D：写操作 = 每行一个「⋮」菜单 → 弹层（配额表单在 Dialog，见页尾）。 -->
              <span style="margin-left:auto;flex-shrink:0;">
                <RowActionsMenu
                  label="更多操作：信任等级 TL{level}"
                  actions={[{ label: '设置附件配额', run: () => openQuota(level) }]}
                />
              </span>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </section>
{/if}

<!-- 附件配额编辑：Dialog 内表单（约定 A），隐藏 level + policy_version（If-Match）
     + 必填 reason（写审计）；成功后关闭弹层（toast 由 form.message effect 统一提示）。 -->
<Dialog
  open={quotaTarget !== null}
  title="设置 TL{quotaTarget ?? ''} 附件配额"
  description="保存创建新策略版本（If-Match 乐观锁）并写入审计，仅影响之后的新上传；既有附件不受影响。"
  onclose={() => (quotaTarget = null)}
>
  {#if quotaTarget !== null}
    {@const policy = quotaOf(quotaTarget)}
    <form
      method="POST"
      action="?/updateQuota"
      use:enhance={() => {
        return async ({ result, update }) => {
          await update();
          if (result.type === 'success') quotaTarget = null;
        };
      }}
      style="display:flex;flex-direction:column;gap:10px;"
    >
      <input type="hidden" name="level" value={quotaTarget} />
      <input type="hidden" name="policy_version" value={policy?.policy_version ?? 0} />
      <div class="quota-grid">
        <label class="quota-field">
          <span>总容量（MB）</span>
          <input
            type="number"
            name="total_mb"
            min="1"
            step="any"
            required
            value={policy ? formatMb(policy.total_bytes) : ''}
            aria-label="信任等级 {quotaTarget} 总容量（MB）"
          />
        </label>
        <label class="quota-field">
          <span>单文件上限（MB）</span>
          <input
            type="number"
            name="single_file_mb"
            min="1"
            step="any"
            required
            value={policy ? formatMb(policy.single_file_max_bytes) : ''}
            aria-label="信任等级 {quotaTarget} 单文件上限（MB）"
          />
        </label>
        <label class="quota-field">
          <span>每日上传量（MB）</span>
          <input
            type="number"
            name="daily_mb"
            min="1"
            step="any"
            required
            value={policy ? formatMb(policy.daily_upload_bytes) : ''}
            aria-label="信任等级 {quotaTarget} 每日上传量（MB）"
          />
        </label>
        <label class="quota-field">
          <span>删除保留期（天）</span>
          <input
            type="number"
            name="retention_days"
            min="0"
            max="3650"
            step="1"
            required
            value={policy ? policy.retention_days : 30}
            aria-label="信任等级 {quotaTarget} 删除保留期（天）"
          />
        </label>
      </div>
      <label class="quota-field">
        <span>操作原因（必填，写入审计日志）</span>
        <input
          type="text"
          name="reason"
          required
          maxlength={200}
          placeholder="例如：调整 TL{quotaTarget} 存储策略"
          aria-label="信任等级 {quotaTarget} 配额操作原因"
        />
      </label>
      <div style="display:flex;gap:8px;align-items:center;">
        <button type="submit" class="btn primary sm">保存配额</button>
        <button type="button" class="btn ghost sm" onclick={() => (quotaTarget = null)}>取消</button>
      </div>
    </form>
  {/if}
</Dialog>

<!-- 等级规则编辑器（2026-09 可配置化）：名称/摘要/启用 + 逐条阈值条件增删改；
     保存 = PATCH（If-Match=version + reason 审计），恢复默认 = POST reset（reason 审计）。
     停用级不参与自动晋升/TL3 自动降级（手动授予不受影响）；保存后立即生效于评估引擎。 -->
<Dialog
  open={editorLevel !== null}
  title="编辑 TL{editorLevel ?? ''} 等级规则"
  description="保存立即生效于自动评估（If-Match 乐观锁 + reason 审计）；「恢复默认」将该级重置为系统内置默认规则。"
  onclose={closeEditor}
>
  {#if editorLevel !== null}
    <form
      method="POST"
      action="?/updateRule"
      use:enhance={() => {
        return async ({ result, update }) => {
          await update();
          if (result.type === 'success') closeEditor();
        };
      }}
      style="display:flex;flex-direction:column;gap:12px;"
    >
      <input type="hidden" name="level" value={editorLevel} />
      <input type="hidden" name="version" value={editVersion} />
      <input type="hidden" name="requirements_json" value={buildRequirementsJson(editorLevel)} />
      <div class="quota-grid">
        <label class="quota-field">
          <span>等级名称（1–50 字）</span>
          <input type="text" name="name" required maxlength={50} bind:value={editName} />
        </label>
        <label class="quota-field">
          <span>摘要（≤200 字，可留空）</span>
          <input type="text" name="summary" maxlength={200} bind:value={editSummary} />
        </label>
      </div>
      <label style="display:flex;align-items:center;gap:8px;font-size:13px;">
        <input type="checkbox" name="is_enabled" value="1" bind:checked={editEnabled} />
        启用该等级（停用后不参与自动晋升，TL3 停用时不再自动降级；手动授予不受影响）
      </label>

      {#if editorLevel === 0}
        <p class="text-secondary" style="margin:0;font-size:12px;">
          TL0 为注册默认等级，不可配置晋升条件。
        </p>
      {:else}
        <div style="display:flex;flex-direction:column;gap:8px;">
          <span class="quota-field"><span>晋升条件（满足全部条件后自动晋升）</span></span>
          {#each editReqs as req, i (req.key)}
            <div style="display:flex;align-items:center;gap:8px;">
              <span style="flex:1 1 180px;font-size:12px;color:var(--color-text-secondary);">
                {requirementLabel(req.key)}
              </span>
              {#if req.key === 'manual_only'}
                <input type="checkbox" checked disabled aria-label="仅手动授予（TL4 固定）" />
              {:else}
                <input
                  type="number"
                  min="0"
                  step={isPercent(req.key) ? '0.1' : '1'}
                  style="width:130px;"
                  bind:value={req.value}
                  aria-label="{requirementLabel(req.key)} 阈值"
                />
                {#if isPercent(req.key)}
                  <span style="font-size:12px;color:var(--color-text-secondary);">%</span>
                {/if}
              {/if}
              <button
                type="button"
                class="btn ghost sm"
                onclick={() => removeReq(req.key)}
                disabled={editorLevel === 4 && req.key === 'manual_only'}
                aria-label="移除条件 {requirementLabel(req.key)}"
              >移除</button>
              <span hidden>{i}</span>
            </div>
          {/each}
          {#if addableKeys(editorLevel).length > 0}
            <div style="display:flex;align-items:center;gap:8px;">
              <select class="input-field" style="flex:1 1 200px;" bind:value={addKey} aria-label="选择要添加的晋升条件">
                <option value="" disabled selected>选择要添加的条件…</option>
                {#each addableKeys(editorLevel) as key (key)}
                  <option value={key}>{requirementLabel(key)}</option>
                {/each}
              </select>
              <button type="button" class="btn secondary sm" onclick={addReq} disabled={!addKey}>添加条件</button>
            </div>
          {/if}
        </div>
      {/if}

      <label class="quota-field">
        <span>操作原因（必填，写入审计日志）</span>
        <input type="text" name="reason" required maxlength={500} placeholder="例如：调整 TL{editorLevel ?? ''} 晋升门槛" />
      </label>
      <div style="display:flex;gap:8px;align-items:center;">
        <button type="submit" class="btn primary sm">保存规则</button>
        <button type="submit" class="btn ghost sm" formaction="?/resetRule">恢复默认</button>
        <button type="button" class="btn ghost sm" onclick={closeEditor}>取消</button>
      </div>
    </form>
  {/if}
</Dialog>

<!-- 批量设置配额（约定 B）：对选中档位应用同一配额；每档各自 If-Match=打开时
     快照的 policy_version（隐藏 JSON），单档冲突不阻塞其余档，结果按档汇总提示。 -->
<Dialog
  open={batchOpen}
  title="批量设置附件配额（已选 {selectedQuotaLevels.length} 档）"
  description="对选中等级应用同一配额：每档创建独立新策略版本（If-Match 乐观锁）并写入审计，仅影响之后的新上传。单档版本冲突不影响其他档。"
  onclose={() => (batchOpen = false)}
>
  <form
    method="POST"
    action="?/batchQuota"
    use:enhance={() => {
      return async ({ result, update }) => {
        await update();
        if (result.type === 'success') batchOpen = false;
      };
    }}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <input type="hidden" name="targets_json" value={JSON.stringify(selectedQuotaLevels)} />
    <input type="hidden" name="versions_json" value={JSON.stringify(batchVersions)} />
    <p class="text-secondary" style="margin:0;font-size:12px;">
      目标档位：TL{selectedQuotaLevels.join('、TL')}
    </p>
    <div class="quota-grid">
      <label class="quota-field">
        <span>总容量（MB）</span>
        <input type="number" name="total_mb" min="1" step="any" required aria-label="批量总容量（MB）" />
      </label>
      <label class="quota-field">
        <span>单文件上限（MB）</span>
        <input type="number" name="single_file_mb" min="1" step="any" required aria-label="批量单文件上限（MB）" />
      </label>
      <label class="quota-field">
        <span>每日上传量（MB）</span>
        <input type="number" name="daily_mb" min="1" step="any" required aria-label="批量每日上传量（MB）" />
      </label>
      <label class="quota-field">
        <span>删除保留期（天）</span>
        <input type="number" name="retention_days" min="0" max="3650" step="1" required value={30} aria-label="批量删除保留期（天）" />
      </label>
    </div>
    <label class="quota-field">
      <span>操作原因（必填，写入审计日志）</span>
      <input type="text" name="reason" required maxlength={200} placeholder="例如：统一调整各档存储策略" aria-label="批量配额操作原因" />
    </label>
    <div style="display:flex;gap:8px;align-items:center;">
      <button type="submit" class="btn primary sm">应用 {selectedQuotaLevels.length} 档</button>
      <button type="button" class="btn ghost sm" onclick={() => (batchOpen = false)}>取消</button>
    </div>
  </form>
</Dialog>

<!-- step-up 重新验证（M02-MFA-07）：配额保存命中 403 step_up_required 时弹窗 -->
<Dialog
  open={Boolean(form?.stepUpRequired) && !reauthCancelled}
  title="需要重新验证身份"
  description="附件配额属于高风险管理操作，要求近期重新认证（登录已超过有效期）。输入当前账号密码完成重新验证后，可继续保存。"
  onclose={() => (reauthCancelled = true)}
>
  <form
    method="POST"
    action="?/reauth"
    class="stack"
    style="gap:10px;"
  >
    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">当前密码</span>
      <input
        type="password"
        name="password"
        class="input-field"
        required
        autocomplete="current-password"
        style="width:100%;"
      />
    </label>
    <div style="display:flex;justify-content:flex-end;gap:8px;">
      <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
      <button type="submit" class="btn primary sm">重新验证</button>
    </div>
  </form>
</Dialog>

<style>
  /* 等级列不换行（lvbadge 含名称，窄列被压缩时会折行成两行） */
  .tl-cell {
    white-space: nowrap;
  }
  .quota-level {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
    background: var(--color-bg-subtle);
  }
  .quota-level__summary {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
  }
  .quota-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 10px;
  }
  .quota-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--color-text-secondary);
  }
  .quota-field input {
    width: 100%;
  }
</style>
