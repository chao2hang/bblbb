<script lang="ts">
  // GAP-FIX（管理域·成就管理）：目录表格（code/名称/分类/条件/奖励/状态/解锁数）
  // + 新建成就表单 + 行操作（启停 If-Match / 手工授予 / 删除，均收进行操作弹层）。
  // M18-ADMIN-OPS（约定 D）：行内「启停表单 / 上传图标 / 移除图标 / 手工授予 / 删除」
  // 多个写入口收敛为每行一个「⋮」三点菜单（RowActionsMenu）→ 点菜单项打开该操作的
  // 单动作确认 Dialog（启停 ?/toggle If-Match；上传图标 ?/uploadIcon file input；
  // 移除图标 ?/removeIcon；手工授予 ?/grant username+reason；删除 ?/delete reason
  // ——危险 danger 提交按钮）。
  // 批量启用/停用（BatchBar + ?/bulk）与页头「新建成就」表单保持不变。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import StatCard from '$lib/components/admin/StatCard.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type {
    AdminAchievementItem,
    AdminAchievementsActionData,
    AdminAchievementsPageData
  } from './+page.server';

  let { data, form }: {
    data: AdminAchievementsPageData;
    form?: AdminAchievementsActionData | null;
  } = $props();

  /** 条件类型（与后端 CONDITION_TYPES 一致）。 */
  const CONDITION_TYPES = [
    { value: 'post_count', label: '发帖数' },
    { value: 'comment_count', label: '回复数' },
    { value: 'reaction_received', label: '被赞数' },
    { value: 'checkin_streak', label: '连续签到' },
    { value: 'follower_count', label: '粉丝数' },
    { value: 'manual', label: '手工授予' }
  ];

  function conditionLabel(type: string): string {
    return CONDITION_TYPES.find((c) => c.value === type)?.label ?? type;
  }

  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  /** 行操作「⋮」菜单 + 弹层（约定 D：菜单项决定动作，弹层内单动作确认）。
   *  按 opsAction 渲染对应表单节：启停（?/toggle If-Match）/
   *  图标（?/uploadIcon + ?/removeIcon）/ 手工授予（?/grant username+reason）/
   *  删除（?/delete reason，danger 提交）。target 区分行，草稿随关闭清空。 */
  type OpsAction = 'toggle' | 'uploadIcon' | 'removeIcon' | 'grant' | 'delete';
  let opsTarget: AdminAchievementItem | null = $state(null);
  let opsAction = $state<OpsAction>('toggle');
  let opsToggleReason = $state('');
  let opsIconReason = $state('');
  let opsGrantUsername = $state('');
  let opsGrantReason = $state('');
  let opsDeleteReason = $state('');

  function openOps(item: AdminAchievementItem, action: OpsAction): void {
    opsTarget = item;
    opsAction = action;
    opsToggleReason = '';
    opsIconReason = '';
    opsGrantUsername = '';
    opsGrantReason = '';
    opsDeleteReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
    opsToggleReason = '';
    opsIconReason = '';
    opsGrantUsername = '';
    opsGrantReason = '';
    opsDeleteReason = '';
  }

  /** 行「⋮」菜单项：启停（状态取反文案）/ 上传图标 / 移除图标（仅已有图标）/
   *  手工授予 / 删除（危险）。 */
  function rowActions(item: AdminAchievementItem) {
    const actions: { label: string; danger?: boolean; run: () => void }[] = [
      { label: item.is_enabled ? '停用' : '启用', run: () => openOps(item, 'toggle') },
      { label: '上传图标', run: () => openOps(item, 'uploadIcon') }
    ];
    if (item.icon_url) {
      actions.push({ label: '移除图标', run: () => openOps(item, 'removeIcon') });
    }
    actions.push({ label: '手工授予', run: () => openOps(item, 'grant') });
    actions.push({ label: '删除', danger: true, run: () => openOps(item, 'delete') });
    return actions;
  }

  /** 弹层标题/描述随菜单选定动作切换（单动作确认，非分节选择）。 */
  function opsActionMetaFor(item: AdminAchievementItem, action: OpsAction): {
    title: string;
    description: string;
  } {
    const subject = `${item.name}（${item.code}）`;
    switch (action) {
      case 'toggle':
        return {
          title: `${item.is_enabled ? '停用' : '启用'}成就：${subject}`,
          description: `将该成就切换为「${item.is_enabled ? '停用' : '启用'}」（If-Match 乐观锁）；操作原因写审计。`
        };
      case 'uploadIcon':
        return {
          title: `上传成就图标：${subject}`,
          description: 'png/jpeg/webp/gif，≤2MB；存储在站点本地磁盘，不经 S3。'
        };
      case 'removeIcon':
        return {
          title: `移除成就图标：${subject}`,
          description: '移除后该成就回退默认占位图标；原因写入审计日志。'
        };
      case 'grant':
        return {
          title: `手工授予成就：${subject}`,
          description: '按用户名授予（已解锁则幂等保持原解锁时间）；授予原因写审计。'
        };
      case 'delete':
        return {
          title: `删除成就：${subject}`,
          description: '危险操作：将级联删除全部解锁记录，不可恢复；删除原因写审计。'
        };
    }
  }

  const opsActionMeta = $derived(opsTarget ? opsActionMetaFor(opsTarget, opsAction) : null);

  // ── 统计卡 + 工具栏（视觉对齐 M17-GAPFIX-06：原型 achievement-stats/
  //    app-filter-tabs 同构；搜索/筛选为客户端过滤，无 JS 时显示全量）──
  const items = $derived(data.items ?? []);
  const stats = $derived.by(() => ({
    total: items.length,
    enabled: items.filter((i) => i.is_enabled).length,
    unlocked: items.reduce((sum, i) => sum + i.unlocked_count, 0),
    incomplete: items.filter((i) => !i.description.trim()).length
  }));

  let search = $state('');
  let statusFilter = $state('');
  let categoryFilter = $state('');

  // 批量选择（M17-GAPFIX-07：原型批量条 同构；约定 B 接共享 BatchBar）。
  let selected = $state(new Set<string>());
  function toggleSelect(code: string): void {
    const next = new Set(selected);
    if (next.has(code)) next.delete(code);
    else next.add(code);
    selected = next;
  }

  // 批量启用/停用 Dialog（一个 Dialog 服务一类操作，方向由 batchNextEnabled 区分；
  // 提交 ?/bulk——服务端循环调用与 ?/toggle 相同的 PATCH /admin/achievements/{code}）。
  let batchOpen = $state(false);
  let batchNextEnabled = $state(true);
  let batchReason = $state('');
  function openBatch(next: boolean): void {
    batchNextEnabled = next;
    batchReason = '';
    batchOpen = true;
  }

  const categories = $derived([...new Set(items.map((i) => i.category))]);

  const filteredItems = $derived.by(() => {
    const kw = search.trim().toLowerCase();
    return items.filter((i) => {
      if (kw && !`${i.code} ${i.name} ${i.description}`.toLowerCase().includes(kw)) return false;
      if (statusFilter === 'enabled' && !i.is_enabled) return false;
      if (statusFilter === 'disabled' && i.is_enabled) return false;
      if (categoryFilter && i.category !== categoryFilter) return false;
      return true;
    });
  });
  const allSelected = $derived(filteredItems.length > 0 && filteredItems.every((i) => selected.has(i.code)));
  function toggleSelectAll(): void {
    if (allSelected) selected = new Set();
    else selected = new Set(filteredItems.map((i) => i.code));
  }

  function clearFilters(): void {
    search = '';
    statusFilter = '';
    categoryFilter = '';
  }

</script>

<svelte:head>
  <title>成就管理 — BBLBB</title>
</svelte:head>

<PageHeader title="成就管理" />

{#if data.state === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    </div>
  </div>
{:else if data.state === 'not_implemented'}
  <div class="app-card">
    <div class="app-card__body">
      <p class="input-hint" role="note">成就管理接口开发中。</p>
    </div>
  </div>
{:else if data.state === 'error'}
  <div class="app-card">
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    </div>
  </div>
{:else}
  {#if message && !hasJs}
    <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
  {/if}
  {#if conflict && !hasJs}
    <p class="input-hint is-error" role="alert">成就版本已变化（If-Match 乐观锁冲突），请刷新后重试。</p>
  {/if}

  <div class="app-card" style="margin-bottom:var(--space-4);">
    <div class="app-card__head"><h2>新建成就</h2></div>
    <div class="app-card__body">
      <form
        method="POST"
        action="?/create"
        use:enhance={() => {
          return async ({ result, update }) => {
            // 结果 message 走全局 Toast（兜底文案与原先一致）
            toastActionResult(result, {
              message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '创建成功' : '创建失败')
            });
            await update();
          };
        }}
      >
        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:var(--space-3);">
          <div class="input-wrapper">
            <label class="input-label" for="ac-code">code（唯一）</label>
            <input id="ac-code" name="code" class="input-field" required pattern="[a-z0-9_-]&#123;1,64&#125;" placeholder="first_post" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="ac-name">名称（1-120 字）</label>
            <input id="ac-name" name="name" class="input-field" required maxlength="120" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="ac-category">分类（1-32 字）</label>
            <input id="ac-category" name="category" class="input-field" required maxlength="32" placeholder="community" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="ac-condition-type">条件类型</label>
            <select id="ac-condition-type" name="condition_type" class="input-field">
              {#each CONDITION_TYPES as ct (ct.value)}
                <option value={ct.value}>{ct.label}（{ct.value}）</option>
              {/each}
            </select>
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="ac-threshold">条件阈值</label>
            <input id="ac-threshold" name="condition_threshold" type="number" min="0" step="1" class="input-field" required value="1" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="ac-reward-coin">奖励金币</label>
            <input id="ac-reward-coin" name="reward_coin" type="number" min="0" step="1" class="input-field" value="0" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="ac-sort">排序</label>
            <input id="ac-sort" name="sort_order" type="number" step="1" class="input-field" value="0" />
          </div>
        </div>
        <div class="input-wrapper" style="margin-top:var(--space-3);">
          <label class="input-label" for="ac-description">描述（1-500 字）</label>
          <textarea id="ac-description" name="description" class="input-field" required maxlength="500" rows="2"></textarea>
        </div>
        <div style="display:flex;gap:var(--space-4);align-items:center;margin-top:var(--space-3);flex-wrap:wrap;">
          <label class="input-label" style="display:flex;align-items:center;gap:var(--space-1);">
            <input type="checkbox" name="is_hidden" /> 隐藏成就（条件仅管理端可见）
          </label>
          <label class="input-label" style="display:flex;align-items:center;gap:var(--space-1);">
            <input type="checkbox" name="is_enabled" checked /> 立即启用
          </label>
          <div class="input-wrapper" style="flex:1;min-width:200px;">
            <label class="input-label" for="ac-reason">操作原因（写审计）</label>
            <input id="ac-reason" name="reason" class="input-field" required placeholder="必填" />
          </div>
          <Button text="创建成就" variant="primary" size="sm" type="submit" />
        </div>
        <p class="input-hint" style="margin-top:var(--space-2);">
          成就创建后可在下方列表「图标」列上传成就图片（png/jpeg/webp/gif，≤2MB；存储在站点本地磁盘，不经 S3）。
        </p>
      </form>
    </div>
  </div>

  <!-- 统计卡（原型 achievement-stats 四项） -->
  <div class="app-stat-grid">
    <StatCard value={stats.total} label="成就总数" icon="trophy" />
    <StatCard value={stats.enabled} label="启用中" icon="check" />
    <StatCard value={stats.unlocked} label="累计解锁" icon="award" />
    <StatCard value={stats.incomplete} label="待完善" icon="alert-triangle" note="缺少描述" />
  </div>

  <div class="app-card" style="margin-top:14px;">
    <div class="app-card__head">
      <h2>成就定义</h2>
      <span class="text-secondary" style="font-size:12px;">共 {items.length} 项 · 支持按名称、分类和状态查找</span>
    </div>
    <div class="card-body" style="padding:0;">
      {#if items.length > 0}
        <form class="app-toolbar" style="padding:12px 14px;gap:8px;" onsubmit={(e) => e.preventDefault()}>
          <input
            type="text"
            class="app-field"
            placeholder="搜索 code、标题或条件"
            bind:value={search}
            aria-label="搜索成就"
            style="min-width:200px;"
          />
          <select class="app-select" bind:value={statusFilter} aria-label="状态筛选" style="width:130px;">
            <option value="">全部状态</option>
            <option value="enabled">已启用</option>
            <option value="disabled">已停用</option>
          </select>
          <select class="app-select" bind:value={categoryFilter} aria-label="分类筛选" style="width:150px;">
            <option value="">全部分类</option>
            {#each categories as c (c)}
              <option value={c}>{c}</option>
            {/each}
          </select>
          <button type="button" class="btn ghost sm" onclick={clearFilters}>清除</button>
          <span class="app-spacer" style="flex:1;"></span>
          <ExportButton label="导出配置" filename="achievements" format="json" getData={() => items as unknown as Record<string, unknown>[]} />
        </form>
      {/if}
      {#if !data.items || data.items.length === 0}
        <div style="padding:var(--space-4);">
          <EmptyState icon="inbox" title="暂无成就" desc="还没有定义任何成就" />
        </div>
      {:else if filteredItems.length === 0}
        <div style="padding:var(--space-4);">
          <p class="input-hint">没有符合该筛选条件的成就。</p>
        </div>
      {:else}
        <BatchBar count={selected.size} onclear={() => (selected = new Set())}>
          <Button text="批量启用" variant="secondary" size="sm" onclick={() => openBatch(true)} />
          <Button text="批量停用" variant="danger" size="sm" onclick={() => openBatch(false)} />
        </BatchBar>
        <div style="overflow-x:auto;">
          <table class="app-table" aria-label="成就列表">
            <thead>
              <tr>
                <th style="width:32px;">
                  <input
                    type="checkbox"
                    aria-label="全选"
                    checked={allSelected}
                    onchange={toggleSelectAll}
                  />
                </th>
                <th>code</th>
                <th style="width:120px;">图标</th>
                <th>名称</th>
                <th>分类</th>
                <th>条件</th>
                <th>奖励</th>
                <th>状态</th>
                <th>解锁数</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredItems as item (item.code)}
                <tr>
                  <td>
                    <input
                      type="checkbox"
                      aria-label="选中 {item.code}"
                      checked={selected.has(item.code)}
                      onchange={() => toggleSelect(item.code)}
                    />
                  </td>
                  <td><code style="font-size:var(--text-sm);">{item.code}</code></td>
                  <td>
                    <!-- 成就图标（不走 S3）：仅预览；上传/移除入口收进行「操作」弹层。
                         预览带 cache-bust（上传后 version 变化 → URL 追加 v=，绕过 5min 缓存）。 -->
                    {#if item.icon_url}
                      <img
                        src="{item.icon_url}?v={item.version}"
                        alt="{item.name} 成就图标"
                        width="38"
                        height="38"
                        loading="lazy"
                        style="width:38px;height:38px;border-radius:10px;object-fit:cover;border:1px solid var(--color-border);background:var(--color-bg-subtle);"
                      />
                    {:else}
                      <span
                        class="achievement-icon achievement-icon--sm"
                        aria-hidden="true"
                        style="width:38px;height:38px;"
                      >
                        <Icon name="award" size={16} />
                      </span>
                    {/if}
                  </td>
                  <td>
                    {item.name}
                    {#if item.is_hidden}<span class="badge badge-neutral" style="margin-left:4px;">隐藏</span>{/if}
                    <span class="text-secondary" style="display:block;font-size:var(--text-xs);max-width:220px;">{item.description}</span>
                  </td>
                  <td><span class="text-secondary" style="font-size:var(--text-sm);">{item.category}</span></td>
                  <td>
                    <span style="font-size:var(--text-sm);">{conditionLabel(item.condition_type)}</span>
                    <span class="text-secondary" style="display:block;font-size:var(--text-xs);">≥ {item.condition_threshold}</span>
                  </td>
                  <td>
                    <span class="text-secondary" style="font-size:var(--text-sm);">币 {item.reward_coin}</span>
                  </td>
                  <td>
                    {#if item.is_enabled}
                      <span class="badge badge-success">启用</span>
                    {:else}
                      <span class="badge badge-neutral">停用</span>
                    {/if}
                  </td>
                  <td><span style="font-variant-numeric:tabular-nums;">{item.unlocked_count}</span></td>
                  <td>
                    <!-- 每行一个「⋮」动作菜单（约定 D）：菜单项打开对应单动作确认 Dialog -->
                    <RowActionsMenu
                      label="更多操作：成就 {item.name}"
                      actions={rowActions(item)}
                    />
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <footer class="app-card__foot" style="padding:10px 14px;">所有配置变更都会追加到审计日志。</footer>
      {/if}
    </div>
  </div>

  <!-- 行操作 Dialog（约定 D：动作由「⋮」菜单选定，单动作确认 + reason 必填）：
       按 opsAction 渲染对应表单节，各节独立提交既有契约；成功 toastActionResult →
       update → 关弹层清 target/草稿。 -->
  <Dialog
    open={opsTarget !== null}
    title={opsActionMeta?.title ?? '成就操作'}
    description={opsActionMeta?.description ?? ''}
    onclose={closeOps}
  >
    {#if opsAction === 'toggle'}
      <!-- 动作节：启停（?/toggle，If-Match version + 目标 is_enabled + 必填原因） -->
      <form
        method="POST"
        action="?/toggle"
        use:enhance={() => {
          return async ({ result, update }) => {
            // 结果 message 走全局 Toast（兜底文案与原先一致）
            toastActionResult(result, {
              message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '操作成功' : '操作失败')
            });
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;"
      >
        <input type="hidden" name="code" value={opsTarget?.code ?? ''} />
        <input type="hidden" name="version" value={String(opsTarget?.version ?? '')} />
        <input type="hidden" name="is_enabled" value={opsTarget ? String(!opsTarget.is_enabled) : ''} />
        <div class="input-wrapper">
          <label class="input-label" for="ach-ops-toggle-reason">操作原因（写审计）</label>
          <input id="ach-ops-toggle-reason" name="reason" class="input-field" required bind:value={opsToggleReason} placeholder="必填" />
        </div>
        <div>
          <Button
            text={opsTarget?.is_enabled ? '确认停用' : '确认启用'}
            variant="primary"
            size="sm"
            type="submit"
          />
        </div>
      </form>
    {:else if opsAction === 'uploadIcon'}
      <!-- 动作节：上传图标（?/uploadIcon multipart。存储在站点本地磁盘，不经 S3；
           png/jpeg/webp/gif，≤2MB）。 -->
      <form
        method="POST"
        action="?/uploadIcon"
        enctype="multipart/form-data"
        use:enhance={() => {
          return async ({ result, update }) => {
            toastActionResult(result, {
              message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '图标已更新' : '上传失败')
            });
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;"
      >
        <input type="hidden" name="code" value={opsTarget?.code ?? ''} />
        <input
          type="file"
          name="icon"
          accept="image/png,image/jpeg,image/webp,image/gif"
          required
          aria-label="成就图标文件（png/jpeg/webp/gif，≤2MB）"
        />
        <div class="input-wrapper">
          <label class="input-label" for="ach-ops-icon-reason">操作原因（写审计）</label>
          <input id="ach-ops-icon-reason" name="reason" class="input-field" required bind:value={opsIconReason} placeholder="必填" />
        </div>
        <div><Button text="上传图标" variant="secondary" size="sm" type="submit" /></div>
      </form>
    {:else if opsAction === 'removeIcon' && opsTarget?.icon_url}
      <!-- 动作节：移除图标（?/removeIcon；仅已有图标时菜单提供该项） -->
      <form
        method="POST"
        action="?/removeIcon"
        use:enhance={() => {
          return async ({ result, update }) => {
            toastActionResult(result, {
              message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '图标已移除' : '移除失败')
            });
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="margin-top:6px;"
      >
        <input type="hidden" name="code" value={opsTarget?.code ?? ''} />
        <input type="hidden" name="reason" value="移除成就图标（管理台操作）" />
        <Button text="移除当前图标" variant="ghost" size="sm" type="submit" />
      </form>
    {:else if opsAction === 'grant'}
      <!-- 动作节：手工授予（?/grant，username + reason；已解锁则幂等保持原解锁时间） -->
      <form
        method="POST"
        action="?/grant"
        use:enhance={() => {
          return async ({ result, update }) => {
            // 结果 message 走全局 Toast（兜底文案与原先一致）
            toastActionResult(result, {
              message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '已授予' : '授予失败')
            });
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;"
      >
        <input type="hidden" name="code" value={opsTarget?.code ?? ''} />
        <div class="input-wrapper">
          <label class="input-label" for="ach-ops-grant-username">用户名</label>
          <input id="ach-ops-grant-username" name="username" class="input-field" required bind:value={opsGrantUsername} placeholder="member_username" />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="ach-ops-grant-reason">授予原因（写审计）</label>
          <input id="ach-ops-grant-reason" name="reason" class="input-field" required bind:value={opsGrantReason} placeholder="必填" />
        </div>
        <div><Button text="确认授予" variant="primary" size="sm" type="submit" /></div>
      </form>
    {:else if opsAction === 'delete'}
      <!-- 动作节：删除（?/delete，reason 必填；危险动作 → danger 提交按钮，级联删除解锁记录）。 -->
      <form
        method="POST"
        action="?/delete"
        use:enhance={() => {
          return async ({ result, update }) => {
            // 结果 message 走全局 Toast（兜底文案与原先一致）
            toastActionResult(result, {
              message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '已删除' : '删除失败')
            });
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;"
      >
        <input type="hidden" name="code" value={opsTarget?.code ?? ''} />
        <div class="input-wrapper">
          <label class="input-label" for="ach-ops-delete-reason">删除原因（写审计）</label>
          <input id="ach-ops-delete-reason" name="reason" class="input-field" required bind:value={opsDeleteReason} placeholder="必填" />
        </div>
        <p class="input-hint is-error" style="margin:0;">将级联删除全部解锁记录，不可恢复。</p>
        <div><Button text="确认删除" variant="danger" size="sm" type="submit" /></div>
      </form>
    {/if}
  </Dialog>

  <!-- 批量启用/停用：Dialog 内填公共原因，POST ?/bulk（服务端循环调用与
       ?/toggle 相同的 PATCH /admin/achievements/{code} 端点并逐条 If-Match）。 -->
  <Dialog
    open={batchOpen}
    title={batchNextEnabled ? '批量启用成就' : '批量停用成就'}
    description={`将对 ${selected.size} 项成就${batchNextEnabled ? '启用' : '停用'}；操作原因写入审计日志。`}
    onclose={() => (batchOpen = false)}
  >
    <form
      method="POST"
      action="?/bulk"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update();
          if (result.type === 'success') {
            selected = new Set();
            batchOpen = false;
          }
        };
      }}
    >
      {#each [...selected] as c (c)}
        <input type="hidden" name="codes" value={c} />
      {/each}
      <input type="hidden" name="is_enabled" value={String(batchNextEnabled)} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="ach-batch-reason">操作原因（写审计）</label>
        <input id="ach-batch-reason" name="reason" class="input-field" required bind:value={batchReason} placeholder="必填" />
      </div>
      <Button text="确认{batchNextEnabled ? '启用' : '停用'} {selected.size} 项" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

{/if}
