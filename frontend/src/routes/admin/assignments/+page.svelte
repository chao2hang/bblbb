<script lang="ts">
  // 角色委派管理页——用户搜索 + 角色授予/撤销（完整 CRUD）。
  // 约定 A（按钮→弹层）：授予 = 按钮 + Dialog（?/grant）；撤销 = 行内写操作入口
  // + DangerConfirm（?/revoke，原因必填写审计）。
  // 约定 B（批量）：后端有单条 revoke 端点（DELETE /admin/users/{id}/roles/{role}），
  // 当前角色表提供选择列 + BatchBar「批量撤销」→ ?/batchRevoke 循环单条端点。
  // 约定 D：行内撤销入口改为「⋮」三点菜单——单项「撤销角色」打开既有撤销
  // DangerConfirm（隐藏表单 requestSubmit 机制不变）。
  import { enhance } from '$app/forms';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminAssignmentsActionData, AdminAssignmentsPageData } from './+page.server';

  let { data, form }: { data: AdminAssignmentsPageData; form?: AdminAssignmentsActionData | null } =
    $props();

  const roleState = $derived(data.loadState);
  const q = $derived(data.q);
  const selectedUser = $derived(form?.user ?? data.selectedUser);
  const userError = $derived(data.userError);
  const message = $derived(form?.message ?? null);

  /** 角色 chip 本地化（后端返回英文角色名，未收录时原样显示）。 */
  const ROLE_LABELS: Record<string, string> = {
    administrator: '管理员',
    global_moderator: '全站版主',
    board_moderator: '板块版主',
    member: '成员'
  };
  function roleLabel(name: string): string {
    return ROLE_LABELS[name] ?? name;
  }

  const STATUS_LABELS: Record<string, string> = {
    pending: '待激活',
    active: '正常',
    restricted: '受限',
    banned: '已封禁'
  };
  function statusLabel(status: string): string {
    return STATUS_LABELS[status] ?? status;
  }

  // JS 启用：动作结果走全局 Toast 浮窗；顶部内联横幅仅保留为无 JS 回退。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 授予 Dialog：所选角色（默认跟随可授予角色目录第一个）。
  let grantOpen = $state(false);
  let grantRole = $state('');
  $effect(() => {
    if (roleState.state === 'ok' && roleState.items.length > 0 && !grantRole) {
      grantRole = roleState.items[0].name;
    }
  });

  // 撤销确认（DangerConfirm + 隐藏表单 requestSubmit）。
  let isSubmitting = $state(false);
  let revokeTarget: string | null = $state(null);
  let revokeReason = $state('');
  let revokeError = $state('');
  let revokeForm: HTMLFormElement | undefined = $state();

  function openRevoke(role: string): void {
    revokeTarget = role;
    revokeReason = '';
    revokeError = '';
  }

  /** 行「⋮」菜单项（约定 D：单一「撤销角色」动作 → 既有撤销 DangerConfirm 流）。 */
  function rowActions(role: string) {
    return [
      {
        label: '撤销角色',
        danger: true,
        run: () => openRevoke(role)
      }
    ];
  }

  // 批量选择（约定 B：当前角色表选择列 + BatchBar 批量撤销）。
  let selectedRoles = $state(new Set<string>());
  function toggleSelectRole(role: string): void {
    const next = new Set(selectedRoles);
    if (next.has(role)) next.delete(role);
    else next.add(role);
    selectedRoles = next;
  }
  const allRolesSelected = $derived(
    !!selectedUser && selectedUser.roles.length > 0 && selectedUser.roles.every((r) => selectedRoles.has(r))
  );
  function toggleSelectAllRoles(): void {
    if (!selectedUser) return;
    if (allRolesSelected) selectedRoles = new Set();
    else selectedRoles = new Set(selectedUser.roles);
  }
  // 批量撤销 Dialog。
  let batchRevokeOpen = $state(false);
  // 切换选中用户后清空批量选择（旧用户的角色集合不再适用）。
  $effect(() => {
    void selectedUser?.id;
    selectedRoles = new Set();
  });

</script>

<svelte:head>
  <title>角色委派 — BBLBB</title>
</svelte:head>

<PageHeader title="角色委派" />

<p class="input-hint" style="margin-bottom:12px;">
  在这里把角色授予指定用户：所有授予与撤销都会写入审计日志；用户的实际权限始终以服务端裁决为准。
</p>

{#if roleState.state === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    </div>
  </div>
{:else if roleState.state === 'not_implemented'}
  <div class="app-card">
    <div class="app-card__body">
      <p class="input-hint" role="note">角色委派接口开发中。</p>
    </div>
  </div>
{:else if roleState.state === 'error'}
  <div class="app-card">
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert">{roleState.message || adminStateLabel('error')}</p>
    </div>
  </div>
{:else if roleState.state === 'ok'}
  {#if message && !hasJs}
    <div class="app-notice" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
      {message}
    </div>
  {/if}

  <div class="app-card">
    <div class="app-card__head"><h2>可授予角色</h2></div>
    <div class="app-card__body">
      <ul style="list-style:none;margin:0;padding:0;display:flex;flex-wrap:wrap;gap:var(--space-2);">
        {#each roleState.items as item (item.id)}
          <li class="tag-chip"><Icon name="shield-check" size={12} /><span>{roleLabel(item.name)}</span></li>
        {/each}
      </ul>
    </div>
  </div>

  <!-- 用户搜索（GET 表单 → ?q=，load 层查询） -->
  <div class="app-card" style="margin-top:16px;">
    <div class="app-card__head"><h2>查找用户</h2></div>
    <div class="app-card__body">
      <form method="GET" action="/admin/assignments" style="display:flex;gap:10px;align-items:flex-end;flex-wrap:wrap;">
        <div style="flex:1;min-width:220px;">
          <label class="input-label" for="asg-q">用户名 / 昵称 / 邮箱关键字</label>
          <input class="input-field" type="search" id="asg-q" name="q" value={q} placeholder="如：alice" />
        </div>
        <button type="submit" class="btn primary sm">搜索用户</button>
      </form>
      {#if userError}
        <p class="input-hint is-error" role="alert" style="margin-top:10px;">{userError}</p>
      {/if}

      {#if data.users && data.users.length === 0}
        <p class="input-hint" style="margin-top:12px;">没有匹配的用户。</p>
      {:else if data.users && data.users.length > 0}
        <div class="app-table-wrap" style="margin-top:12px;">
          <table class="app-table" aria-label="用户搜索结果">
            <thead>
              <tr>
                <th>用户</th>
                <th>状态</th>
                <th>当前角色</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {#each data.users as u (u.id)}
                <tr>
                  <td>
                    <b>{u.display_name || u.username}</b>
                    <small class="text-secondary" style="display:block;font-size:12px;">@{u.username}</small>
                  </td>
                  <td>
                    <span class="sbadge {u.status === 'active' ? 'sb-success' : 'sb-gray'}">{statusLabel(u.status)}</span>
                  </td>
                  <td>
                    {#if u.roles.length === 0}
                      <span class="text-secondary" style="font-size:12px;">无角色</span>
                    {:else}
                      {#each u.roles as r (r)}
                        <span class="tag-chip" style="margin:1px 2px 1px 0;">{roleLabel(r)}</span>
                      {/each}
                    {/if}
                  </td>
                  <td>
                    <a class="btn secondary sm" href={`/admin/assignments?user=${encodeURIComponent(u.id)}${q ? `&q=${encodeURIComponent(q)}` : ''}`}>
                      管理角色
                    </a>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  </div>

  <!-- 选中用户：查看 / 授予 / 撤销角色（写操作 = 按钮 + 弹层） -->
  {#if selectedUser}
    <div class="app-card" style="margin-top:16px;">
      <div class="app-card__head">
        <h2>角色管理 · {selectedUser.display_name || selectedUser.username}</h2>
        <a class="btn ghost sm" href={`/admin/assignments${q ? `?q=${encodeURIComponent(q)}` : ''}`}>关闭</a>
      </div>
      <div class="app-card__body">
        <p class="input-hint">
          <b>@{selectedUser.username}</b> · 状态 {statusLabel(selectedUser.status)} · 当前角色：
          {#if selectedUser.roles.length === 0}
            <span class="text-secondary">无</span>
          {:else}
            {selectedUser.roles.map((r) => roleLabel(r)).join('、')}
          {/if}
        </p>

        <div style="display:flex;gap:8px;align-items:center;margin:14px 0;">
          <Button text="授予角色" variant="primary" size="sm" onclick={() => (grantOpen = true)} />
          <span class="text-secondary" style="font-size:12px;">在弹层中选择角色并填写操作原因（写审计）。</span>
        </div>

        <!-- 当前角色列表 + 撤销（行「⋮」菜单 + DangerConfirm；批量撤销走 BatchBar） -->
        {#if selectedUser.roles.length === 0}
          <p class="input-hint">该用户暂无角色。</p>
        {:else}
          <BatchBar count={selectedRoles.size} noun="个角色" onclear={() => (selectedRoles = new Set())}>
            <Button text="批量撤销" variant="danger" size="sm" onclick={() => (batchRevokeOpen = true)} />
          </BatchBar>
          <div class="app-table-wrap">
            <table class="app-table" aria-label="用户当前角色">
              <thead>
                <tr>
                  <th style="width:32px;">
                    <input
                      type="checkbox"
                      aria-label="全选角色"
                      checked={allRolesSelected}
                      onchange={toggleSelectAllRoles}
                    />
                  </th>
                  <th>角色</th>
                  <th>角色标识</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {#each selectedUser.roles as r (r)}
                  <tr>
                    <td>
                      <input
                        type="checkbox"
                        aria-label="选中角色 {roleLabel(r)}"
                        checked={selectedRoles.has(r)}
                        onchange={() => toggleSelectRole(r)}
                      />
                    </td>
                    <td><b>{roleLabel(r)}</b></td>
                    <td><code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;font-size:12px;">{r}</code></td>
                    <td>
                      <!-- 写操作：每行一个「⋮」菜单（约定 D）→ 撤销 DangerConfirm（隐藏表单机制不变） -->
                      <RowActionsMenu
                        label="更多操作：角色 {roleLabel(r)}"
                        actions={rowActions(r)}
                      />
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>
    </div>

    <!-- 授予角色：Dialog 内表单（user_id 隐藏域 + 角色选择 + 原因必填）。 -->
    <Dialog
      open={grantOpen}
      title="授予角色"
      description={`将把角色授予 @${selectedUser.username}；授予写入审计日志，重复授予幂等。`}
      onclose={() => (grantOpen = false)}
    >
      <form
        method="POST"
        action="?/grant"
        use:enhance={() => {
          isSubmitting = true;
          return async ({ result, update }) => {
            isSubmitting = false;
            toastActionResult(result);
            await update({ reset: false });
            if (result.type === 'success') grantOpen = false;
          };
        }}
        style="display:flex;flex-direction:column;gap:12px;"
      >
        <input type="hidden" name="user_id" value={selectedUser.id} />
        <div class="input-wrapper" style="margin-bottom:0;">
          <label class="input-label" for="asg-role">选择角色</label>
          <select class="input-field" id="asg-role" name="role_name" bind:value={grantRole} required>
            {#each roleState.items as item (item.id)}
              <option value={item.name}>{roleLabel(item.name)}（{item.name}）</option>
            {/each}
          </select>
        </div>
        <div class="input-wrapper" style="margin-bottom:0;">
          <label class="input-label" for="asg-grant-reason">操作原因（审计必填）</label>
          <input class="input-field" type="text" id="asg-grant-reason" name="reason" required placeholder="如：接任板块版主" />
        </div>
        <div style="display:flex;gap:8px;justify-content:flex-end;">
          <button type="button" class="btn ghost sm" onclick={() => (grantOpen = false)} disabled={isSubmitting}>取消</button>
          <Button text={isSubmitting ? '授予中...' : '确认授予'} variant="primary" size="sm" type="submit" disabled={isSubmitting} />
        </div>
      </form>
    </Dialog>

    <!-- 撤销角色：DangerConfirm + 隐藏表单（user_id + role_name + reason）。 -->
    <form
      method="POST"
      action="?/revoke"
      bind:this={revokeForm}
      use:enhance={() => {
        isSubmitting = true;
        return async ({ result, update }) => {
          isSubmitting = false;
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') {
            selectedRoles.delete(revokeTarget ?? '');
            selectedRoles = new Set(selectedRoles);
            revokeTarget = null;
            revokeReason = '';
            revokeError = '';
          }
        };
      }}
    >
      <input type="hidden" name="user_id" value={selectedUser.id} />
      <input type="hidden" name="role_name" value={revokeTarget ?? ''} />
      <input type="hidden" name="reason" value={revokeReason} />
    </form>

    <DangerConfirm
      open={revokeTarget !== null}
      title="撤销角色"
      description={`确认撤销 @${selectedUser.username} 的「${roleLabel(revokeTarget ?? '')}」（${revokeTarget ?? ''}）？撤销写入审计日志。`}
      confirmText="确认撤销"
      busy={isSubmitting}
      error={revokeError}
      oncancel={() => {
        revokeTarget = null;
        revokeError = '';
      }}
      onconfirm={() => {
        if (!revokeReason.trim()) {
          revokeError = '撤销原因必填（写入审计日志）';
          return;
        }
        revokeError = '';
        revokeForm?.requestSubmit();
      }}
    >
      <label class="input-label" for="asg-revoke-reason">撤销原因（审计必填）</label>
      <input
        id="asg-revoke-reason"
        type="text"
        class="input-field"
        bind:value={revokeReason}
        placeholder="必填，写入审计日志"
        required
      />
    </DangerConfirm>

    <!-- 批量撤销：Dialog 内批量隐藏表单（ids = 角色名列表，服务端循环单条端点）。 -->
    <Dialog
      open={batchRevokeOpen}
      title="批量撤销角色"
      description={`将撤销 @${selectedUser.username} 的 ${selectedRoles.size} 个角色；原因写入审计日志。`}
      onclose={() => (batchRevokeOpen = false)}
    >
      <form
        method="POST"
        action="?/batchRevoke"
        use:enhance={() => {
          isSubmitting = true;
          return async ({ result, update }) => {
            isSubmitting = false;
            toastActionResult(result);
            await update({ reset: false });
            if (result.type === 'success') {
              selectedRoles = new Set();
              batchRevokeOpen = false;
            }
          };
        }}
        style="display:flex;flex-direction:column;gap:12px;"
      >
        <input type="hidden" name="user_id" value={selectedUser.id} />
        <input type="hidden" name="ids" value={[...selectedRoles].join(',')} />
        <div class="input-wrapper" style="margin-bottom:0;">
          <label class="input-label" for="asg-batch-revoke-reason">操作原因（审计必填）</label>
          <input
            class="input-field"
            type="text"
            id="asg-batch-revoke-reason"
            name="reason"
            required
            placeholder="如：离任批量回收"
          />
        </div>
        <div style="display:flex;gap:8px;justify-content:flex-end;">
          <button type="button" class="btn ghost sm" onclick={() => (batchRevokeOpen = false)} disabled={isSubmitting}>取消</button>
          <Button text={isSubmitting ? '撤销中...' : '确认批量撤销'} variant="danger" size="sm" type="submit" disabled={isSubmitting} />
        </div>
      </form>
    </Dialog>
  {/if}
{/if}
