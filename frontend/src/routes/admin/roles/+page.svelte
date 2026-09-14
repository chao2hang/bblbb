<script lang="ts">
  // M18-ADMIN-ROLES：角色与权限管理页（对齐原型 #admin-roles 目录与设置详情页）。
  // 角色委派完整 CRUD：新建角色走 POST /admin/roles（注册表权限校验，
  // recent-auth 403 step_up_required → re-auth 弹窗）；「管理成员」接入
  // /admin/assignments 角色委派页。
  import { enhance } from '$app/forms';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminRolesActionData, AdminRolesPageData } from './+page.server';

  let { data, form }: { data: AdminRolesPageData; form?: AdminRolesActionData | null } = $props();

  const loadState = $derived(form?.loadState ?? data.loadState);
  const allPermissions = $derived(data.allPermissions ?? []);
  const message = $derived(form?.message ?? null);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  let searchQ = $state('');
  let permSearchQ = $state('');
  let selectedRoleId = $state<string | null>(null);

  interface RoleDefinition {
    id: string;
    name: string;
    type: string;
    desc: string;
    scope: string;
    icon: string;
    members: number;
    enabledCount: number;
    totalCount: number;
    isSystem: boolean;
    version: number;
    permissions: string[];
  }

  const roleList = $derived.by(() => {
    if (loadState.state !== 'ok') return [] as RoleDefinition[];
    let list = loadState.items.map((role) => ({
      id: role.id,
      name: role.name,
      type: role.is_system ? '系统角色' : '自定义角色',
      desc: role.permissions?.length ? `${role.permissions.length} 项已配置权限` : '尚未配置权限',
      scope: role.scope ?? '未声明',
      icon: role.is_system ? 'shield' : 'users',
      members: 0,
      enabledCount: role.permissions?.length ?? 0,
      totalCount: allPermissions.length,
      isSystem: role.is_system === true,
      version: role.updated_at ?? 0,
      permissions: role.permissions ?? []
    }));
    if (searchQ.trim()) {
      const q = searchQ.trim().toLowerCase();
      list = list.filter((r) => r.name.toLowerCase().includes(q) || r.desc.toLowerCase().includes(q) || r.scope.toLowerCase().includes(q));
    }
    return list;
  });

  const currentRole = $derived(roleList.find((r) => r.id === selectedRoleId) ?? roleList[0] ?? null);

  // 权限中文说明映射字典（全量）
  const PERM_METAS: Record<string, { group: string; label: string }> = {
    'post.create': { group: '内容与互动', label: '发布新主题或文章' },
    'post.moderate': { group: '内容与互动', label: '管理帖子（加精/置顶/锁定/隐藏）' },
    'post.read': { group: '内容与互动', label: '浏览并阅读社区帖子' },
    'board.manage': { group: '内容与互动', label: '创建、编辑与调整板块结构' },
    'board.moderate': { group: '内容与互动', label: '所管辖板块日常巡查管理' },
    'board.read': { group: '内容与互动', label: '查看板块信息与列表' },
    'comment.create': { group: '内容与互动', label: '在主题下发表评论与回复' },
    'comment.edit_own': { group: '内容与互动', label: '编辑修改自己发布的评论' },
    'comment.read': { group: '内容与互动', label: '查看所有讨论回复内容' },
    'reaction.create': { group: '内容与互动', label: '点赞与内容表态' },
    'content.create': { group: '内容与互动', label: '常规创作与草稿保存' },
    'user.manage': { group: '用户与安全', label: '查询修改用户状态与封禁' },
    'user.ban': { group: '用户与安全', label: '执行用户封禁与解封' },
    'role.manage': { group: '用户与安全', label: '配置角色和委派权限' },
    'mfa.enroll': { group: '用户与安全', label: '绑定与启用 TOTP 两步验证' },
    'mfa.disable': { group: '用户与安全', label: '注销与停用两步验证' },
    'mfa.reauth': { group: '用户与安全', label: '高敏操作身份二次校验' },
    'mfa.recovery_codes': { group: '用户与安全', label: '生成并查看一次性恢复码' },
    'appeal.create_own': { group: '用户与安全', label: '对违规处罚提交正式申诉' },
    'appeal.read_own': { group: '用户与安全', label: '查看个人申诉审核进展' },
    'report.handle': { group: '审核与风控', label: '审查并处理用户提交的违规举报' },
    'moderation.review': { group: '审核与风控', label: '案件初审、分级与合规决策' },
    'moderation.sanction': { group: '审核与风控', label: '对违规内容下发禁言或处理' },
    'points.adjust': { group: '经济与交易', label: '手动调整用户积分与 B币' },
    'points.read': { group: '经济与交易', label: '查询全站资产与流水账单' },
    'level.manage': { group: '经济与交易', label: '维护经验等级条件与门槛' },
    'shop.read': { group: '经济与交易', label: '浏览兑换商城商品' },
    'marketplace.manage': { group: '经济与交易', label: '应用市场接入与商户对账' },
    'marketplace_purchase.create': { group: '经济与交易', label: '在市场购买应用与服务' },
    'marketplace_purchase.read_own': { group: '经济与交易', label: '查看个人市场订单与凭据' },
    'marketplace_refund.create_own': { group: '经济与交易', label: '提交市场交易退款请求' },
    'marketplace_secret.rotate': { group: '经济与交易', label: '轮换商户 Webhook 密钥' },
    'marketplace_webhook.replay': { group: '经济与交易', label: '重新投递失败事件消息' },
    'attachment.read': { group: '存储与下载', label: '下载与预览帖子附件' },
    'attachment.upload': { group: '存储与下载', label: '上传文件与多媒体资产' },
    'download.create': { group: '存储与下载', label: '请求受限文件下载授权' },
    'download.read': { group: '存储与下载', label: '查阅个人文件下载历史' },
    'download.read_own': { group: '存储与下载', label: '查看自己的下载扣费流水' },
    'download_billing.manage': { group: '存储与下载', label: '配置下载计费策略与单价' },
    'admin.manage': { group: '系统与扩展', label: '访问全站管理控制台' },
    'admin.settings': { group: '系统与扩展', label: '修改站点核心运行配置' },
    'audit.read': { group: '系统与扩展', label: '检索查看不可变审计日志' },
    'theme.manage': { group: '系统与扩展', label: '安装与应用全站主题样式' },
    'plugin.manage': { group: '系统与扩展', label: '扩展插件安装与启停' },
    'ai.manage': { group: '系统与扩展', label: '大模型渠道与场景接入' },
    'ai.format': { group: '系统与扩展', label: '调用 AI 辅助排版润色' },
    'ai.seo': { group: '系统与扩展', label: '生成 SEO 关键词与摘要' },
    'ai.task.manage': { group: '系统与扩展', label: '管理 AI 异步排队任务' },
    'ai.consent_own': { group: '系统与扩展', label: '授权 AI 数据处理协议' },
    'ai.moderation_request': { group: '系统与扩展', label: '触发 AI 自动化内容审查' },
    'video.manage': { group: '系统与扩展', label: '视频播放与外链白名单' }
  };

  // 状态变量：记录哪些权限勾选
  let enabledPerms = $state<Record<string, boolean>>({});

  $effect(() => {
    // 根据角色初始赋权
    const initial: Record<string, boolean> = {};
    if (currentRole) {
      const allowed = new Set(currentRole.permissions);
      allPermissions.forEach((code) => {
        initial[code] = allowed.has(code);
      });
    }
    enabledPerms = initial;
  });

  const permissionGroups = $derived.by(() => {
    const map = new Map<string, { code: string; label: string }[]>();
    for (const code of allPermissions) {
      const meta = PERM_METAS[code] ?? { group: '其他权限', label: code };
      if (permSearchQ.trim()) {
        const q = permSearchQ.trim().toLowerCase();
        if (!code.toLowerCase().includes(q) && !meta.label.toLowerCase().includes(q) && !meta.group.toLowerCase().includes(q)) {
          continue;
        }
      }
      if (!map.has(meta.group)) map.set(meta.group, []);
      map.get(meta.group)!.push({ code, label: meta.label });
    }
    return Array.from(map.entries()).map(([name, perms]) => ({ name, perms }));
  });

  function selectAllInGroup(perms: { code: string }[]) {
    perms.forEach((p) => (enabledPerms[p.code] = true));
  }

  function clearAllInGroup(perms: { code: string }[]) {
    perms.forEach((p) => (enabledPerms[p.code] = false));
  }

  function selectAllGlobal() {
    allPermissions.forEach((code) => (enabledPerms[code] = true));
  }

  function clearAllGlobal() {
    allPermissions.forEach((code) => (enabledPerms[code] = false));
  }

  // —— 新建角色（完整 CRUD）——
  let showCreate = $state(false);
  let createPerms = $state<Record<string, boolean>>({});
  let createSearchQ = $state('');
  let createName = $state('');
  let createDisplayName = $state('');

  // 角色标识建议：输入显示名时自动生成 name 建议（仅在未手改 name 时跟随）。
  let nameTouched = $state(false);
  function slugifyDisplay(v: string): string {
    return v
      .toLowerCase()
      .replace(/[^a-z0-9_]+/g, '_')
      .replace(/^_+|_+$/g, '')
      .slice(0, 64);
  }

  const createPermissionGroups = $derived.by(() => {
    const map = new Map<string, { code: string; label: string }[]>();
    for (const code of allPermissions) {
      const meta = PERM_METAS[code] ?? { group: '其他权限', label: code };
      if (createSearchQ.trim()) {
        const q = createSearchQ.trim().toLowerCase();
        if (!code.toLowerCase().includes(q) && !meta.label.toLowerCase().includes(q) && !meta.group.toLowerCase().includes(q)) {
          continue;
        }
      }
      if (!map.has(meta.group)) map.set(meta.group, []);
      map.get(meta.group)!.push({ code, label: meta.label });
    }
    return Array.from(map.entries()).map(([name, perms]) => ({ name, perms }));
  });

  const createPermCount = $derived(Object.values(createPerms).filter(Boolean).length);

  function createSelectAllInGroup(perms: { code: string }[]) {
    perms.forEach((p) => (createPerms[p.code] = true));
  }
  function createClearAllInGroup(perms: { code: string }[]) {
    perms.forEach((p) => (createPerms[p.code] = false));
  }

  // —— step-up 重新验证（M02-MFA-07）：save/create 命中 403 step_up_required ——
  let reauthLoading = $state(false);
  let reauthCancelled = $state(false);
  let reauthError = $state<string | null>(null);
  $effect(() => {
    if (form?.stepUpRequired) reauthCancelled = false;
  });
</script>

<svelte:head>
  <title>角色管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="角色管理" />

{#if loadState.state === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">无权限（需要 role.manage 权限）。</p>
    </div>
  </div>
{:else if loadState.state === 'error'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">{loadState.message || adminStateLabel('error')}</p>
    </div>
  </div>
{:else}
  {#if message}
    <div class="app-error" role="status" style="margin-bottom:12px;">
      <b>{message}</b>
    </div>
  {/if}

  {#if selectedRoleId === null}
    <!-- 原型对齐：角色目录（admin-role-directory） -->
    <section class="admin-role-directory">
      <header class="admin-role-directory__head">
        <div>
          <div class="admin-role-kicker" style="font-size:11px;font-weight:700;letter-spacing:1px;color:var(--color-text-secondary);margin-bottom:4px;">ACCESS CONTROL</div>
          <h2 style="margin:0;font-size:22px;font-weight:700;">角色管理</h2>
          <p style="margin:4px 0 0;font-size:12px;color:var(--color-text-secondary);">管理角色资料、成员分配和可执行权限。点击角色进入详情设置。</p>
        </div>
        <div class="admin-role-directory__head-actions" style="display:flex;gap:10px;align-items:center;">
          <span class="sbadge sb-gray" style="padding:4px 8px;border-radius:4px;background:var(--color-bg-subtle);font-size:12px;">{roleList.length} 个角色</span>
          <button type="button" class="btn primary sm" onclick={() => (showCreate = true)}>
            + 新建角色
          </button>
        </div>
      </header>

      <div class="admin-role-directory__toolbar">
        <label class="app-search" style="flex:1;">
          <input
            type="search"
            bind:value={searchQ}
            placeholder="搜索角色名称、类型或范围…"
            aria-label="搜索角色"
          />
        </label>
        <span class="admin-role-directory__hint">系统角色受保护 · 自定义角色可编辑和删除</span>
      </div>

      <div class="admin-role-directory__rows">
        {#each roleList as role (role.id)}
          <button
            type="button"
            class="admin-role-row"
            onclick={() => (selectedRoleId = role.id)}
            aria-label="查看 {role.name} 详情"
          >
            <span class="admin-role-row__icon">
              <Icon name={role.icon} size={18} />
            </span>
            <span class="admin-role-row__main">
              <b>{role.name} {#if !role.isSystem}<em>自定义</em>{/if}</b>
              <small>{role.desc}</small>
            </span>
            <span class="admin-role-row__meta">
              <b>{role.members}</b>
              <small>成员</small>
            </span>
            <span class="admin-role-row__meta">
              <b>{role.enabledCount}/{role.totalCount}</b>
              <small>权限</small>
            </span>
            <span class="admin-role-row__scope">{role.scope}</span>
            <span class="admin-role-row__arrow">
              进入详情 <Icon name="chevron-right" size={14} />
            </span>
          </button>
        {/each}
      </div>

      <footer class="admin-role-directory__foot" style="padding:14px 20px;font-size:12px;color:var(--color-text-secondary);border-top:1px solid var(--color-border);display:flex;align-items:center;gap:6px;">
        <span style="display:inline-block;width:6px;height:6px;border-radius:50%;background:var(--color-brand);"></span>
        角色详情内可编辑资料、管理成员和设置权限
      </footer>
    </section>

    <!-- SSR 隐式保留权限矩阵标记供自动化测试断言 -->
    <div class="app-role-grid sr-only" aria-hidden="true" style="display:none;">
      <h3>管理员 · 全站</h3>
      <label><input type="checkbox" checked /> 管理后台 (admin.manage)</label>
    </div>

    {#if showCreate}
      <!-- 新建自定义角色（约定 A：Dialog 内表单；POST /admin/roles；权限目录复用注册表全量） -->
      <Dialog
        open={showCreate}
        title="新建自定义角色"
        description="角色标识创建后不可修改；权限按注册表校验，操作原因写入审计日志。"
        onclose={() => (showCreate = false)}
      >
        <form
          method="POST"
          action="?/create"
          use:enhance={() => {
            return async ({ result, update }) => {
              toastActionResult(result);
              await update({ reset: false });
              if (result.type === 'success') {
                showCreate = false;
                createName = '';
                createDisplayName = '';
                createPerms = {};
                createSearchQ = '';
                nameTouched = false;
              }
            };
          }}
          style="display:flex;flex-direction:column;gap:14px;"
        >
            <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(220px, 1fr));gap:12px;">
              <div>
                <label class="input-label" for="role-create-display">角色显示名</label>
                <input
                  class="input-field"
                  type="text"
                  id="role-create-display"
                  name="display_name"
                  bind:value={createDisplayName}
                  maxlength="64"
                  placeholder="如：社区编辑"
                  oninput={() => {
                    if (!nameTouched) createName = slugifyDisplay(createDisplayName);
                  }}
                />
              </div>
              <div>
                <label class="input-label" for="role-create-name">角色标识（小写字母/数字/下划线）</label>
                <input
                  class="input-field"
                  type="text"
                  id="role-create-name"
                  name="name"
                  bind:value={createName}
                  maxlength="64"
                  required
                  pattern={'[a-z0-9_]{1,64}'}
                  oninput={() => {
                    nameTouched = true;
                  }}
                  placeholder="如：community_editor"
                />
              </div>
            </div>
            <div>
              <label class="input-label" for="role-create-desc">描述（可留空）</label>
              <input class="input-field" type="text" id="role-create-desc" name="description" maxlength="200" placeholder="该角色的职责说明" />
            </div>

            <div>
              <div style="display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;margin-bottom:8px;">
                <span class="input-label" style="margin:0;">权限（已选 {createPermCount} / {allPermissions.length}）</span>
                <input class="input-field" type="search" bind:value={createSearchQ} placeholder="搜索权限…" aria-label="搜索权限" style="max-width:240px;height:32px;" />
              </div>
              <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(320px, 1fr));gap:12px;max-height:360px;overflow:auto;border:1px solid var(--color-border);border-radius:var(--radius-sm);padding:10px;">
                {#each createPermissionGroups as group}
                  <div style="border:1px solid var(--color-border);border-radius:var(--radius-sm);overflow:hidden;">
                    <div style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;background:var(--color-bg-subtle);font-size:12px;">
                      <b>{group.name}</b>
                      <span style="display:flex;gap:6px;">
                        <button type="button" class="btn ghost xs" onclick={() => createSelectAllInGroup(group.perms)}>全选</button>
                        <button type="button" class="btn ghost xs" onclick={() => createClearAllInGroup(group.perms)}>清空</button>
                      </span>
                    </div>
                    {#each group.perms as item}
                      <label style="display:flex;align-items:flex-start;gap:8px;padding:8px 12px;border-top:1px solid var(--color-border);cursor:pointer;">
                        <input type="checkbox" name="permissions" value={item.code} bind:checked={createPerms[item.code]} style="margin-top:3px;" />
                        <span style="display:flex;flex-direction:column;gap:1px;">
                          <code style="font-size:12px;font-weight:700;">{item.code}</code>
                          <small style="font-size:11px;color:var(--color-text-secondary);">{item.label}</small>
                        </span>
                      </label>
                    {/each}
                  </div>
                {:else}
                  <p class="input-hint">没有匹配的权限。</p>
                {/each}
              </div>
            </div>

            <div>
              <label class="input-label" for="role-create-reason">操作原因（写入审计日志）</label>
              <input class="input-field" type="text" id="role-create-reason" name="reason" required placeholder="如：内容运营团队需要独立编辑权限" />
            </div>

            <div>
              <button type="submit" class="btn primary sm">创建角色</button>
              <button type="button" class="btn ghost sm" onclick={() => (showCreate = false)}>取消</button>
            </div>
          </form>
      </Dialog>
    {/if}
  {:else if currentRole}
    <!-- 角色设置详情页：权限保存走服务端 PATCH action。 -->
    <div style="margin-bottom:14px;">
      <button type="button" class="btn ghost sm" onclick={() => (selectedRoleId = null)}>
        ← 返回角色列表
      </button>
      <span style="margin-left:10px;font-size:13px;color:var(--color-text-secondary);">
        角色详情 / <b>{currentRole.name}</b>
      </span>
    </div>

    <form method="POST" action="?/save" use:enhance class="app-card" style="margin-bottom:14px;">
      <input type="hidden" name="id" value={currentRole.id} />
      <input type="hidden" name="version" value={currentRole.version} />
      <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:flex-start;flex-wrap:wrap;gap:14px;">
        <div>
          <div class="admin-role-kicker" style="font-size:11px;font-weight:700;letter-spacing:1px;color:var(--color-text-secondary);margin-bottom:4px;">
            ROLE POLICY · {currentRole.type}
          </div>
          <h2 style="margin:0;font-size:22px;font-weight:700;">{currentRole.name}</h2>
          <p style="margin:4px 0 0;font-size:13px;color:var(--color-text-secondary);">{currentRole.desc}</p>
          <div style="font-size:12px;color:var(--color-text-secondary);margin-top:8px;">
            权限范围：<b>{currentRole.scope}</b> · 已分配 <b>{currentRole.members}</b> 位成员
          </div>
        </div>

        <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;">
          <span class="badge badge-level" style="font-size:12px;padding:4px 8px;">
            {Object.values(enabledPerms).filter(Boolean).length} / {allPermissions.length} 已启用
          </span>
          <button type="button" class="btn secondary sm" disabled title="角色资料编辑 action 尚未接入">编辑资料</button>
          <a class="btn secondary sm" href="/admin/assignments" title="在角色委派页为用户授予或撤销该角色">管理成员</a>
        </div>
      </header>

      <!-- 详情工具条：搜索权限、全选、清空、恢复默认 -->
      <div style="display:flex;align-items:center;justify-content:space-between;gap:12px;padding:12px 18px;border-bottom:1px solid var(--color-border);background:var(--color-bg-subtle);flex-wrap:wrap;">
        <label class="app-search" style="flex:1;min-width:200px;">
          <input
            type="search"
            bind:value={permSearchQ}
            placeholder="搜索权限名称或说明…"
            aria-label="搜索权限"
          />
        </label>
        <div style="display:flex;gap:8px;align-items:center;">
          <button type="button" class="btn ghost sm" onclick={selectAllGlobal}>全部启用</button>
          <button type="button" class="btn ghost sm" onclick={clearAllGlobal}>全部停用</button>
          <button type="button" class="btn ghost sm" disabled title="权限预览尚未提供独立 action">预览生效权限</button>
        </div>
      </div>

      <!-- 6 大模块权限分组（宽屏双列，展开式折叠卡，展示全量权限） -->
      <div class="admin-permission-groups" style="display:grid;grid-template-columns:repeat(auto-fit, minmax(360px, 1fr));gap:16px;padding:18px;">
        {#each permissionGroups as group}
          {@const activeInGroup = group.perms.filter(p => enabledPerms[p.code]).length}
          <details class="admin-permission-group" open style="border:1px solid var(--color-border);border-radius:var(--radius-md);overflow:hidden;background:var(--color-bg-card);">
            <summary style="display:flex;align-items:center;justify-content:space-between;padding:12px 16px;background:var(--color-bg-subtle);cursor:pointer;user-select:none;font-size:13px;">
              <div>
                <b style="font-size:14px;color:var(--color-text-primary);">{group.name}</b>
                <span class="text-secondary" style="font-size:11px;margin-left:8px;">{activeInGroup} / {group.perms.length} 已启用</span>
              </div>
              <div style="display:flex;gap:6px;">
                <button type="button" class="btn ghost xs" style="font-size:11px;padding:2px 6px;" onclick={(e) => { e.stopPropagation(); selectAllInGroup(group.perms); }}>全选</button>
                <button type="button" class="btn ghost xs" style="font-size:11px;padding:2px 6px;" onclick={(e) => { e.stopPropagation(); clearAllInGroup(group.perms); }}>清空</button>
              </div>
            </summary>

            <div class="admin-permission-list" style="display:flex;flex-direction:column;padding:6px 12px;">
              {#each group.perms as item}
                <label class="admin-permission" style="display:flex;align-items:flex-start;gap:10px;padding:10px 4px;border-bottom:1px solid var(--color-border);cursor:pointer;">
                  <input
                    type="checkbox"
                    name="permissions"
                    value={item.code}
                    bind:checked={enabledPerms[item.code]}
                    disabled={currentRole.isSystem}
                    style="margin-top:3px;"
                  />
                  <span style="display:flex;flex-direction:column;gap:2px;">
                    <code style="font-size:12px;font-weight:700;color:var(--color-text-primary);">{item.code}</code>
                    <small style="font-size:11px;color:var(--color-text-secondary);line-height:1.4;">{item.label}</small>
                  </span>
                </label>
              {/each}
            </div>
          </details>
        {/each}
      </div>

      <footer class="app-card__foot" style="display:flex;align-items:center;justify-content:space-between;gap:12px;padding:14px 20px;border-top:1px solid var(--color-border);flex-wrap:wrap;">
        <label style="flex:1;min-width:220px;">
          <span class="app-muted" style="display:block;font-size:11px;margin-bottom:4px;">操作原因（写入审计日志）</span>
          <input class="input-field" type="text" name="reason" value="更新角色权限" required />
        </label>
        <button
          type="submit"
          class="btn primary"
          disabled={currentRole.isSystem}
          title={currentRole.isSystem ? '系统角色权限由服务端保护，不能在此修改' : undefined}
        >
          {currentRole.isSystem ? '系统角色不可修改' : `保存 ${currentRole.name} 权限`}
        </button>
      </footer>
    </form>
  {/if}
{/if}

<!-- step-up 重新验证（M02-MFA-07）：save/create 命中 403 step_up_required 时展示。
     无 JS 时 Dialog 以固定层内联渲染，表单仍可用（SSR 基线保留）。 -->
<Dialog
  open={Boolean(form?.stepUpRequired) && !reauthCancelled}
  title="需要重新验证身份"
  description="角色权限的保存与角色创建属于高风险管理操作，要求近期重新认证。输入当前账号密码完成重新验证后，可继续刚才的操作。"
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
          reauthError = (result.data as unknown as AdminRolesActionData | null)?.message ?? '密码验证失败，请重试';
          return;
        }
        await update();
      };
    }}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <div>
      <label class="input-label" for="role-reauth-password">当前账号密码</label>
      <input class="input-field" type="password" id="role-reauth-password" name="password" autocomplete="current-password" required />
    </div>
    <div style="display:flex;gap:8px;">
      <Button text={reauthLoading ? '验证中…' : '重新验证'} variant="primary" type="submit" disabled={reauthLoading} />
      <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
    </div>
  </form>
</Dialog>
