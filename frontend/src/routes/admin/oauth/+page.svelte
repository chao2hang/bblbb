<script lang="ts">
  // OAuth 客户端管理页（完整 CRUD）：列表 + 状态切换 + 创建 + 编辑 + 密钥重置。
  // 高敏操作（创建/编辑/重置/切换）命中 403 step_up_required 时展示 re-auth 弹窗；
  // secret 仅在创建/重置响应中出现一次，本页展示后不再保留。
  // M18-ADMIN-OPS（约定 D）：行内「编辑 / 启停表单 / 重置密钥表单」多个写入口
  // 收敛为每行一个「⋮」三点菜单（RowActionsMenu）→ 点菜单项打开该操作的
  // 单动作确认 Dialog（编辑 ?/edit 带 If-Match version；启停 ?/toggle
  // status+reason；轮换密钥 ?/rotateSecret reason 必填、danger 提交按钮）。
  // 批量启用/禁用（BatchBar + ?/batchToggle）与页头「+ 新建客户端」入口保持不变。
  import { enhance } from '$app/forms';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminOAuthActionData, AdminOAuthClientItem, AdminOAuthPageData } from './+page.server';

  let { data, form }: { data: AdminOAuthPageData; form?: AdminOAuthActionData | null } = $props();

  const clientsList = $derived.by(() => {
    const raw = form?.clients ?? data.clients;
    return raw ?? [];
  });
  const backendReady = $derived(data.state === 'ok');

  let q = $state('');
  let statusFilter = $state('');

  // JS 启用：动作结果走全局 Toast 浮窗；顶部内联横幅仅保留为无 JS 回退。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  const displayedClients = $derived.by(() => {
    let list = clientsList;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((c) => c.name.toLowerCase().includes(kw) || c.client_id.toLowerCase().includes(kw));
    }
    if (statusFilter === 'active') list = list.filter((c) => c.status === 'active');
    if (statusFilter === 'disabled') list = list.filter((c) => c.status !== 'active');
    return list;
  });

  // —— 批量选择（约定 B：选择列 + 共享 BatchBar 工具条） ——
  let selectedIds = $state<string[]>([]);
  const allSelected = $derived(
    displayedClients.length > 0 && displayedClients.every((c) => selectedIds.includes(c.id))
  );
  function toggleRow(id: string): void {
    selectedIds = selectedIds.includes(id)
      ? selectedIds.filter((x) => x !== id)
      : [...selectedIds, id];
  }
  function toggleSelectAll(): void {
    selectedIds = allSelected ? [] : displayedClients.map((c) => c.id);
  }

  // —— 批量启用/禁用 Dialog（一个 Dialog 服务一类操作，status 区分方向） ——
  let batchOpen = $state(false);
  let batchStatus = $state<'active' | 'disabled'>('active');
  let batchReason = $state('');
  function openBatch(next: 'active' | 'disabled'): void {
    batchStatus = next;
    batchReason = '';
    batchOpen = true;
  }

  // —— 新建客户端（Dialog 内表单，成功后关闭） ——
  let showCreate = $state(false);
  let createType = $state('confidential');

  // —— 行操作「⋮」菜单 + 弹层（约定 D：菜单项决定动作，弹层内单动作确认） ——
  // 按 opsAction 渲染对应表单节：编辑（?/edit，隐藏 id/version If-Match）/
  // 启停（?/toggle，status 取反 + 必填 reason）/ 轮换密钥（?/rotateSecret，
  // reason 必填，danger 提交按钮）。target 区分行，草稿随弹层关闭清空。
  type OpsAction = 'edit' | 'toggle' | 'rotateSecret';
  let opsTarget: AdminOAuthClientItem | null = $state(null);
  let opsAction = $state<OpsAction>('edit');
  let opsEditDraft = $state({ name: '', redirect_uris: '', post_logout_uris: '', scopes: '' });
  let opsToggleReason = $state('');
  let opsRotateReason = $state('');

  /** 启停目标状态（当前状态取反；?/toggle 契约 = id + status + reason）。 */
  function toggleStatusFor(client: AdminOAuthClientItem): 'active' | 'disabled' {
    return client.status === 'active' ? 'disabled' : 'active';
  }
  const opsToggleStatus = $derived(opsTarget ? toggleStatusFor(opsTarget) : '');

  function openOps(client: AdminOAuthClientItem, action: OpsAction): void {
    opsTarget = client;
    opsAction = action;
    opsEditDraft = {
      name: client.name,
      redirect_uris: (client.redirect_uris ?? []).join('\n'),
      post_logout_uris: (client.post_logout_uris ?? []).join('\n'),
      scopes: (client.scopes ?? []).join(' ')
    };
    opsToggleReason = '';
    opsRotateReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
    opsEditDraft = { name: '', redirect_uris: '', post_logout_uris: '', scopes: '' };
    opsToggleReason = '';
    opsRotateReason = '';
  }

  /** 行「⋮」菜单项：编辑 / 启停（按状态取反文案）/ 轮换密钥（Public 客户端禁用）。 */
  function rowActions(client: AdminOAuthClientItem) {
    const isPublic = client.client_type !== 'confidential';
    return [
      { label: '编辑客户端', run: () => openOps(client, 'edit') },
      {
        label: client.status === 'active' ? '禁用' : '启用',
        run: () => openOps(client, 'toggle')
      },
      {
        label: '轮换密钥',
        danger: true,
        disabled: isPublic,
        hint: isPublic ? 'Public 客户端无 Secret，无需轮换' : undefined,
        run: () => openOps(client, 'rotateSecret')
      }
    ];
  }

  /** 弹层标题/描述随菜单选定动作切换（单动作确认，非分节选择）。 */
  const opsActionMeta = $derived.by(() => {
    if (!opsTarget) return null;
    switch (opsAction) {
      case 'edit':
        return {
          title: `编辑客户端：${opsTarget.name}`,
          description: '修改应用信息（If-Match 乐观锁）；操作原因写审计。'
        };
      case 'toggle':
        return {
          title: `${opsToggleStatus === 'active' ? '启用' : '禁用'}客户端：${opsTarget.name}`,
          description: `将该客户端切换为「${opsToggleStatus === 'active' ? '已启用' : '已禁用'}」；操作原因写审计。`
        };
      case 'rotateSecret':
        return {
          title: `轮换 Client Secret：${opsTarget.name}`,
          description: '危险操作：新 Secret 仅在成功响应中显示一次；操作原因写审计。'
        };
    }
  });

  // —— 一次性 secret 展示（创建/重置成功后） ——
  const secretOnce = $derived(form?.secretOnce ?? null);
  let secretCopied = $state(false);
  async function copySecret() {
    if (!secretOnce) return;
    try {
      await navigator.clipboard.writeText(secretOnce);
      secretCopied = true;
      setTimeout(() => (secretCopied = false), 2000);
    } catch {
      showToast('复制失败，请手动选择复制', 'danger');
    }
  }

  // —— step-up 重新验证（M02-MFA-07） ——
  let reauthLoading = $state(false);
  let reauthCancelled = $state(false);
  let reauthError = $state<string | null>(null);
  $effect(() => {
    if (form?.stepUpRequired) reauthCancelled = false;
  });
</script>

<svelte:head>
  <title>OAuth 客户端 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="OAuth 客户端" />

{#if !backendReady}
  <section class="app-card">
    <div class="app-card__body">
      {#if data.state === 'forbidden'}
        <p class="input-hint is-error" role="alert">无权限：该页面仅限管理员访问。</p>
      {:else if data.state === 'not_implemented'}
        <p class="input-hint" role="note">OAuth 客户端管理接口开发中。</p>
      {:else}
        <p class="input-hint is-error" role="alert">{data.error || '服务暂时不可用'}</p>
      {/if}
    </div>
  </section>
{:else}
  {#if form?.message && !hasJs}
    <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
      {form.message}
    </div>
  {/if}

  <!-- 一次性 secret 展示（创建/重置成功） -->
  {#if secretOnce}
    <div class="app-notice" role="alert" style="margin-bottom:14px;padding:14px 16px;border:1px solid var(--color-warning, #b8860b);border-radius:var(--radius-sm);background:var(--color-bg-subtle);">
      <b>Client Secret（仅显示这一次）— {form?.secretClientName}</b>
      <div style="display:flex;gap:10px;align-items:center;margin-top:8px;flex-wrap:wrap;">
        <code style="padding:6px 10px;background:var(--color-bg-card);border:1px solid var(--color-border);border-radius:4px;font-size:13px;word-break:break-all;">{secretOnce}</code>
        <button type="button" class="btn secondary sm" onclick={copySecret}>{secretCopied ? '已复制' : '复制'}</button>
      </div>
      <p class="input-hint" style="margin:8px 0 0;">请立即妥善保存。关闭本页后无法再次查看，只能重置生成新 Secret。</p>
    </div>
  {/if}

  <!-- 顶部提示框（原型同款） -->
  <div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:14px 16px;border-radius:var(--radius-sm);font-size:13px;line-height:1.5;color:var(--color-text-secondary);margin-bottom:14px;">
    所有 redirect_uri 必须使用 HTTPS；Client Secret 仅在创建或重置凭证时显示一次。
  </div>

  <section class="app-card">
    <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:10px;">
      <h2>已注册 OAuth 客户端列表</h2>
      <div style="display:flex;gap:8px;align-items:center;">
        <span class="text-secondary" style="font-size:12px;">共 {displayedClients.length} 个客户端</span>
        <button type="button" class="btn primary sm" onclick={() => (showCreate = true)}>
          + 新建客户端
        </button>
      </div>
    </header>
    <div class="app-card__body">
      <!-- 工具栏 -->
      <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          bind:value={q}
          class="app-field"
          placeholder="按名称或 Client ID 搜索..."
          aria-label="搜索当前列表"
        />
        <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;">
          <select
            class="app-select"
            bind:value={statusFilter}
            aria-label="状态筛选"
            style="min-width:140px;"
          >
            <option value="">全部状态</option>
            <option value="active">已启用</option>
            <option value="disabled">已禁用</option>
          </select>
          {#if q || statusFilter}
            <button type="button" class="btn ghost sm" onclick={() => { q = ''; statusFilter = ''; }}>
              清除
            </button>
          {/if}
          <ExportButton
            label="导出 CSV"
            filename="oauth-clients"
            format="csv"
            columns={[
              { key: 'name', label: '应用名称' },
              { key: 'client_id', label: 'Client ID' },
              { key: 'client_type', label: '客户端类型' },
              { key: 'redirect_uris', label: '回调地址' },
              { key: 'status', label: '状态' }
            ]}
            getData={() => displayedClients as unknown as Record<string, unknown>[]}
          />
        </div>
      </div>

      <BatchBar count={selectedIds.length} onclear={() => (selectedIds = [])}>
        <Button text="批量启用" variant="secondary" size="sm" onclick={() => openBatch('active')} />
        <Button text="批量禁用" variant="danger" size="sm" onclick={() => openBatch('disabled')} />
      </BatchBar>

      {#if clientsList.length === 0}
        <p class="input-hint">暂无客户端，点击右上角「+ 新建客户端」创建。</p>
      {:else if displayedClients.length === 0}
        <p class="input-hint">没有匹配的客户端。</p>
      {:else}
        <div class="app-table-wrap">
          <table class="app-table" aria-label="客户端列表">
            <thead>
              <tr>
                <th style="width:36px;">
                  <input
                    type="checkbox"
                    aria-label="全选客户端"
                    checked={allSelected}
                    onchange={toggleSelectAll}
                  />
                </th>
                <th>应用名称</th>
                <th>Client ID</th>
                <th>客户端类型</th>
                <th>回调地址</th>
                <th>状态</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {#each displayedClients as client (client.id)}
                <tr>
                  <td>
                    <input
                      type="checkbox"
                      aria-label="选中 {client.name}"
                      checked={selectedIds.includes(client.id)}
                      onchange={() => toggleRow(client.id)}
                    />
                  </td>
                  <td><b>{client.name}</b></td>
                  <td><code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{client.client_id || client.id}</code></td>
                  <td><span class="text-secondary" style="font-size:13px;">{client.client_type === 'public' ? 'Public' : 'Confidential'}</span></td>
                  <td class="text-secondary" style="font-size:12px;max-width:220px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;" title={(client.redirect_uris ?? []).join('\n')}>
                    {(client.redirect_uris ?? []).length > 0 ? (client.redirect_uris ?? []).join(', ') : '—'}
                  </td>
                  <td>
                    <span class="sbadge {client.status === 'active' ? 'sb-success' : 'sb-gray'}">
                      {client.status === 'active' ? '已启用' : '已禁用'}
                    </span>
                  </td>
                  <td class="adm-acts">
                    <!-- 每行一个「⋮」动作菜单（约定 D）：菜单项打开对应单动作确认 Dialog -->
                    <RowActionsMenu
                      label="更多操作：客户端 {client.name}"
                      actions={rowActions(client)}
                    />
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  </section>

  <!-- 新建客户端：Dialog 内表单（约定 A），成功后关闭弹层；
       一次性 secret 在页面顶部提示区展示（创建/重置成功）。 -->
  <Dialog
    open={showCreate}
    title="新建 OAuth 客户端"
    description="创建后 Secret 仅显示一次；redirect_uri 必须为 HTTPS。"
    onclose={() => (showCreate = false)}
  >
    <form
      method="POST"
      action="?/create"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') showCreate = false;
        };
      }}
      style="display:flex;flex-direction:column;gap:14px;"
    >
      <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(220px, 1fr));gap:12px;">
        <div>
          <label class="input-label" for="oa-create-name">应用名称</label>
          <input class="input-field" type="text" id="oa-create-name" name="name" required maxlength="64" />
        </div>
        <div>
          <label class="input-label" for="oa-create-type">客户端类型</label>
          <select class="input-field" id="oa-create-type" name="client_type" bind:value={createType}>
            <option value="confidential">Confidential（服务端应用，含 Secret）</option>
            <option value="public">Public（SPA/移动端，无 Secret）</option>
          </select>
        </div>
      </div>
      <div>
        <label class="input-label" for="oa-create-redirect">Redirect URIs（每行一个，必填）</label>
        <textarea class="input-field" id="oa-create-redirect" name="redirect_uris" rows="3" required placeholder="https://app.example.com/callback"></textarea>
      </div>
      <div>
        <label class="input-label" for="oa-create-logout">Post-Logout URIs（每行一个，可留空）</label>
        <textarea class="input-field" id="oa-create-logout" name="post_logout_uris" rows="2"></textarea>
      </div>
      <div>
        <label class="input-label" for="oa-create-scopes">授权 Scopes（空格分隔，可留空）</label>
        <input class="input-field" type="text" id="oa-create-scopes" name="scopes" placeholder="openid profile email" />
      </div>
      <div>
        <label class="input-label" for="oa-create-reason">操作原因（审计）</label>
        <input class="input-field" type="text" id="oa-create-reason" name="reason" required placeholder="如：接入第三方应用" />
      </div>
      <div>
        <button type="submit" class="btn primary sm">创建客户端</button>
      </div>
    </form>
  </Dialog>

  <!-- 批量启用/禁用（约定 B）：公共参数（目标状态 + 必填原因）在 Dialog 内填写，
       ids 以逗号分隔隐藏字段提交；成功后清空选择并关闭弹层。 -->
  <Dialog
    open={batchOpen}
    title={batchStatus === 'active' ? '批量启用客户端' : '批量禁用客户端'}
    description={`将对 ${selectedIds.length} 个客户端${batchStatus === 'active' ? '启用' : '禁用'}；操作原因写入审计日志。`}
    onclose={() => (batchOpen = false)}
  >
    <form
      method="POST"
      action="?/batchToggle"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update();
          if (result.type === 'success') {
            selectedIds = [];
            batchOpen = false;
          }
        };
      }}
      style="display:flex;flex-direction:column;gap:12px;"
    >
      <input type="hidden" name="ids" value={selectedIds.join(',')} />
      <input type="hidden" name="status" value={batchStatus} />
      <div class="input-wrapper">
        <label class="input-label" for="oa-batch-reason">操作原因（写审计）</label>
        <input id="oa-batch-reason" name="reason" class="input-field" required bind:value={batchReason} placeholder="必填" />
      </div>
      <div>
        <button type="submit" class="btn primary sm">
          确认{batchStatus === 'active' ? '启用' : '禁用'} {selectedIds.length} 个客户端
        </button>
      </div>
    </form>
  </Dialog>

  <!-- 行操作 Dialog（约定 D：动作由「⋮」菜单选定，单动作确认 + reason 必填）：
       按 opsAction 渲染对应表单节，各节独立提交既有契约；成功 toastActionResult →
       update → 关弹层清 target/草稿。 -->
  <Dialog
    open={opsTarget !== null}
    title={opsActionMeta?.title ?? '客户端操作'}
    description={opsActionMeta?.description ?? ''}
    onclose={closeOps}
  >
    <!-- 动作节：编辑（?/edit，隐藏 id/version If-Match + 必填原因） -->
    {#if opsAction === 'edit'}
      <form
        method="POST"
        action="?/edit"
        use:enhance={() => {
          return async ({ result, update }) => {
            toastActionResult(result);
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;"
      >
        <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
        <input type="hidden" name="version" value={opsTarget?.version ?? ''} />
        <div>
          <label class="input-label" for="oa-ops-name">应用名称</label>
          <input class="input-field" type="text" id="oa-ops-name" name="name" bind:value={opsEditDraft.name} required />
        </div>
        <div>
          <label class="input-label" for="oa-ops-scopes">授权 Scopes（空格分隔，留空保持默认）</label>
          <input class="input-field" type="text" id="oa-ops-scopes" name="scopes" bind:value={opsEditDraft.scopes} placeholder="openid profile" />
        </div>
        <div>
          <label class="input-label" for="oa-ops-redirect">Redirect URIs（每行一个，必填）</label>
          <textarea class="input-field" id="oa-ops-redirect" name="redirect_uris" rows="3" required>{opsEditDraft.redirect_uris}</textarea>
        </div>
        <div>
          <label class="input-label" for="oa-ops-logout">Post-Logout URIs（每行一个，可留空）</label>
          <textarea class="input-field" id="oa-ops-logout" name="post_logout_uris" rows="2">{opsEditDraft.post_logout_uris}</textarea>
        </div>
        <div>
          <label class="input-label" for="oa-ops-edit-reason">操作原因（审计）</label>
          <input class="input-field" type="text" id="oa-ops-edit-reason" name="reason" required placeholder="如：更换生产回调域名" />
        </div>
        <div><Button text="保存修改" variant="primary" size="sm" type="submit" /></div>
      </form>
    {:else if opsAction === 'toggle'}
      <!-- 动作节：启停（?/toggle，status 取反 + 必填原因） -->
      <form
        method="POST"
        action="?/toggle"
        use:enhance={() => {
          return async ({ result, update }) => {
            toastActionResult(result);
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;"
      >
        <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
        <input type="hidden" name="status" value={opsToggleStatus} />
        <div class="input-wrapper">
          <label class="input-label" for="oa-ops-toggle-reason">操作原因（写审计）</label>
          <input id="oa-ops-toggle-reason" name="reason" class="input-field" required bind:value={opsToggleReason} placeholder="必填" />
        </div>
        <div>
          <Button
            text={opsTarget?.status === 'active' ? '确认禁用' : '确认启用'}
            variant="primary"
            size="sm"
            type="submit"
          />
        </div>
      </form>
    {:else if opsAction === 'rotateSecret'}
      <!-- 动作节：轮换密钥（?/rotateSecret，reason 必填；危险动作 → danger 提交按钮；
           仅 Confidential 客户端可轮换，新 Secret 仅成功响应显示一次）。 -->
      <form
        method="POST"
        action="?/rotateSecret"
        use:enhance={() => {
          return async ({ result, update }) => {
            toastActionResult(result);
            await update({ reset: false });
            if (result.type === 'success') closeOps();
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;"
      >
        <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
        <input type="hidden" name="name" value={opsTarget?.name ?? ''} />
        <div class="input-wrapper">
          <label class="input-label" for="oa-ops-rotate-reason">操作原因（写审计）</label>
          <input id="oa-ops-rotate-reason" name="reason" class="input-field" required bind:value={opsRotateReason} placeholder="必填" />
        </div>
        {#if opsTarget && opsTarget.client_type !== 'confidential'}
          <p class="input-hint">Public 客户端无 Secret，无需轮换。</p>
        {/if}
        <div>
          <Button
            text="确认轮换密钥"
            variant="danger"
            size="sm"
            type="submit"
            disabled={opsTarget?.client_type !== 'confidential'}
          />
        </div>
      </form>
    {/if}
  </Dialog>

  {#if form?.message && !hasJs && form?.stepUpRequired}
    <p class="input-hint is-error" role="alert" style="margin-top:12px;">{form.message}</p>
  {/if}
{/if}

<!-- step-up 重新验证（M02-MFA-07）：高敏操作命中 403 step_up_required 时展示。
     无 JS 时 Dialog 以固定层内联渲染，表单仍可用（SSR 基线保留）。 -->
<Dialog
  open={Boolean(form?.stepUpRequired) && !reauthCancelled}
  title="需要重新验证身份"
  description="OAuth 客户端的创建与修改属于高风险管理操作，要求近期重新认证。输入当前账号密码完成重新验证后，可继续刚才的操作。"
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
          reauthError = (result.data as unknown as AdminOAuthActionData | null)?.message ?? '密码验证失败，请重试';
          return;
        }
        await update();
      };
    }}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <div>
      <label class="input-label" for="oa-reauth-password">当前账号密码</label>
      <input class="input-field" type="password" id="oa-reauth-password" name="password" autocomplete="current-password" required />
    </div>
    <div style="display:flex;gap:8px;">
      <Button text={reauthLoading ? '验证中…' : '重新验证'} variant="primary" type="submit" disabled={reauthLoading} />
      <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
    </div>
  </form>
</Dialog>
