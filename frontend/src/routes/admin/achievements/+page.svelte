<script lang="ts">
  // GAP-FIX（管理域·成就管理）：目录表格（code/名称/分类/条件/奖励/状态/解锁数）
  // + 新建成就表单 + 行操作（启停 If-Match / 手工授予 Dialog / 删除 DangerConfirm）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import StatCard from '$lib/components/admin/StatCard.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { show as showToast } from '$lib/ui/toast';
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

  /** 手工授予对话框（表单在 Dialog 内，成功后关闭）。 */
  let grantTarget: AdminAchievementItem | null = $state(null);
  let grantCode = $state('');
  let grantUsername = $state('');
  let grantReason = $state('');

  function openGrant(item: AdminAchievementItem): void {
    grantTarget = item;
    grantCode = item.code;
    grantUsername = '';
    grantReason = '';
  }

  /** 删除确认（DangerConfirm + 隐藏表单 requestSubmit）。 */
  let deleteTarget: AdminAchievementItem | null = $state(null);
  let deleteReason = $state('');
  let deleteForm: HTMLFormElement | undefined = $state();

  function openDelete(item: AdminAchievementItem): void {
    deleteTarget = item;
    deleteReason = '';
  }

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

  // 批量选择（M17-GAPFIX-07：原型批量条 同构）。
  let selected = $state(new Set<string>());
  function toggleSelect(code: string): void {
    const next = new Set(selected);
    if (next.has(code)) next.delete(code);
    else next.add(code);
    selected = next;
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
  {#if message}
    <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
  {/if}
  {#if conflict}
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
            if (result.type === 'success') {
              const payload = result.data as { message?: string } | undefined;
              await update();
              showToast(payload?.message ?? '创建成功', 'success');
            } else if (result.type === 'failure') {
              const payload = result.data as { message?: string } | undefined;
              await update();
              showToast(payload?.message ?? '创建失败', 'danger');
            } else {
              await update();
            }
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
            <label class="input-label" for="ac-reward-exp">奖励 EXP</label>
            <input id="ac-reward-exp" name="reward_exp" type="number" min="0" step="1" class="input-field" value="0" />
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
        {#if selected.size > 0}
          <div class="app-notice" role="status" style="margin:12px 14px 0;display:flex;gap:10px;align-items:center;flex-wrap:wrap;">
            <b>{selected.size} 项已选中</b>
            <form method="POST" action="?/bulk" use:enhance style="display:inline-flex;gap:8px;align-items:center;flex-wrap:wrap;">
              {#each [...selected] as c (c)}
                <input type="hidden" name="codes" value={c} />
              {/each}
              <input type="hidden" name="is_enabled" value="true" />
              <input type="text" class="input-field" name="reason" placeholder="操作原因（审计）" required style="max-width:200px;" />
              <button type="submit" class="btn primary sm">批量启用</button>
            </form>
            <form method="POST" action="?/bulk" use:enhance style="display:inline-flex;gap:8px;align-items:center;flex-wrap:wrap;">
              {#each [...selected] as c (c)}
                <input type="hidden" name="codes" value={c} />
              {/each}
              <input type="hidden" name="is_enabled" value="false" />
              <input type="text" class="input-field" name="reason" placeholder="操作原因（审计）" required style="max-width:200px;" />
              <button type="submit" class="btn secondary sm">批量停用</button>
            </form>
          </div>
        {/if}
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
                    <span class="text-secondary" style="font-size:var(--text-sm);">EXP {item.reward_exp} · 币 {item.reward_coin}</span>
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
                    <div style="display:flex;flex-direction:column;gap:var(--space-2);min-width:240px;">
                      <form
                        method="POST"
                        action="?/toggle"
                        use:enhance={() => {
                          return async ({ result, update }) => {
                            if (result.type === 'success') {
                              const payload = result.data as { message?: string } | undefined;
                              await update();
                              showToast(payload?.message ?? '操作成功', 'success');
                            } else if (result.type === 'failure') {
                              const payload = result.data as { message?: string } | undefined;
                              await update();
                              showToast(payload?.message ?? '操作失败', 'danger');
                            } else {
                              await update();
                            }
                          };
                        }}
                        style="display:flex;gap:var(--space-1);flex-wrap:wrap;align-items:center;"
                      >
                        <input type="hidden" name="code" value={item.code} />
                        <input type="hidden" name="version" value={String(item.version)} />
                        <input type="hidden" name="is_enabled" value={item.is_enabled ? 'false' : 'true'} />
                        <input type="text" class="input-field" name="reason" placeholder="原因（审计）" required style="max-width:130px;" />
                        <Button text={item.is_enabled ? '停用' : '启用'} variant={item.is_enabled ? 'secondary' : 'primary'} size="sm" type="submit" />
                      </form>
                      <div style="display:flex;gap:var(--space-1);">
                        <Button text="手工授予" variant="secondary" size="sm" onclick={() => openGrant(item)} />
                        <Button text="删除" variant="danger" size="sm" onclick={() => openDelete(item)} />
                      </div>
                    </div>
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

  <!-- 手工授予：Dialog 内表单（username + reason → POST grant）。 -->
  <Dialog
    open={grantTarget !== null}
    title="手工授予成就"
    description={grantTarget ? `将「${grantTarget.name}」（${grantTarget.code}）授予指定成员；已解锁则幂等保持原解锁时间。` : ''}
    onclose={() => (grantTarget = null)}
  >
    <form
      method="POST"
      action="?/grant"
      use:enhance={() => {
        return async ({ result, update }) => {
          if (result.type === 'success') {
            const payload = result.data as { message?: string } | undefined;
            await update();
            showToast(payload?.message ?? '已授予', 'success');
          } else if (result.type === 'failure') {
            const payload = result.data as { message?: string } | undefined;
            await update();
            showToast(payload?.message ?? '授予失败', 'danger');
          } else {
            await update();
          }
          grantTarget = null;
        };
      }}
    >
      <input type="hidden" name="code" value={grantCode} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="grant-username">用户名</label>
        <input id="grant-username" name="username" class="input-field" required bind:value={grantUsername} placeholder="member_username" />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="grant-reason">授予原因（写审计）</label>
        <input id="grant-reason" name="reason" class="input-field" required bind:value={grantReason} placeholder="必填" />
      </div>
      <Button text="确认授予" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 删除确认：DangerConfirm + reason（写审计），确认后提交隐藏表单。 -->
  <form
    method="POST"
    action="?/delete"
    bind:this={deleteForm}
    use:enhance={() => {
      return async ({ result, update }) => {
        if (result.type === 'success') {
          const payload = result.data as { message?: string } | undefined;
          await update();
          showToast(payload?.message ?? '已删除', 'success');
        } else if (result.type === 'failure') {
          const payload = result.data as { message?: string } | undefined;
          await update();
          showToast(payload?.message ?? '删除失败', 'danger');
        } else {
          await update();
        }
        deleteTarget = null;
      };
    }}
  >
    <input type="hidden" name="code" value={deleteTarget?.code ?? ''} />
    <input type="hidden" name="reason" value={deleteReason} />
  </form>

  <DangerConfirm
    open={deleteTarget !== null}
    title="删除成就"
    description={deleteTarget ? `确认删除「${deleteTarget.name}」（${deleteTarget.code}）？将级联删除全部解锁记录，不可恢复。` : ''}
    confirmText="确认删除"
    oncancel={() => (deleteTarget = null)}
    onconfirm={() => deleteForm?.requestSubmit()}
  >
    <label class="input-label" for="ach-del-reason">删除原因（写审计）</label>
    <input id="ach-del-reason" class="input-field" bind:value={deleteReason} placeholder="必填" required />
  </DangerConfirm>
{/if}
