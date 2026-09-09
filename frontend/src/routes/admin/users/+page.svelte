<script lang="ts">
  // M13-UI-01/ADMIN-02：管理用户页——列表 + 状态更新（If-Match + reason）。
  // 原型对齐：prototype/pages/admin-users.html
  // - .app-card > .app-card__head + .app-card__body
  // - .app-toolbar 工具条（搜索框 + 计数）
  // - .app-table-wrap > .app-table 数据表格
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { enhance } from '$app/forms';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import { withActionToast } from '$lib/ui/action-toast';
  import type { AdminUsersPageData, AdminUsersActionData, AdminUserItem } from './+page.server';

  let { data, form }: { data: AdminUsersPageData; form?: AdminUsersActionData | null } = $props();

  const mockUsers: AdminUserItem[] = [
    {
      id: 'u-1',
      username: 'admin',
      display_name: '超级管理员',
      email: 'admin@bblbb.local',
      email_verified: true,
      status: 'active',
      level: 4,
      roles: ['admin', 'moderator'],
      coin_balance: 10000,
      exp_balance: 8500,
      created_at: 1700000000000,
      updated_at: 1700000000000,
      last_login_at: 1700000000000,
      version: 1
    },
    {
      id: 'u-2',
      username: 'Alice',
      display_name: '爱丽丝',
      email: 'alice@example.com',
      email_verified: true,
      status: 'active',
      level: 3,
      roles: ['member'],
      coin_balance: 520,
      exp_balance: 1200,
      created_at: 1699000000000,
      updated_at: 1699000000000,
      last_login_at: 1699000000000,
      version: 1
    },
    {
      id: 'u-3',
      username: 'Bob',
      display_name: '鲍勃',
      email: 'bob@example.com',
      email_verified: false,
      status: 'pending',
      level: 1,
      roles: ['member'],
      coin_balance: 50,
      exp_balance: 120,
      created_at: 1698000000000,
      updated_at: 1698000000000,
      last_login_at: 1698000000000,
      version: 1
    },
    {
      id: 'u-4',
      username: 'Charlie',
      display_name: '查理',
      email: 'charlie@example.com',
      email_verified: true,
      status: 'banned',
      level: 0,
      roles: ['member'],
      coin_balance: 0,
      exp_balance: 10,
      created_at: 1697000000000,
      updated_at: 1697000000000,
      last_login_at: null,
      version: 1
    }
  ];

  const loadState = $derived(data.state);
  const items = $derived(data.items ?? []);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const conflict = $derived(form?.conflict === true);

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

  function updateFilterParams(qVal: string, statusVal: string) {
    try {
      const url = new URL(page.url);
      if (qVal.trim()) url.searchParams.set('q', qVal.trim());
      else url.searchParams.delete('q');
      if (statusVal) url.searchParams.set('status', statusVal);
      else url.searchParams.delete('status');
      goto(url.toString(), { replaceState: true, keepFocus: true, noScroll: true });
    } catch {}
  }

  const filteredItems = $derived.by(() => {
    let list = items.length > 0 ? items : (data.items === null ? mockUsers : items);
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
  <header class="app-card__head">
    <h2>成员列表</h2>
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
        <!-- M18：原型对齐工具栏（按用户名过滤 + 全部状态 + 数量） -->
        <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
          <input
            type="search"
            bind:value={searchQ}
            oninput={() => updateFilterParams(searchQ, statusFilter)}
            class="app-field"
            placeholder="按用户名过滤"
            aria-label="按用户名过滤"
          />
          <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;">
            <span class="app-muted" style="font-size:var(--text-xs);">共 {filteredItems.length} 名成员</span>
            <select
              class="app-select"
              bind:value={statusFilter}
              onchange={() => updateFilterParams(searchQ, statusFilter)}
              aria-label="状态筛选"
              style="min-width:140px;"
            >
              <option value="">全部状态</option>
              <option value="active">正常</option>
              <option value="banned">封禁</option>
              <option value="pending">待验证</option>
              <option value="restricted">受限</option>
            </select>
          </div>
        </div>

        {#if selectedIds.length > 0}
          <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
            <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
            <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
          </div>
        {/if}

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
                <th>经验</th>
                <th>邮箱</th>
                <th>状态</th>
                <th>角色</th>
                <th>最近活动</th>
                <th style="min-width:280px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#if filteredItems.length === 0}
                <tr>
                  <td colspan="10" style="text-align:center;padding:32px;color:var(--color-text-secondary);">
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
                      aria-label="选择此项"
                    />
                  </td>
                  <td>
                    <div style="display:flex;align-items:center;gap:10px;">
                      <Avatar name={item.display_name || item.username} size="sm" />
                      <div>
                        <a class="text-link" href="/users/{item.username}" style="font-weight:600;">
                          {item.display_name || item.username}
                        </a>
                      </div>
                    </div>
                  </td>
                  <td><span class="lvbadge">LV.{item.level}</span></td>
                  <td><span style="font-size:12px;white-space:nowrap;">{(item.coin_balance ?? 0).toLocaleString('zh-CN')}</span></td>
                  <td><span class="text-secondary" style="font-size:12px;white-space:nowrap;">{(item.exp_balance ?? 0).toLocaleString('zh-CN')}</span></td>
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
                    <div style="display:flex;gap:6px;margin-bottom:6px;">
                      <a class="btn ghost sm" href="/users/{item.username}">详情</a>
                      <a class="btn ghost sm" href="/admin/points?username={item.username}">调整积分</a>
                    </div>
                    <form
                      method="POST"
                      action="?/update"
                      use:enhance={withActionToast()}
                      style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;"
                    >
                      <input type="hidden" name="id" value={item.id} />
                      <input type="hidden" name="version" value={String(item.version)} />
                      <select
                        class="input-field"
                        name="status"
                        aria-label="状态"
                        style="height:32px;padding:0 8px;font-size:12px;width:auto;"
                      >
                        <option value="active" selected={item.status === 'active'}>正常 (active)</option>
                        <option value="restricted" selected={item.status === 'restricted'}>受限 (restricted)</option>
                        <option value="banned" selected={item.status === 'banned'}>封禁 (banned)</option>
                        <option value="pending" selected={item.status === 'pending'}>待审 (pending)</option>
                      </select>
                      <input
                        type="text"
                        class="input-field"
                        name="reason"
                        placeholder="原因（审计）"
                        required
                        style="height:32px;padding:0 8px;font-size:12px;width:130px;"
                      />
                      <button type="submit" class="btn primary sm" style="height:32px;padding:0 12px;font-size:12px;">
                        保存
                      </button>
                    </form>
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
              { key: 'exp', label: '经验' },
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
                exp: item.exp_balance ?? 0,
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
