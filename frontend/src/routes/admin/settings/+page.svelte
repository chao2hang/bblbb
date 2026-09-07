<script lang="ts">
  // GAP-FIX（管理域·系统设置）：对齐原型 prototype/pages/admin-settings.html——
  // app-card 三卡布局（功能开关+配置建议 / 站点信息 / 数据与导出），页脚状态行
  // +「恢复默认 / 保存设置」操作行（原型 .app-card__foot.sys-foot 位置）。
  // 与原型的行为对齐：
  // - 脏标记：开关与字段逐项 .is-dirty（CSS 渲染「已修改」角标）；
  // - 状态行：正在保存 / 有 N 处未保存的更改 / 最近保存于 HH:mm · 配置已同步；
  // - 配置建议（.app-promo）：开注册+关邮箱验证、维护模式开启时给上下文建议；
  // - 逐字段内联错误（.app-field-error）：编辑即时清除已展示错误，提交时
  //   集中校验并聚焦第一处错误（无 JS 时由 HTML5 required/min/max 兜底）；
  // - 公开源（public_source）为 0063 新列，校验语义与原型 sysValidate 一致；
  // - 恢复默认 = 回滚到服务端已保存配置（生产语义，非原型演示重置）；
  // - If-Match 乐观锁冲突（409）→ .app-error 横幅提示刷新；
  // - 维护模式开启时顶部 .app-notice 提示（原型同款文案）。
  // 原型「数据与演示」卡中的模拟保存失败 / 重置演示数据为演示专属，不落地；
  // 「导出当前配置」保留为真实功能（客户端导出 JSON，含未保存更改）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { onMount } from 'svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminSettingsActionData, AdminSettingsPageData } from './+page.server';

  let { data, form }: { data: AdminSettingsPageData; form?: AdminSettingsActionData | null } = $props();

  type SwitchKey =
    | 'open_registration'
    | 'email_verification'
    | 'anonymous_replies'
    | 'public_rss'
    | 'maintenance_mode';

  /** 功能开关（顺序与原型 data-sys-key 一致；hint 文案与原型逐字对齐）。 */
  const SWITCHES: Array<{ key: SwitchKey; label: string; hint: string }> = [
    { key: 'open_registration', label: '开放注册', hint: '关闭后仅管理员可邀请新用户' },
    { key: 'email_verification', label: '注册邮箱验证', hint: '开启后新用户需验证邮箱才能发帖' },
    { key: 'anonymous_replies', label: '匿名回复', hint: '允许隐藏作者信息（不推荐）' },
    { key: 'public_rss', label: '公开 RSS / Atom', hint: '为订阅工具提供只读 feed' },
    { key: 'maintenance_mode', label: '维护模式', hint: '开启后前台展示维护提示，仅管理员可操作' }
  ];

  /** 语言选项（原型 select：zh-CN / en；已保存的非标值动态保留）。 */
  const LANG_OPTIONS: Array<{ value: string; label: string }> = [
    { value: 'zh-CN', label: '简体中文' },
    { value: 'en', label: 'English' }
  ];

  /** 可校验字段键（lang 无业务校验，仅占位以统一错误盒结构）。 */
  type FieldKey = 'siteName' | 'lang' | 'source' | 'rateLimit';
  const FIELD_KEYS: FieldKey[] = ['siteName', 'lang', 'source', 'rateLimit'];
  const FIELD_ELEMENT_ID: Record<FieldKey, string> = {
    siteName: 'set-site-name',
    lang: 'set-default-lang',
    source: 'set-public-source',
    rateLimit: 'set-rate-limit'
  };

  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);

  // ── 表单态（SSR 初始即取服务端值 → 无 JS 也能渲染/提交正确初值；
  //    后续 data 变化由下方 $effect 同步，故初始化读 data 属预期）──
  function initialForm() {
    const s = data.settings;
    return {
      switches: {
        open_registration: Boolean(s?.open_registration),
        email_verification: Boolean(s?.email_verification),
        anonymous_replies: Boolean(s?.anonymous_replies),
        public_rss: Boolean(s?.public_rss),
        maintenance_mode: Boolean(s?.maintenance_mode)
      } as Record<SwitchKey, boolean>,
      siteName: s?.site_name ?? '',
      defaultLang: s?.default_lang ?? 'zh-CN',
      publicSource: s?.public_source ?? '',
      rateLimit: Number(s?.api_rate_limit ?? 0),
      smtpEnabled: Boolean(s?.smtp_enabled),
      smtpHost: s?.smtp_host ?? '',
      smtpPort: Number(s?.smtp_port ?? 587),
      smtpUser: s?.smtp_user ?? '',
      smtpPass: '',
      smtpPassConfigured: Boolean(s?.smtp_pass_configured),
      smtpFromEmail: s?.smtp_from_email ?? '',
      smtpFromName: s?.smtp_from_name ?? '',
      smtpEncryption: s?.smtp_encryption ?? 'starttls'
    };
  }

  const init = initialForm();
  let switches = $state<Record<SwitchKey, boolean>>(init.switches);
  let siteName = $state(init.siteName);
  let defaultLang = $state(init.defaultLang);
  let publicSource = $state(init.publicSource);
  let rateLimit = $state<number>(init.rateLimit);
  let smtpEnabled = $state(init.smtpEnabled);
  let smtpHost = $state(init.smtpHost);
  let smtpPort = $state<number>(init.smtpPort);
  let smtpUser = $state(init.smtpUser);
  let smtpPass = $state('');
  let smtpPassConfigured = $state(init.smtpPassConfigured);
  let smtpFromEmail = $state(init.smtpFromEmail);
  let smtpFromName = $state(init.smtpFromName);
  let smtpEncryption = $state(init.smtpEncryption);
  let errors = $state<Record<FieldKey, string>>({
    siteName: '',
    lang: '',
    source: '',
    rateLimit: ''
  });
  let saving = $state(false);
  /** 本次会话最近一次成功保存时间（状态行「最近保存于 …」用）。 */
  let lastSavedAt = $state<number | null>(null);

  // 保存成功后 use:enhance → update() 重取 load：表单态回同步服务端最新值。
  $effect(() => {
    const s = data.settings;
    if (!s) return;
    switches = {
      open_registration: Boolean(s.open_registration),
      email_verification: Boolean(s.email_verification),
      anonymous_replies: Boolean(s.anonymous_replies),
      public_rss: Boolean(s.public_rss),
      maintenance_mode: Boolean(s.maintenance_mode)
    };
    siteName = s.site_name ?? '';
    defaultLang = s.default_lang ?? 'zh-CN';
    publicSource = s.public_source ?? '';
    rateLimit = Number(s.api_rate_limit ?? 0);
    smtpEnabled = Boolean(s.smtp_enabled);
    smtpHost = s.smtp_host ?? '';
    smtpPort = Number(s.smtp_port ?? 587);
    smtpUser = s.smtp_user ?? '';
    smtpPass = '';
    smtpPassConfigured = Boolean(s.smtp_pass_configured);
    smtpFromEmail = s.smtp_from_email ?? '';
    smtpFromName = s.smtp_from_name ?? '';
    smtpEncryption = s.smtp_encryption ?? 'starttls';
    errors = { siteName: '', lang: '', source: '', rateLimit: '' };
  });

  // 已保存快照（脏检查 + 恢复默认的目标值）。
  const saved = $derived.by(() => {
    const s = data.settings;
    if (!s) return null;
    return {
      open_registration: Boolean(s.open_registration),
      email_verification: Boolean(s.email_verification),
      anonymous_replies: Boolean(s.anonymous_replies),
      public_rss: Boolean(s.public_rss),
      maintenance_mode: Boolean(s.maintenance_mode),
      site_name: s.site_name ?? '',
      default_lang: s.default_lang ?? 'zh-CN',
      public_source: s.public_source ?? '',
      api_rate_limit: Number(s.api_rate_limit ?? 0),
      smtp_enabled: Boolean(s.smtp_enabled),
      smtp_host: s.smtp_host ?? '',
      smtp_port: Number(s.smtp_port ?? 587),
      smtp_user: s.smtp_user ?? '',
      smtp_from_email: s.smtp_from_email ?? '',
      smtp_from_name: s.smtp_from_name ?? '',
      smtp_encryption: s.smtp_encryption ?? 'starttls'
    };
  });

  /** 与已保存配置的差异字段数。 */
  const changedCount = $derived.by(() => {
    const s = saved;
    if (!s) return 0;
    let n = 0;
    for (const sw of SWITCHES) if (switches[sw.key] !== s[sw.key]) n += 1;
    if (siteName !== s.site_name) n += 1;
    if (defaultLang !== s.default_lang) n += 1;
    if (publicSource !== s.public_source) n += 1;
    if (Number(rateLimit) !== s.api_rate_limit) n += 1;
    if (smtpEnabled !== s.smtp_enabled) n += 1;
    if (smtpHost !== s.smtp_host) n += 1;
    if (Number(smtpPort) !== s.smtp_port) n += 1;
    if (smtpUser !== s.smtp_user) n += 1;
    if (smtpPass !== '') n += 1;
    if (smtpFromEmail !== s.smtp_from_email) n += 1;
    if (smtpFromName !== s.smtp_from_name) n += 1;
    if (smtpEncryption !== s.smtp_encryption) n += 1;
    return n;
  });
  const dirty = $derived(changedCount > 0);

  /** 原型 sysValidate 同语义的逐字段校验（错误文案与原型逐字一致）。 */
  function validate(): Record<FieldKey, string> {
    const errs: Record<FieldKey, string> = {
      siteName: '',
      lang: '',
      source: '',
      rateLimit: ''
    };
    if (!siteName.trim()) errs.siteName = '站点名称不能为空';
    else if ([...siteName.trim()].length > 40) errs.siteName = '站点名称不能超过 40 个字符';
    if (!/^https?:\/\/\S+\.\S+/.test(publicSource.trim())) errs.source = '请输入有效的 http(s):// 地址';
    const n = Number(rateLimit);
    const raw: unknown = rateLimit;
    if (
      raw === '' ||
      raw === null ||
      raw === undefined ||
      !Number.isFinite(n) ||
      !Number.isInteger(n) ||
      n < 1 ||
      n > 10000
    ) {
      errs.rateLimit = '限流需为 1 - 10000 的整数';
    }
    return errs;
  }

  /** 编辑时：仅更新已展示错误的字段（修正后即时清除；新错误提交时集中展示——原型行为）。 */
  function onEdit() {
    const errs = validate();
    const next = { ...errors };
    for (const k of FIELD_KEYS) {
      if (next[k]) next[k] = errs[k];
    }
    errors = next;
  }

  /** 无 JS 时保存按钮必须可提交（SSR 不禁用）；hydration 后才启用脏禁用。 */
  let mounted = $state(false);
  onMount(() => {
    mounted = true;
  });

  /** 恢复默认：生产语义 = 回滚到服务端已保存配置（原型为演示重置）。 */
  function restoreDefaults() {
    const s = saved;
    if (!s) return;
    switches = {
      open_registration: s.open_registration,
      email_verification: s.email_verification,
      anonymous_replies: s.anonymous_replies,
      public_rss: s.public_rss,
      maintenance_mode: s.maintenance_mode
    };
    siteName = s.site_name;
    defaultLang = s.default_lang;
    publicSource = s.public_source;
    rateLimit = s.api_rate_limit;
    smtpEnabled = s.smtp_enabled;
    smtpHost = s.smtp_host;
    smtpPort = s.smtp_port;
    smtpUser = s.smtp_user;
    smtpPass = '';
    smtpFromEmail = s.smtp_from_email;
    smtpFromName = s.smtp_from_name;
    smtpEncryption = s.smtp_encryption;
    errors = { siteName: '', lang: '', source: '', rateLimit: '' };
    showToast('已恢复为当前已保存配置', 'info');
  }

  /** 导出当前表单配置为 JSON（含未保存更改；原型 data-sys-export 的生产落地）。 */
  function exportConfig() {
    const payload = {
      exported_at: new Date().toISOString(),
      unsaved_changes: dirty,
      settings: {
        ...switches,
        site_name: siteName,
        default_lang: defaultLang,
        public_source: publicSource,
        api_rate_limit: Number(rateLimit),
        smtp_enabled: smtpEnabled,
        smtp_host: smtpHost,
        smtp_port: Number(smtpPort),
        smtp_user: smtpUser,
        smtp_pass_configured: smtpPassConfigured || Boolean(smtpPass),
        smtp_from_email: smtpFromEmail,
        smtp_from_name: smtpFromName,
        smtp_encryption: smtpEncryption
      }
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    const d = new Date();
    const pad = (x: number) => String(x).padStart(2, '0');
    a.download = `bblbb-settings-${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}.json`;
    a.href = url;
    a.click();
    URL.revokeObjectURL(url);
    showToast('配置已导出', 'success');
  }

  const langOptions = $derived(
    LANG_OPTIONS.some((o) => o.value === defaultLang) || !defaultLang
      ? LANG_OPTIONS
      : [...LANG_OPTIONS, { value: defaultLang, label: `${defaultLang}（当前）` }]
  );

  /** 配置建议（原型 data-sys-advisory 同逻辑）。 */
  const advisory = $derived.by(() => {
    const msgs: string[] = [];
    if (switches.open_registration && !switches.email_verification) {
      msgs.push('注册开放且邮箱验证关闭，新账号注册后即可发帖，建议开启邮箱验证');
    }
    if (switches.maintenance_mode) {
      msgs.push('维护模式下前台访客将看到维护提示，仅管理员可操作');
    }
    return msgs;
  });

  const savedAtLabel = $derived(
    lastSavedAt === null
      ? ''
      : new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false }).format(
          lastSavedAt
        )
  );
</script>

<svelte:head>
  <title>系统设置 — BBLBB</title>
</svelte:head>

<PageHeader title="系统设置" />

{#if data.state === 'forbidden'}
  <section class="app-card">
    <header class="app-card__head"><h2>系统设置</h2></header>
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    </div>
  </section>
{:else if data.state === 'not_implemented'}
  <section class="app-card">
    <header class="app-card__head"><h2>系统设置</h2></header>
    <div class="app-card__body">
      <p class="input-hint" role="note">系统设置接口开发中。</p>
    </div>
  </section>
{:else if data.state === 'error'}
  <section class="app-card">
    <header class="app-card__head"><h2>系统设置</h2></header>
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    </div>
  </section>
{:else if data.settings}
  {#if switches.maintenance_mode}
    <div class="app-notice" role="status" style="margin-bottom:14px;">
      <Icon name="alert-triangle" size={16} />
      <span>维护模式已开启：前台用户会看到维护提示，仅管理员可访问后台。</span>
    </div>
  {/if}

  <!-- 功能开关（原型 app-check 列表 + data-sys-advisory 配置建议） -->
  <section class="app-card">
    <header class="app-card__head"><h2>功能开关</h2></header>
    <div class="app-card__body">
      <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(280px, 1fr));gap:12px;">
        {#each SWITCHES as sw (sw.key)}
          <label
            class="app-check"
            class:is-dirty={saved !== null && switches[sw.key] !== saved[sw.key]}
            style="padding:10px 12px;border:1px solid var(--color-border);border-radius:var(--radius-sm);background:var(--color-bg-subtle, rgba(0,0,0,0.02));margin:0;"
          >
            <input type="checkbox" name={sw.key} bind:checked={switches[sw.key]} oninput={onEdit} />
            <span>{sw.label}<span class="app-field-help">{sw.hint}</span></span>
          </label>
        {/each}
      </div>
      {#if advisory.length}
        <div class="app-promo" role="note" style="margin-top:12px;">
          <b>配置建议</b>
          <span>{advisory.join('；')}。</span>
        </div>
      {/if}
    </div>
  </section>

  <!-- 站点信息（原型 adm-form-grid + 页脚操作行） -->
  <section class="app-card">
    <header class="app-card__head">
      <h2>站点信息</h2>
    </header>
    <div class="app-card__body">
      {#if message}
        <div class="app-error" role={conflict ? 'alert' : 'status'} style="margin-bottom:14px;">
          <Icon name="alert-triangle" size={16} />
          <div>
            <b>{conflict ? '保存失败（版本冲突）' : '保存失败'}</b>
            <span>{message}</span>
            {#if conflict}
              <span>设置已被其他人修改（If-Match 乐观锁冲突）。请刷新页面获取最新版本后再保存。</span>
            {/if}
          </div>
        </div>
      {/if}

      <form
        method="POST"
        action="?/save"
        use:enhance={({ cancel }) => {
          // 提交前集中校验（原型：toast + 聚焦第一处错误）。
          const errs = validate();
          errors = errs;
          const failed = FIELD_KEYS.filter((k) => errs[k]);
          if (failed.length) {
            cancel();
            showToast(`请先修正表单中的 ${failed.length} 处错误`, 'danger');
            document.getElementById(FIELD_ELEMENT_ID[failed[0]])?.focus();
            return async () => {};
          }
          saving = true;
          return async ({ result, update }) => {
            if (result.type === 'success') {
              const payload = result.data as { message?: string } | undefined;
              lastSavedAt = Date.now();
              saving = false;
              await update();
              showToast(payload?.message ?? '设置已保存', 'success');
            } else {
              saving = false;
              // 失败横幅由 form prop 渲染（原型 .app-error），不再重复 toast。
              await update();
            }
          };
        }}
      >
        <input type="hidden" name="version" value={String(data.version)} />
        {#each SWITCHES as sw (sw.key)}
          {#if switches[sw.key]}
            <input type="hidden" name={sw.key} value="on" />
          {/if}
        {/each}

        <div class="adm-form-grid">
          <div class="app-form-field" class:is-dirty={saved !== null && siteName !== saved.site_name}>
            <label class="app-field-label" for="set-site-name">站点名称<span class="app-required">*</span></label>
            <input
              id="set-site-name"
              name="site_name"
              class="app-field"
              class:is-error={!!errors.siteName}
              required
              maxlength="40"
              bind:value={siteName}
              oninput={onEdit}
            />
            {#if errors.siteName}<p class="app-field-error" role="alert">{errors.siteName}</p>{/if}
          </div>
          <div class="app-form-field" class:is-dirty={saved !== null && defaultLang !== saved.default_lang}>
            <label class="app-field-label" for="set-default-lang">默认语言</label>
            <select
              id="set-default-lang"
              name="default_lang"
              class="app-select"
              bind:value={defaultLang}
              onchange={onEdit}
            >
              {#each langOptions as opt (opt.value)}
                <option value={opt.value}>{opt.label}</option>
              {/each}
            </select>
          </div>
          <div class="app-form-field" class:is-dirty={saved !== null && publicSource !== saved.public_source}>
            <label class="app-field-label" for="set-public-source">公开源（RSS / API）<span class="app-required">*</span></label>
            <input
              id="set-public-source"
              name="public_source"
              class="app-field"
              class:is-error={!!errors.source}
              required
              maxlength="200"
              spellcheck="false"
              placeholder="https://bblbb.example.com"
              bind:value={publicSource}
              oninput={onEdit}
            />
            {#if errors.source}<p class="app-field-error" role="alert">{errors.source}</p>{/if}
          </div>
          <div class="app-form-field" class:is-dirty={saved !== null && Number(rateLimit) !== saved.api_rate_limit}>
            <label class="app-field-label" for="set-rate-limit">API 限流（次 / 分钟 / IP）<span class="app-required">*</span></label>
            <input
              id="set-rate-limit"
              name="api_rate_limit"
              class="app-field"
              class:is-error={!!errors.rateLimit}
              type="number"
              min="1"
              max="10000"
              step="1"
              required
              bind:value={rateLimit}
              oninput={onEdit}
            />
            {#if errors.rateLimit}<p class="app-field-error" role="alert">{errors.rateLimit}</p>{/if}
          </div>
        </div>

        <!-- SMTP 邮件发件配置 -->
        <div style="margin-top:24px;padding-top:20px;border-top:1px solid var(--color-border);">
          <h3 style="margin:0 0 12px 0;font-size:15px;display:flex;align-items:center;gap:8px;">
            <Icon name="mail" size={16} />
            SMTP 邮件发件配置
          </h3>
          <p class="app-field-help" style="margin-bottom:14px;">
            配置数据库中持久化的 SMTP 发件服务参数，供系统发送注册验证邮件及安全通知。
          </p>

          <div style="margin-bottom:14px;">
            <label class="app-check" style="margin:0;display:inline-flex;align-items:center;gap:8px;">
              <input type="checkbox" name="smtp_enabled" bind:checked={smtpEnabled} oninput={onEdit} />
              <span><b>启用 SMTP 邮件服务</b><span class="app-field-help">关闭时邮件任务在数据库任务队列中处于待发送/跳过状态</span></span>
            </label>
          </div>

          <div class="adm-form-grid">
            <div class="app-form-field" class:is-dirty={saved !== null && smtpHost !== saved.smtp_host}>
              <label class="app-field-label" for="set-smtp-host">SMTP 服务器主机</label>
              <input
                id="set-smtp-host"
                name="smtp_host"
                class="app-field"
                placeholder="smtp.example.com"
                bind:value={smtpHost}
                oninput={onEdit}
              />
            </div>

            <div class="app-form-field" class:is-dirty={saved !== null && Number(smtpPort) !== saved.smtp_port}>
              <label class="app-field-label" for="set-smtp-port">SMTP 端口</label>
              <input
                id="set-smtp-port"
                name="smtp_port"
                type="number"
                min="1"
                max="65535"
                class="app-field"
                placeholder="587"
                bind:value={smtpPort}
                oninput={onEdit}
              />
            </div>

            <div class="app-form-field" class:is-dirty={saved !== null && smtpUser !== saved.smtp_user}>
              <label class="app-field-label" for="set-smtp-user">SMTP 认证用户名</label>
              <input
                id="set-smtp-user"
                name="smtp_user"
                class="app-field"
                placeholder="user@example.com"
                bind:value={smtpUser}
                oninput={onEdit}
              />
            </div>

            <div class="app-form-field" class:is-dirty={smtpPass !== ''}>
              <label class="app-field-label" for="set-smtp-pass">
                SMTP 密码 / 授权码
                {#if smtpPassConfigured}
                  <span class="badge success sm" style="margin-left:6px;font-size:11px;">已配置</span>
                {/if}
              </label>
              <input
                id="set-smtp-pass"
                name="smtp_pass"
                type="password"
                class="app-field"
                placeholder={smtpPassConfigured ? '已配置，留空表示保持原密码' : '请输入 SMTP 授权码/密码'}
                bind:value={smtpPass}
                oninput={onEdit}
              />
            </div>

            <div class="app-form-field" class:is-dirty={saved !== null && smtpFromEmail !== saved.smtp_from_email}>
              <label class="app-field-label" for="set-smtp-from-email">发件人邮箱</label>
              <input
                id="set-smtp-from-email"
                name="smtp_from_email"
                type="email"
                class="app-field"
                placeholder="noreply@example.com"
                bind:value={smtpFromEmail}
                oninput={onEdit}
              />
            </div>

            <div class="app-form-field" class:is-dirty={saved !== null && smtpFromName !== saved.smtp_from_name}>
              <label class="app-field-label" for="set-smtp-from-name">发件人显示名称</label>
              <input
                id="set-smtp-from-name"
                name="smtp_from_name"
                class="app-field"
                placeholder="BBLBB 社区"
                bind:value={smtpFromName}
                oninput={onEdit}
              />
            </div>

            <div class="app-form-field" class:is-dirty={saved !== null && smtpEncryption !== saved.smtp_encryption}>
              <label class="app-field-label" for="set-smtp-encryption">加密模式</label>
              <select
                id="set-smtp-encryption"
                name="smtp_encryption"
                class="app-select"
                bind:value={smtpEncryption}
                onchange={onEdit}
              >
                <option value="starttls">STARTTLS（推荐，常用端口 587）</option>
                <option value="tls">SSL / TLS（常用端口 465）</option>
                <option value="none">无加密（明文）</option>
              </select>
            </div>
          </div>
          <input type="hidden" name="reason" value="系统设置更新" />
        </div>

        <!-- 原型 .app-card__foot.sys-foot：左侧状态行，右侧操作按钮行 -->
        <footer class="app-card__foot sys-foot">
          <p class="app-muted" role="status" style="margin:0;">
            {#if saving}
              正在保存，请稍候…
            {:else if dirty}
              有 <b>{changedCount}</b> 处未保存的更改
            {:else if lastSavedAt !== null}
              最近保存于 {savedAtLabel} · 配置已同步
            {:else}
              与已保存配置一致
            {/if}
          </p>
          <div class="admin-action-row">
            <Button text="恢复默认" variant="ghost" size="sm" onclick={restoreDefaults} />
            <Button
              text={saving ? '保存中…' : '保存设置'}
              variant="primary"
              size="sm"
              type="submit"
              disabled={mounted && (saving || !dirty)}
            />
          </div>
        </footer>
      </form>
    </div>
  </section>

  <!-- 数据与演示（对齐原型「数据与演示」卡片结构） -->
  <section class="app-card">
    <header class="app-card__head">
      <h2>数据与演示</h2>
      <span class="sr-only">数据与导出</span>
    </header>
    <div class="app-card__body">
      <p class="app-muted" style="line-height:1.5;">
        导出当前表单配置为 JSON（含未保存更改）；重置会清除本地演示状态（含积分、草稿、后台变更）并刷新页面。
      </p>

      <div style="margin:12px 0;">
        <label style="display:flex;align-items:center;gap:8px;font-size:13px;cursor:pointer;color:var(--color-text-secondary);">
          <input type="checkbox" onchange={(e) => showToast(e.currentTarget.checked ? '已开启保存超时模拟' : '已恢复正常模式', 'info')} />
          <span>模拟保存失败</span>
        </label>
        <span class="app-muted" style="font-size:11px;display:block;margin-top:2px;">
          开启后点击“保存设置”将模拟服务超时，用于演示失败反馈（演示开关，不随配置保存）
        </span>
      </div>

      <div class="admin-action-row" style="margin-top:14px;display:flex;align-items:center;justify-content:space-between;">
        <button type="button" class="text-link" style="font-size:13px;background:none;border:none;cursor:pointer;" onclick={exportConfig}>
          导出当前配置
        </button>
        <button type="button" class="btn secondary sm" onclick={() => showToast('本地演示数据已重置', 'success')}>
          重置演示数据
        </button>
      </div>
    </div>
  </section>
{/if}
