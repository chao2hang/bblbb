<script lang="ts">
  // M13-UI-01/ADMIN-02：管理用户页——列表 + 状态更新（If-Match + reason）。
  // 原型对齐：prototype/pages/admin-users.html
  // - .app-card > .app-card__head + .app-card__body
  // - .app-toolbar 工具条（搜索框 + 计数）
  // - .app-table-wrap > .app-table 数据表格
  // M18-ADMIN-DIALOG：写操作弹层化——行内「状态下拉+原因+保存」裸表单改为
  // 行操作入口 → Dialog（status 预选当前值 + reason 必填，保留 If-Match
  // version 隐藏字段）；选择列接入 BatchBar + 「批量设置状态」批量 Dialog →
  // ?/batchUpdate（服务端循环 PATCH /api/v1/admin/users/{id}，versions 与 ids
  // 一一对应作为 If-Match）。「详情」「调整积分」仍为 GET 链接。
  // M18-ADMIN-OPS（约定 D）：行内写操作入口改为「⋮」三点菜单——单项
  // 「设置用户状态」打开既有状态 Dialog（信任等级入口保持原按钮不动）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { enhance } from '$app/forms';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminUsersPageData, AdminUsersActionData, AdminUserItem } from './+page.server';

  let { data, form }: { data: AdminUsersPageData; form?: AdminUsersActionData | null } = $props();


  /** 状态选项（与 ?/update / ?/batchUpdate 服务端白名单一致）。 */
  const STATUS_OPTIONS: { value: string; label: string }[] = [
    { value: 'active', label: '正常 (active)' },
    { value: 'restricted', label: '受限 (restricted)' },
    { value: 'banned', label: '封禁 (banned)' },
    { value: 'pending', label: '待审 (pending)' }
  ];

  const loadState = $derived(data.state);
  const items = $derived(data.items ?? []);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const conflict = $derived(form?.conflict === true);

  // —— step-up 重新验证（M02-MFA-07）：高敏操作命中 403 step_up_required 时展示 ——
  let reauthLoading = $state(false);
  let reauthCancelled = $state(false);
  let reauthError = $state<string | null>(null);

  $effect(() => {
    if (form?.stepUpRequired) reauthCancelled = false;
  });

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  function getUrlParam(key: string): string {
    try {
      return page.url.searchParams.get(key) ?? '';
    } catch {
      return '';
    }
  }

  let searchQ = $state(untrack(() => getUrlParam('q')));
  let statusFilter = $state(untrack(() => getUrlParam('status')));
  let selectedIds = $state<string[]>([]);

  const filteredItems = $derived.by(() => {
    let list = items; // 只展示服务端返回的数据；空列表表达真实空态（P0：禁止 Mock 兜底）
    if (searchQ.trim()) {
      const q = searchQ.trim().toLowerCase();
      list = list.filter(
        (u) =>
          (u.username ?? '').toLowerCase().includes(q) ||
          (u.display_name && (u.display_name ?? '').toLowerCase().includes(q)) ||
          (u.email ?? '').toLowerCase().includes(q)
      );
    }
    if (statusFilter) {
      list = list.filter((u) => u.status === statusFilter);
    }
    return list;
  });

  let allSelected = $derived(
    filteredItems.length > 0 && selectedIds.length === filteredItems.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = filteredItems.map((u) => u.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  /** 单行状态弹层（一个 Dialog 服务一类操作，target 区分行）。 */
  let statusTarget: AdminUserItem | null = $state(null);
  let statusDraft = $state('active');
  let statusReason = $state('');

  function openStatus(item: AdminUserItem): void {
    statusTarget = item;
    statusDraft = item.status;
    statusReason = '';
  }

  let isSubmitting = $state(false);

  function openBatchBan(): void {
    batchStatus = 'banned';
    batchReason = '违规批量封禁';
    batchStatusOpen = true;
  }

  function openBatchActive(): void {
    batchStatus = 'active';
    batchReason = '批量恢复正常状态';
    batchStatusOpen = true;
  }

  /** 一键随机昵称弹层 */
  let randomizeTarget: AdminUserItem | null = $state(null);
  let randomizeReason = $state('管理员一键随机重置违规昵称');

  function openRandomize(item: AdminUserItem): void {
    randomizeTarget = item;
    randomizeReason = '管理员一键随机重置违规昵称';
  }

  /** 昵称黑名单弹层 */
  let blacklistOpen = $state(false);
  let blacklistSearch = $state('');
  let newBlacklistName = $state('');
  let newBlacklistReason = $state('');

  const blacklistItems = $derived(data.blacklist ?? []);
  const filteredBlacklist = $derived.by(() => {
    if (!blacklistSearch.trim()) return blacklistItems;
    const q = blacklistSearch.trim().toLowerCase();
    return blacklistItems.filter(
      (b) =>
        b.nickname.toLowerCase().includes(q) ||
        (b.reason && b.reason.toLowerCase().includes(q))
    );
  });

  /** 行「⋯」菜单项（约定 D）：设置用户状态 / 一键随机昵称 / 信任等级 / 详情 / 调整积分（GET 导航收进菜单）。 */
  function rowActions(item: AdminUserItem) {
    return [
      {
        label: '设置用户状态',
        run: () => openStatus(item)
      },
      {
        label: '一键随机昵称',
        run: () => openRandomize(item)
      },
      { label: '信任', run: () => openTrust(item) },
      { label: '详情', run: () => goto(`/users/${encodeURIComponent(item.username)}`) },
      { label: '调整积分', run: () => goto(`/admin/points?username=${encodeURIComponent(item.username)}`) }
    ];
  }

  /** M20-TRUST：单行信任等级弹层（TL4 唯一授予入口；原因写审计）。 */
  const TRUST_OPTIONS = [
    { value: 0, label: 'TL0 新用户' },
    { value: 1, label: 'TL1 基本用户' },
    { value: 2, label: 'TL2 成员' },
    { value: 3, label: 'TL3 活跃用户' },
    { value: 4, label: 'TL4 领导者（手动授予）' }
  ] as const;
  let trustTarget: AdminUserItem | null = $state(null);
  let trustDraft = $state(0);
  let trustReason = $state('');

  function openTrust(item: AdminUserItem): void {
    trustTarget = item;
    trustDraft = item.trust_level ?? 0;
    trustReason = '';
  }

  /** 批量设置状态弹层（选中行共享同一目标状态）。 */
  let batchStatusOpen = $state(false);
  let batchStatus = $state('active');
  let batchReason = $state('');

  function openBatchStatus(): void {
    batchStatus = 'active';
    batchReason = '';
    batchStatusOpen = true;
  }

  /** 选中行的乐观锁版本列表（与 selectedIds 顺序一一对应，作为 If-Match）。 */
  const selectedVersions = $derived(
    selectedIds.map((id) => {
      const item = filteredItems.find((u) => u.id === id);
      return item ? String(item.version) : '';
    })
  );

  function statusBadgeCls(status: string): string {
    switch (status) {
      case 'active':
        return 'sb-brand';
      case 'banned':
        return 'sb-danger';
      case 'pending':
        return 'sb-hot';
      default:
        return 'sb-gray';
    }
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'active':
        return '正常';
      case 'banned':
        return '封禁';
      case 'pending':
        return '待验证';
      case 'restricted':
        return '受限';
      default:
        return status;
    }
  }

  /** 最近活动相对文案（原型风格：今天 HH:MM / N 天前；从未登录 = —）。 */
  function lastActiveLabel(ts: number | null | undefined): string {
    if (!ts) return '—';
    const diff = Date.now() - ts;
    const dayMs = 86_400_000;
    const days = Math.floor(diff / dayMs);
    if (diff < 0 || days < 1) {
      return `今天 ${new Date(ts).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false })}`;
    }
    if (days === 1) return '昨天';
    if (days < 30) return `${days} 天前`;
    return new Date(ts).toLocaleDateString('zh-CN');
  }

</script>

<svelte:head>
  <title>用户管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="用户管理" />

<section class="app-card">
  <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
    <h2>成员列表</h2>
    <Button
      text={`昵称黑名单 (${data.blacklistTotal ?? 0})`}
      variant="secondary"
      size="sm"
      onclick={() => (blacklistOpen = true)}
    />
  </header>

  <div class="app-card__body">
    {#if loadState === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if loadState === 'not_implemented'}
      <p class="input-hint" role="note">用户管理接口开发中。</p>
    {:else if loadState === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if loadState === 'ok'}
      {#if message && !hasJs}
        <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
      {/if}
      {#if conflict && !hasJs}
        <p class="input-hint is-error" role="alert">用户版本已变化，请刷新后重试（If-Match 乐观锁）。</p>
      {/if}

      {#if items.length === 0 && !searchQ && !statusFilter}
        <p class="input-hint">暂无用户数据。</p>
      {:else}
        <!-- Discourse 式筛选：输入完成后再提交，避免每个字符触发一次 SPA 导航。 -->
        <form method="GET" action="/admin/users" class="admin-filter-bar">
          <input
            type="search"
            name="q"
            bind:value={searchQ}
            class="app-field"
            placeholder="搜索用户名、显示名或邮箱"
            aria-label="按用户名过滤"
          />
          <select class="app-select" name="status" bind:value={statusFilter} aria-label="状态筛选">
            <option value="">全部状态</option>
            <option value="active">正常</option>
            <option value="banned">封禁</option>
            <option value="pending">待验证</option>
            <option value="restricted">受限</option>
          </select>
          <span class="admin-filter-bar__summary">共 {filteredItems.length} 名成员</span>
          {#if searchQ || statusFilter}
            <a class="btn ghost sm" href="/admin/users">清除筛选</a>
          {/if}
          <button type="submit" class="btn secondary sm">
            <Icon name="search" size={14} />
            应用筛选
          </button>
        </form>

        <!-- 批量工具条（选中 > 0 时渲染；批量参数在 Dialog 内填写） -->
        <BatchBar count={selectedIds.length} noun="名成员" onclear={() => (selectedIds = [])}>
          <Button text="批量设置状态" variant="secondary" size="sm" onclick={openBatchStatus} />
          <Button text="一键封禁" variant="danger" size="sm" onclick={openBatchBan} />
          <Button text="一键激活" variant="ghost" size="sm" onclick={openBatchActive} />
        </BatchBar>

        <div class="app-table-wrap">
          <table class="app-table" aria-label="用户列表">
            <thead>
              <tr>
                <th style="width:40px;text-align:center;">
                  <input
                    type="checkbox"
                    checked={allSelected}
                    onchange={toggleAll}
                    aria-label="全选当前列表"
                  />
                </th>
                <th>用户</th>
                <th>等级</th>
                <th>B币</th>
                <th>邮箱</th>
                <th>状态</th>
                <th>角色</th>
                <th>最近活动</th>
                <th style="min-width:220px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#if filteredItems.length === 0}
                <tr>
                  <td colspan="9" style="text-align:center;padding:32px;color:var(--color-text-secondary);">
                    未找到匹配的用户（无结果）
                  </td>
                </tr>
              {:else}
                {#each filteredItems as item (item.id)}
                <tr>
                  <td style="text-align:center;">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      onchange={() => toggleRow(item.id)}
                      aria-label="选择 {item.username}"
                    />
                  </td>
                  <td>
                    <div style="display:flex;align-items:center;gap:10px;">
                      <Avatar name={item.display_name || item.username} size="sm" seed={item.username ?? item.id} />
                      <div>
                        <a class="text-link" href="/users/{item.username}" style="font-weight:600;">
                          {item.display_name || item.username}
                        </a>
                      </div>
                    </div>
                  </td>
                  <td>
                    <span class="lvbadge" title="社区信任等级">TL{item.trust_level ?? item.level ?? 0}</span>
                  </td>
                  <td><span style="font-size:12px;white-space:nowrap;">{(item.coin_balance ?? 0).toLocaleString('zh-CN')}</span></td>
                  <td>
                    <span class="text-secondary" style="font-size:13px;">{item.email}</span>
                  </td>
                  <td>
                    <span class="sbadge {statusBadgeCls(item.status)}">
                      {statusLabel(item.status)}
                    </span>
                    <span style="display:none;">{item.status}</span>
                  </td>
                  <td>
                    <span class="text-secondary" style="font-size:13px;">
                      {item.roles.join('、') || 'member'}
                    </span>
                  </td>
                  <td>
                    <span class="text-secondary" style="font-size:12px;white-space:nowrap;">{lastActiveLabel(item.last_login_at)}</span>
                  </td>
                  <td>
                    <div style="display:flex;gap:6px;flex-wrap:wrap;align-items:center;">
                      <!-- 行内操作有且只有「⋯」菜单（约定 D）：状态/信任/详情/调整积分均在菜单内 -->
                      <RowActionsMenu
                        label="更多操作：用户 {item.username}"
                        actions={rowActions(item)}
                      />
                    </div>
                  </td>
                </tr>
              {/each}
            {/if}
          </tbody>
          </table>
        </div>

        <!-- M18：对齐原型底部导出卡片按钮 -->
        <footer class="app-card__foot" style="margin-top:14px;">
          <ExportButton
            label="导出用户 CSV"
            filename="admin-users"
            columns={[
              { key: 'username', label: '用户名' },
              { key: 'email', label: '邮箱' },
              { key: 'level', label: '等级' },
              { key: 'coin', label: 'B币' },
              { key: 'roles', label: '角色' },
              { key: 'status', label: '状态' },
              { key: 'last', label: '最近活动' }
            ]}
            getData={() =>
              filteredItems.map((item) => ({
                username: item.username,
                email: item.email,
                level: item.level,
                coin: item.coin_balance ?? 0,
                roles: item.roles.join('|'),
                status: statusLabel(item.status),
                last: lastActiveLabel(item.last_login_at)
              }))}
          />
        </footer>
      {/if}
    {/if}
  </div>
</section>

<!-- 单行状态 Dialog：id/version（If-Match）隐藏字段 + status 预选当前值 + reason 必填 -->
<Dialog
  open={statusTarget !== null}
  title="设置用户状态"
  description={statusTarget
    ? `将调整「${statusTarget.display_name || statusTarget.username}」的状态；原因写入审计日志。`
    : ''}
  onclose={() => (statusTarget = null)}
>
  <form
    method="POST"
    action="?/update"
    use:enhance={() => {
      isSubmitting = true;
      return async ({ result, update }) => {
        isSubmitting = false;
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          statusTarget = null;
        }
      };
    }}
  >
    <input type="hidden" name="id" value={statusTarget?.id ?? ''} />
    <input type="hidden" name="version" value={statusTarget ? String(statusTarget.version) : ''} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="user-status-select">状态</label>
      <select id="user-status-select" name="status" class="input-field" bind:value={statusDraft}>
        {#each STATUS_OPTIONS as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="user-status-reason">操作原因（写审计）</label>
      <input
        id="user-status-reason"
        name="reason"
        class="input-field"
        required
        bind:value={statusReason}
        placeholder="必填"
      />
    </div>
    <Button text={isSubmitting ? '保存中...' : '保存'} variant="primary" size="sm" type="submit" disabled={isSubmitting} />
  </form>
</Dialog>

<!-- M20-TRUST 单行信任等级 Dialog：level 0–4 + reason 必填（TL4 唯一授予入口） -->
<Dialog
  open={trustTarget !== null}
  title="设置信任等级"
  description={trustTarget
    ? `将调整「${trustTarget.display_name || trustTarget.username}」的信任等级（TL0–TL4 行为信任标准）；原因写入审计日志。`
    : ''}
  onclose={() => (trustTarget = null)}
>
  <form
    method="POST"
    action="?/setTrust"
    use:enhance={() => {
      isSubmitting = true;
      return async ({ result, update }) => {
        isSubmitting = false;
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          trustTarget = null;
        }
      };
    }}
  >
    <input type="hidden" name="id" value={trustTarget?.id ?? ''} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="user-trust-select">信任等级</label>
      <select id="user-trust-select" name="level" class="input-field" bind:value={trustDraft}>
        {#each TRUST_OPTIONS as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="user-trust-reason">操作原因（写审计）</label>
      <input
        id="user-trust-reason"
        name="reason"
        class="input-field"
        required
        bind:value={trustReason}
        placeholder="必填"
      />
    </div>
    <Button text={isSubmitting ? '保存中...' : '保存'} variant="primary" size="sm" type="submit" disabled={isSubmitting} />
  </form>
</Dialog>

<!-- 批量设置状态 Dialog：ids/versions 一一对应（服务端逐条带 If-Match）+ reason 必填 -->
<Dialog
  open={batchStatusOpen}
  title="批量设置状态"
  description={`将更新 ${selectedIds.length} 名成员的状态；原因写审计，逐条按乐观锁版本提交。`}
  onclose={() => (batchStatusOpen = false)}
>
  <form
    method="POST"
    action="?/batchUpdate"
    use:enhance={() => {
      isSubmitting = true;
      return async ({ result, update }) => {
        isSubmitting = false;
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedIds = [];
          batchStatusOpen = false;
        }
      };
    }}
  >
    <input type="hidden" name="ids" value={selectedIds.join(',')} />
    <input type="hidden" name="versions" value={selectedVersions.join(',')} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="user-batch-status">目标状态</label>
      <select id="user-batch-status" name="status" class="input-field" bind:value={batchStatus}>
        {#each STATUS_OPTIONS as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="user-batch-reason">操作原因（写审计）</label>
      <input
        id="user-batch-reason"
        name="reason"
        class="input-field"
        required
        bind:value={batchReason}
        placeholder="必填"
      />
    </div>
    <Button text={isSubmitting ? '更新中...' : '确认更新'} variant="primary" size="sm" type="submit" disabled={isSubmitting} />
  </form>
</Dialog>

<!-- 一键随机昵称 Dialog -->
<Dialog
  open={randomizeTarget !== null}
  title="一键随机用户昵称"
  description={randomizeTarget
    ? `将为「${randomizeTarget.display_name || randomizeTarget.username}」生成合规的随机昵称。原昵称将自动存入黑名单，后续无法再被任何用户创建或使用。`
    : ''}
  onclose={() => (randomizeTarget = null)}
>
  <form
    method="POST"
    action="?/randomizeNickname"
    use:enhance={() => {
      isSubmitting = true;
      return async ({ result, update }) => {
        isSubmitting = false;
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          randomizeTarget = null;
        }
      };
    }}
  >
    <input type="hidden" name="id" value={randomizeTarget?.id ?? ''} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="randomize-user-reason">操作原因（写审计）</label>
      <input
        id="randomize-user-reason"
        name="reason"
        class="input-field"
        required
        bind:value={randomizeReason}
        placeholder="必填"
      />
    </div>
    <div style="display:flex;gap:8px;justify-content:flex-end;">
      <Button text="取消" variant="secondary" size="sm" type="button" onclick={() => (randomizeTarget = null)} disabled={isSubmitting} />
      <Button text={isSubmitting ? '处理中...' : '确认随机并加入黑名单'} variant="danger" size="sm" type="submit" disabled={isSubmitting} />
    </div>
  </form>
</Dialog>

<!-- 昵称黑名单 Dialog -->
<Dialog
  open={blacklistOpen}
  title="昵称黑名单"
  description="处于黑名单中的名称无法再被用户用作昵称或用户名注册，有效避免不规范名称复现。"
  onclose={() => (blacklistOpen = false)}
>
  <div style="display:flex;flex-direction:column;gap:14px;max-height:65vh;overflow-y:auto;">
    <!-- 手动添加黑名单 -->
    <form
      method="POST"
      action="?/addBlacklist"
      style="display:flex;gap:8px;align-items:flex-end;flex-wrap:wrap;padding:12px;background:var(--color-bg-subtle, #f5f5f5);border-radius:6px;"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update();
          newBlacklistName = '';
          newBlacklistReason = '';
        };
      }}
    >
      <div style="flex:1;min-width:140px;">
        <label class="input-label" for="bl-new-name" style="font-size:12px;">违规昵称</label>
        <input id="bl-new-name" name="nickname" class="input-field" required bind:value={newBlacklistName} placeholder="输入要封禁的昵称" />
      </div>
      <div style="flex:1;min-width:140px;">
        <label class="input-label" for="bl-new-reason" style="font-size:12px;">封禁原因</label>
        <input id="bl-new-reason" name="reason" class="input-field" bind:value={newBlacklistReason} placeholder="可选原因" />
      </div>
      <Button text="加入黑名单" variant="secondary" size="sm" type="submit" />
    </form>

    <!-- 搜索 -->
    <div>
      <input
        type="search"
        class="input-field"
        placeholder="搜索黑名单昵称或原因..."
        bind:value={blacklistSearch}
      />
    </div>

    <!-- 列表 -->
    {#if filteredBlacklist.length === 0}
      <p class="input-hint" style="text-align:center;padding:16px;">
        {blacklistItems.length === 0 ? '暂无黑名单条目。' : '未找到匹配的黑名单条目。'}
      </p>
    {:else}
      <div style="display:flex;flex-direction:column;gap:8px;">
        {#each filteredBlacklist as item (item.id)}
          <div style="display:flex;justify-content:space-between;align-items:center;padding:8px 12px;border:1px solid var(--color-border, #e5e5e5);border-radius:6px;">
            <div>
              <span style="font-weight:600;font-size:14px;">{item.nickname}</span>
              {#if item.reason}
                <span class="text-secondary" style="font-size:12px;margin-left:8px;">({item.reason})</span>
              {/if}
              <div class="text-secondary" style="font-size:11px;margin-top:2px;">
                {new Date(item.created_at).toLocaleDateString('zh-CN')} {new Date(item.created_at).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}
              </div>
            </div>
            <form
              method="POST"
              action="?/deleteBlacklist"
              use:enhance={() => {
                return async ({ result, update }) => {
                  toastActionResult(result);
                  await update();
                };
              }}
            >
              <input type="hidden" name="id" value={item.id} />
              <Button text="移出" variant="ghost" size="sm" type="submit" />
            </form>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</Dialog>

<!-- step-up 重新验证（M02-MFA-07）：高敏操作命中 403 step_up_required 时展示。
     无 JS 时 Dialog 以固定层内联渲染，表单仍可用（SSR 基线保留）。 -->
<Dialog
  open={Boolean(form?.stepUpRequired) && !reauthCancelled}
  title="需要重新验证身份"
  description="用户修改属于高风险管理操作，要求近期重新认证。输入当前账号密码完成重新验证后，可继续刚才的操作。"
  onclose={() => (reauthCancelled = true)}
>
  {#if reauthError}
    <div class="alert alert-danger" role="alert" style="margin-bottom:10px;padding:8px 12px;font-size:12px;">
      {reauthError}
    </div>
  {/if}
  <form
    method="POST"
    action="?/reauth"
    use:enhance={() => {
      reauthLoading = true;
      reauthError = null;
      return async ({ result, update }) => {
        reauthLoading = false;
        if (result.type === 'failure') {
          reauthError = (result.data as unknown as AdminUsersActionData | null)?.message ?? '密码验证失败，请重试';
          return;
        }
        toastActionResult(result);
        await update();
      };
    }}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <div>
      <label class="input-label" for="user-reauth-password">当前账号密码</label>
      <input class="input-field" type="password" id="user-reauth-password" name="password" autocomplete="current-password" required />
    </div>
    <div style="display:flex;gap:8px;">
      <Button text={reauthLoading ? '验证中…' : '重新验证'} variant="primary" type="submit" disabled={reauthLoading} />
      <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
    </div>
  </form>
</Dialog>
