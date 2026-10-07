<script lang="ts">
  // 积分与货币管理页：展示全站用户列表以及对应积分，支持在用户列表操作中直接调整积分。
  // 积分日志已收录于独立页面 /admin/points/logs。
  import { enhance } from '$app/forms';
  import { page } from '$app/state';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import { getCurrencyNameContext } from '$lib/site/currency-context.svelte';

  import type { AdminPointsPageData, AdminPointsActionData, AdminPointsUserItem } from './+page.server';

  let { data, form }: { data: AdminPointsPageData; form?: AdminPointsActionData | null } = $props();

  const currencyName = $derived(getCurrencyNameContext()?.currencyName ?? '金币');
  const items = $derived(data.items ?? []);
  const filters = $derived(data.filters ?? { q: '', status: '' });

  // 调整积分 Dialog
  let adjustOpen = $state(false);
  let targetUser = $state<AdminPointsUserItem | null>(null);
  let adjustSubmitting = $state(false);

  function openAdjust(user: AdminPointsUserItem) {
    targetUser = user;
    adjustOpen = true;
  }

  // 兼容从 /admin/users 等页面携带 ?username=xxx 导航过来时，自动选定该用户弹开调账弹层
  let hasAutoOpened = false;
  $effect(() => {
    const paramUser = page.url.searchParams.get('username');
    if (paramUser && !hasAutoOpened && items.length > 0) {
      hasAutoOpened = true;
      const matched = items.find(
        (u) => u.username.toLowerCase() === paramUser.toLowerCase()
      );
      if (matched) {
        openAdjust(matched);
      }
    }
  });

  // JS 启用：动作结果走全局 Toast 浮窗；无 JS 降级保留横幅
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // step-up 重新验证
  let reauthLoading = $state(false);
  let reauthCancelled = $state(false);
  let reauthError = $state<string | null>(null);

  $effect(() => {
    if (form?.stepUpRequired) reauthCancelled = false;
  });

  const ROLE_LABELS: Record<string, string> = {
    administrator: '管理员',
    global_moderator: '全站版主',
    board_moderator: '板块版主',
    member: '成员'
  };

  function roleLabel(name: string): string {
    return ROLE_LABELS[name] ?? name;
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

  function lastActiveLabel(ts: number | null): string {
    if (!ts) return '从未登录';
    const diff = Math.max(0, Date.now() - ts);
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return '刚刚';
    if (mins < 60) return `${mins} 分钟前`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours} 小时前`;
    const days = Math.floor(hours / 24);
    if (days === 1) return '昨天';
    if (days < 30) return `${days} 天前`;
    return new Date(ts).toLocaleDateString('zh-CN');
  }
</script>

<svelte:head>
  <title>积分与货币 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="积分与货币" description="管理全站用户积分余额及手动调账" />

<!-- 顶部业务分区导航 -->
<nav class="tabs" aria-label="积分功能分区" style="margin-bottom:16px;">
  <a href="/admin/points" class="tab is-active" aria-current="page">
    <Icon name="coins" size={15} />
    <span>用户积分</span>
  </a>
  <a href="/admin/points/logs" class="tab">
    <Icon name="file-text" size={15} />
    <span>积分日志</span>
  </a>
  <a href="/admin/points/rules" class="tab">
    <Icon name="list" size={15} />
    <span>积分规则</span>
  </a>
</nav>

{#if form?.message && !hasJs}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
    {form.message}
  </div>
{/if}

<!-- 用户列表与积分卡片 -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;">
    <div>
      <h2 style="margin:0;">用户积分管理</h2>
      <p class="text-secondary" style="font-size:13px;margin:2px 0 0;">
        展示全站用户及其 {currencyName} 实时余额；在每行操作栏可直接手动调账。查看变动流水请前往 <a href="/admin/points/logs" class="text-link">积分日志</a>。
      </p>
    </div>
    <span class="text-secondary" style="font-size:12px;">共 {items.length} 位用户</span>
  </header>

  <div class="app-card__body">
    <!-- GET 查询与筛选表单 -->
    <form method="GET" action="/admin/points" class="stack" style="gap:10px;margin-bottom:14px;">
      <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(200px, 1fr));gap:8px;">
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          用户检索
          <input
            type="search"
            name="q"
            class="app-field"
            value={filters.q}
            placeholder="搜索用户名、显示名或邮箱..."
            aria-label="按用户名或邮箱过滤"
          />
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          账号状态
          <select name="status" class="app-select" aria-label="状态筛选">
            <option value="" selected={!filters.status}>全部状态</option>
            <option value="active" selected={filters.status === 'active'}>正常</option>
            <option value="banned" selected={filters.status === 'banned'}>封禁</option>
            <option value="pending" selected={filters.status === 'pending'}>待验证</option>
            <option value="restricted" selected={filters.status === 'restricted'}>受限</option>
          </select>
        </label>
      </div>

      <div style="display:flex;align-items:center;gap:8px;margin-top:2px;">
        <button type="submit" class="btn secondary sm" style="width:100px;">
          <Icon name="search" size={14} />
          查询用户
        </button>
        {#if filters.q || filters.status}
          <a href="/admin/points" class="btn ghost sm">重置条件</a>
        {/if}
      </div>
    </form>

    {#if data.state === 'forbidden'}
      <div class="alert alert-danger" role="alert" style="margin-bottom:12px;padding:10px 14px;font-size:13px;">
        暂无权限查看用户列表（需要 user.manage 或 points.adjust 权限）。
      </div>
    {:else if data.state === 'error'}
      <div class="alert alert-danger" role="alert" style="margin-bottom:12px;padding:10px 14px;font-size:13px;">
        {data.error || '获取用户积分列表失败，请稍后重试'}
      </div>
    {/if}

    <div class="app-table-wrap">
      <table class="app-table" aria-label="用户积分列表">
        <thead>
          <tr>
            <th>用户</th>
            <th>角色</th>
            <th>状态</th>
            <th>{currencyName} 余额</th>
            <th>最近活动</th>
            <th style="min-width:140px;text-align:right;">操作</th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.id)}
            <tr>
              <td>
                <div style="display:flex;align-items:center;gap:10px;">
                  <Avatar name={item.display_name || item.username} size="sm" seed={item.username ?? item.id} />
                  <div>
                    <a class="text-link" href="/users/{item.username}" style="font-weight:600;">
                      {item.display_name || item.username}
                    </a>
                    {#if item.display_name && item.display_name !== item.username}
                      <span class="text-secondary" style="font-size:12px;margin-left:4px;">@{item.username}</span>
                    {/if}
                  </div>
                </div>
              </td>
              <td>
                <span class="text-secondary" style="font-size:13px;">
                  {item.roles.map(roleLabel).join('、') || '成员'}
                </span>
              </td>
              <td>
                <span class="sbadge {statusBadgeCls(item.status)}">
                  {statusLabel(item.status)}
                </span>
              </td>
              <td>
                <div style="display:flex;align-items:center;gap:6px;">
                  <b style="font-size:15px;color:var(--color-brand);">
                    {(item.coin_balance ?? 0).toLocaleString('zh-CN')}
                  </b>
                  <span class="badge badge-gray" style="font-size:11px;">{currencyName}</span>
                </div>
              </td>
              <td>
                <span class="text-secondary" style="font-size:12px;white-space:nowrap;">
                  {lastActiveLabel(item.last_login_at)}
                </span>
              </td>
              <td style="text-align:right;">
                <div style="display:inline-flex;gap:6px;align-items:center;">
                  <Button
                    text="调整积分"
                    variant="primary"
                    size="sm"
                    onclick={() => openAdjust(item)}
                  />
                  <a
                    href={`/admin/points/logs?username=${encodeURIComponent(item.username)}`}
                    class="btn ghost sm"
                    title="查看此用户的积分流水记录"
                  >
                    流水
                  </a>
                </div>
              </td>
            </tr>
          {/each}
          {#if items.length === 0}
            <tr>
              <td colspan="6" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                未找到符合条件的用户记录
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>

    {#if data.nextCursor}
      <div style="display:flex;justify-content:flex-end;margin-top:12px;">
        <a
          class="btn secondary sm"
          href={`/admin/points?after=${encodeURIComponent(data.nextCursor)}${filters.q ? `&q=${encodeURIComponent(filters.q)}` : ''}${filters.status ? `&status=${encodeURIComponent(filters.status)}` : ''}`}
        >
          下一页 →
        </a>
      </div>
    {/if}
  </div>
</section>

<!-- 底层配置引导卡片 -->
<details class="app-card">
  <summary class="app-card__head" style="cursor:pointer;user-select:none;">
    <h2 style="display:inline-block;font-size:15px;margin:0;">积分 / 活跃规则配置</h2>
  </summary>
  <div class="app-card__body" style="padding-top:12px;font-size:13px;">
    <p style="margin:0 0 10px;line-height:1.6;">
      「签到 / 发帖 / 回复 / 表态」等自动行为的积分奖励规则在
      <a href="/admin/points/rules" style="font-weight:600;">积分规则配置</a>
      页管理。修改历史可在 <a href="/admin/audit">审计日志</a> 追溯。
    </p>
    <a href="/admin/points/rules" class="btn secondary sm">前往积分规则配置 →</a>
  </div>
</details>

<!-- 调整积分 Dialog（目标用户在用户列表行中选定） -->
{#if targetUser}
  <Dialog
    open={adjustOpen}
    title={`调整用户积分 — ${targetUser.display_name || targetUser.username}`}
    description="手动调账（正数增加、负数扣减）；调整原因将记录在审计日志中并向用户发送系统通知。"
    onclose={() => {
      adjustOpen = false;
      targetUser = null;
    }}
  >
    <div style="background:var(--color-bg-subtle);border-radius:var(--radius-sm);padding:10px 14px;margin-bottom:12px;display:flex;justify-content:space-between;align-items:center;">
      <div>
        <span style="font-size:12px;color:var(--color-text-secondary);display:block;">目标用户</span>
        <strong>{targetUser.display_name || targetUser.username}</strong>
        <span class="text-secondary" style="font-size:12px;">(@{targetUser.username})</span>
      </div>
      <div style="text-align:right;">
        <span style="font-size:12px;color:var(--color-text-secondary);display:block;">当前余额</span>
        <strong style="font-size:16px;color:var(--color-brand);">{targetUser.coin_balance ?? 0}</strong>
        <span style="font-size:12px;margin-left:2px;">{currencyName}</span>
      </div>
    </div>

    <form
      method="POST"
      action="?/adjust"
      use:enhance={() => {
        adjustSubmitting = true;
        return async ({ result, update }) => {
          adjustSubmitting = false;
          toastActionResult(result);
          await update();
          if (result.type === 'success') {
            adjustOpen = false;
            targetUser = null;
          }
        };
      }}
      class="stack"
      style="gap:10px;"
    >
      <input type="hidden" name="username" value={targetUser.username} />
      <input type="hidden" name="currency" value="coin" />

      <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px;">
        <div>
          <span class="field-label" style="font-size:12px;font-weight:600;margin-bottom:4px;display:block;">币种</span>
          <span class="app-select" style="width:100%;display:flex;align-items:center;background:var(--color-bg-subtle);">
            {currencyName}
          </span>
        </div>
        <label>
          <span class="field-label" style="font-size:12px;font-weight:600;margin-bottom:4px;display:block;">
            调整数值（正增负减）*
          </span>
          <input
            type="number"
            name="amount"
            class="input-field"
            placeholder="如 50 或 -20"
            required
          />
        </label>
      </div>

      <label>
        <span class="field-label" style="font-size:12px;font-weight:600;margin-bottom:4px;display:block;">
          调整原因（写审计日志，必填）*
        </span>
        <input
          type="text"
          name="reason"
          class="input-field"
          placeholder="如：活动达人奖励发放 / 违规积分扣减"
          required
        />
      </label>

      <div style="display:flex;gap:8px;justify-content:flex-end;margin-top:4px;">
        <button
          type="button"
          class="btn ghost sm"
          onclick={() => {
            adjustOpen = false;
            targetUser = null;
          }}
        >
          取消
        </button>
        <Button
          text={adjustSubmitting ? '提交中…' : '确认调整'}
          variant="primary"
          size="sm"
          type="submit"
          disabled={adjustSubmitting}
        />
      </div>
    </form>
  </Dialog>
{/if}

<!-- step-up 重新验证（高敏操作命中 403 step_up_required 时展示） -->
<Dialog
  open={Boolean(form?.stepUpRequired) && !reauthCancelled}
  title="需要重新验证身份"
  description="积分调整属于高风险管理操作，要求近期重新认证。输入当前账号密码完成重新验证后，可继续刚才的操作。"
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
          reauthError = (result.data as unknown as AdminPointsActionData | null)?.message ?? '密码验证失败，请重试';
          return;
        }
        toastActionResult(result);
        await update();
      };
    }}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <div>
      <label class="input-label" for="pts-reauth-password">当前账号密码</label>
      <input
        class="input-field"
        type="password"
        id="pts-reauth-password"
        name="password"
        autocomplete="current-password"
        required
      />
    </div>
    <div style="display:flex;gap:8px;">
      <Button
        text={reauthLoading ? '验证中…' : '重新验证'}
        variant="primary"
        type="submit"
        disabled={reauthLoading}
      />
      <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
    </div>
  </form>
</Dialog>
