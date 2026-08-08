<script lang="ts">
  // M13-UI-01/ADMIN-02：管理用户页——列表 + 状态更新（If-Match + reason）。
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminUsersPageData, AdminUsersActionData } from './+page.server';

  let { data, form }: { data: AdminUsersPageData; form?: AdminUsersActionData | null } = $props();

  const state = $derived(data.state);
  const items = $derived(data.items);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  const conflict = $derived(form?.conflict === true);
</script>

<svelte:head>
  <title>用户管理 — BBLBB</title>
</svelte:head>

<div class="card">
  <div class="card-header"><span class="card-title">用户管理</span></div>
  <div class="card-body">
    {#if state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if state === 'not_implemented'}
      <p class="input-hint" role="note">用户管理接口开发中。</p>
    {:else if state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if state === 'ok'}
      {#if message}
        <p class="input-hint {conflict ? 'is-error' : ''}" role="status">{message}</p>
      {/if}
      {#if conflict}
        <p class="input-hint is-error" role="alert">用户版本已变化，请刷新后重试（If-Match 乐观锁）。</p>
      {/if}
      {#if !items || items.length === 0}
        <p class="input-hint">暂无用户数据。</p>
      {:else}
        <div style="overflow-x:auto;">
          <table class="table" aria-label="用户列表">
            <thead>
              <tr>
                <th>用户名</th>
                <th>邮箱</th>
                <th>状态</th>
                <th>角色</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {#each items as item (item.id)}
                <tr>
                  <td>
                    <a href="/users/{item.username}">{item.display_name || item.username}</a>
                    <span class="text-secondary" style="font-size:var(--text-sm);">LV.{item.level}</span>
                  </td>
                  <td><span class="text-secondary" style="font-size:var(--text-sm);">{item.email}</span></td>
                  <td><span class="badge">{item.status}</span></td>
                  <td><span class="text-secondary" style="font-size:var(--text-sm);">{item.roles.join('、') || 'member'}</span></td>
                  <td>
                    <form method="POST" action="?/update" use:enhance style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
                      <input type="hidden" name="id" value={item.id} />
                      <input type="hidden" name="version" value={String(item.version)} />
                      <select class="input-field" name="status" aria-label="状态">
                        <option value="active" selected={item.status === 'active'}>active</option>
                        <option value="restricted" selected={item.status === 'restricted'}>restricted</option>
                        <option value="banned" selected={item.status === 'banned'}>banned</option>
                        <option value="pending" selected={item.status === 'pending'}>pending</option>
                      </select>
                      <input type="text" class="input-field" name="reason" placeholder="原因（审计）" required style="max-width:160px;" />
                      <Button text="保存" variant="primary" size="sm" type="submit" />
                    </form>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {/if}
  </div>
</div>
