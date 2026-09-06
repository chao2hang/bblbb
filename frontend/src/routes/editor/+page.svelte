<script lang="ts">
  // M04-UI-02/03/04/05：内容编辑器
  // - M04-UI-02：Markdown 输入 / 安全预览（预览用客户端 renderSafeMarkdown，
  //   仅编辑器内展示、永不持久化为 HTML；正文发布后一律用后端 body_html）/
  //   Unicode 字数 / 服务端字段错误（422/400/409/429 映射）；
  // - M04-UI-03：登录后 1.5s 防抖自动保存草稿、beforeunload 离开提示、
  //   ?draft= 恢复、409 version_conflict diff 提示（重新加载）、删除（草稿列表页）；
  // - M04-UI-04：板块、标签（多选，契约暂无字段 → 置灰+提示）、封面（占位，
  //   附件 M6）、定时发布时间（datetime-local → 毫秒）。
  //   原型对齐：文章/讨论类型切换已移除（产品不再区分文章类型），新内容统一
  //   以 post_type=discussion 提交（post_type 仍是数据层字段，历史内容不受影响）。
  // - M04-UI-05：可见性选项只展示后端允许等级（≤ 作者当前等级），超等级选项
  //   置灰+提示；前端篡改仍由后端 422 visibility_level_exceeds_author 拒绝。
  // - GAP-FIX 编辑器增强：Markdown 工具栏（加粗/斜体/标题/引用/代码/链接/
  //   列表 + Ctrl+B/I，无 JS 降级为纯 textarea）、标签输入（逗号分隔；后端
  //   CreatePostRequest 暂无 tags 字段 → 提交时静默丢弃 TODO）、付费可见 +
  //   价格输入（1-1000 B币，price_coin 随发布提交）、内容摘要（summary，
  //   后端字段落地前随发布提交）。
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import {
    listBoards,
    listTags,
    createPost,
    createDraft,
    updateDraft,
    getDraft,
    deleteDraft,
    getMe,
    resolveVideoEmbed,
    createVideoEmbed,
    newClientRequestId,
    type Board,
    type Tag,
    type User,
    type Draft,
    type PostCreateInput,
    type DraftCreateInput,
    type DraftPatchInput,
    type VideoResolveResult
  } from '$lib/api/client';
  import {
    problemText,
    problemRecovery,
    fieldError,
    type Problem
  } from '$lib/errors';
  import Button from '$lib/components/ui/Button.svelte';
  import SafeHtml from '$lib/components/SafeHtml.svelte';
  import EditorAssistantPanel from '$lib/components/ai/EditorAssistantPanel.svelte';
  import VideoInsertPanel from '$lib/components/video/VideoInsertPanel.svelte';
  import { videoProviderLabel } from '$lib/video/labels';
  import { renderSafeMarkdown, charCount } from '$lib/utils';

  const MAX_TITLE_CHARS = 200;
  const MAX_MARKDOWN_CHARS = 50_000; // 后端 PostContent 权威上限（Unicode 字符）
  const AUTOSAVE_DEBOUNCE_MS = 1500;
  /** 内容摘要上限（GAP-FIX 编辑器增强；后端字段落地前仅随发布提交）。 */
  const MAX_SUMMARY_CHARS = 300;
  /** 付费帖子价格区间（B 币，GAP-FIX-SPEC 付费解锁：1-1000）。 */
  const PRICE_MIN = 1;
  const PRICE_MAX = 1000;

  const POLICY_OPTIONS = [
    { value: 'public', label: '公开' },
    { value: 'logged_in', label: '登录可见' },
    { value: 'after_reply', label: '回复解锁' },
    { value: 'level', label: '等级可见' },
    { value: 'paid', label: '付费可见' }
  ] as const;

  // 原型对齐：产品不再区分文章/讨论类型——编辑器无类型选择，新内容统一按
  // discussion 提交（post_type 仍为契约必填字段；?draft= 恢复时草稿内容
  // 优先，见 hydrateFromDraft）。
  const POST_TYPE = 'discussion' as const;

  let title = $state('');
  let markdown = $state('');
  let summaryInput = $state('');
  let tagsInput = $state('');
  let priceCoinInput = $state('');
  let boardId = $state('');
  let accessPolicy = $state<'public' | 'logged_in' | 'after_reply' | 'level' | 'paid'>('public');
  let visibilityLevel = $state(1);
  let scheduledAt = $state('');
  // M08-INDEX-03：作者逐帖退出搜索索引 / AI 摘要（管理员全站/板块策略优先）。
  let searchIndexOptOut = $state(false);
  let aiSummaryOptOut = $state(false);

  let boards = $state<Board[]>([]);
  let tags = $state<Tag[]>([]);
  let user = $state<User | null>(null);
  let userLoaded = $state(false);

  let previewMode = $state(false);
  let submitting = $state(false);
  let error = $state<Problem | null>(null);

  // ── Markdown 工具栏（GAP-FIX 编辑器增强） ──
  // 需要Selection API（selectionStart/End + setRangeText），无 JS 环境
  // 不可用 → 按钮栏挂载后才渲染（textarea 本身始终可直接输入）。
  let toolbarMounted = $state(false);
  let editorEl = $state<HTMLTextAreaElement | undefined>(undefined);

  // ── 视频引用（M10-UI-01/02） ──
  let videoResolutions = $state<VideoResolveResult[]>([]);
  let videoNotice = $state<string | null>(null);
  /** 发布成功但视频引用有失败时的停留态（不阻塞发帖，提示外链）。 */
  let published = $state<{ id: string; videoFailed: number } | null>(null);

  // ── 草稿状态（M04-UI-03） ──
  let draftId = $state<string | null>(null);
  let draftVersion = $state(1);
  let draftState = $state<'idle' | 'saved' | 'saving' | 'error' | 'conflict'>('idle');
  let dirty = $state(false);
  let conflict = $state<Problem | null>(null);
  let restoring = $state(true); // 挂载/恢复期间抑制自动保存
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  /** 最近一次保存/恢复时的表单快照（与其相等不视为脏）。 */
  let lastSaved = $state('');

  const userLevel = $derived(user?.level ?? 1);
  const titleError = $derived(fieldError(error, 'title'));
  const markdownError = $derived(fieldError(error, 'markdown'));
  const boardError = $derived(fieldError(error, 'board_id'));
  const recovery = $derived(problemRecovery(error));

  function currentSnapshot(): string {
    return `${title}|${markdown}|${boardId}|${visibilityLevel}|${accessPolicy}|${scheduledAt}|${searchIndexOptOut}|${aiSummaryOptOut}`;
  }

  /** M04-UI-05：可见等级选项只展示到作者当前等级，超等级选项置灰。 */
  const levelOptions = $derived.by(() => {
    const max = Math.max(1, userLevel);
    return Array.from({ length: max }, (_, i) => i + 1);
  });
  const levelHint = $derived(
    accessPolicy === 'level' && user
      ? `可选的可见等级上限为你当前等级 LV.${userLevel}`
      : null
  );

  onMount(async () => {
    // Markdown 工具栏依赖 Selection API：仅在浏览器挂载后才渲染按钮
    // （无 JS 环境按钮不出现，textarea 直接可用）。
    toolbarMounted = true;
    // 并行拉取基础数据；getMe 决定是否启用草稿自动保存与可见等级上限。
    user = await getMe(fetch);
    userLoaded = true;
    // M04-UI-05：恢复的草稿/初始值若超出当前等级，收敛到作者等级
    // （后端仍会以 visibility_level_exceeds_author 拒绝越级提交）。
    if (user && visibilityLevel > (user.level ?? 1)) visibilityLevel = Math.max(1, user.level ?? 1);
    const [boardResult, tagResult] = await Promise.allSettled([listBoards(fetch), listTags(fetch)]);
    if (boardResult.status === 'fulfilled') {
      boards = boardResult.value.items;
      if (!boardId && boards.length > 0) boardId = boards[0].id;
    }
    if (tagResult.status === 'fulfilled') tags = tagResult.value.items;

    // ?draft=<id> 恢复草稿（M04-UI-03 恢复流程）。
    const draftParam = page.url.searchParams.get('draft');
    if (draftParam) {
      try {
        const draft = await getDraft(fetch, draftParam);
        hydrateFromDraft(draft);
      } catch {
        draftState = 'error';
      }
    }
    restoring = false;
  });

  function hydrateFromDraft(draft: Draft) {
    draftId = draft.id;
    draftVersion = draft.version;
    title = draft.title;
    markdown = draft.markdown;
    if (draft.board_id) boardId = draft.board_id;
    if (draft.visibility_level > 0) visibilityLevel = draft.visibility_level;
    if (draft.access_policy === 'public' || draft.access_policy === 'logged_in' ||
        draft.access_policy === 'after_reply' || draft.access_policy === 'level' ||
        draft.access_policy === 'paid') {
      accessPolicy = draft.access_policy;
    }
    scheduledAt = msToDatetimeLocal(draft.scheduled_at);
    if (typeof draft.search_index_opt_out === 'boolean') searchIndexOptOut = draft.search_index_opt_out;
    if (typeof draft.ai_summary_opt_out === 'boolean') aiSummaryOptOut = draft.ai_summary_opt_out;
    // 摘要/标签/价格暂不随草稿保存（DraftCreate/Patch 无对应字段；后端
    // GAP-FIX BE-2a 落地前仅随发布提交，见 handleSubmit 内 TODO 注释）。
    lastSaved = currentSnapshot();
    dirty = false;
    draftState = 'saved';
    conflict = null;
  }

  /** 字段变化 → 标记脏 + 防抖自动保存（登录时，且与上次保存快照不同）。 */
  $effect(() => {
    const snapshot = currentSnapshot();
    if (restoring || !userLoaded || !user) return;
    if (!title.trim() && !markdown.trim()) return;
    if (snapshot === lastSaved) return;
    dirty = true;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveDraft, AUTOSAVE_DEBOUNCE_MS);
  });

  /** 离开提示：有未保存修改时 beforeunload 拦截。 */
  $effect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  });

  // ── Markdown 工具栏（GAP-FIX 编辑器增强） ────────────────────────────────

  /** 把 [start,end) 替换为 replacement 并同步 state（setRangeText 不触发
   *  input 事件，需手动回写 bind:value 绑定的 markdown）。 */
  function applyEdit(
    start: number,
    end: number,
    replacement: string,
    caret: number,
    caretEnd?: number
  ): void {
    if (!editorEl) return;
    const el = editorEl;
    el.focus();
    el.setRangeText(replacement, start, end);
    el.setSelectionRange(caret, caretEnd ?? caret);
    markdown = el.value;
  }

  /** 选区包裹语法（如 **加粗**）；无选区时插入占位词并选中，便于直接输入。 */
  function wrapSelection(prefix: string, suffix: string, placeholder: string): void {
    if (!editorEl) return;
    const start = editorEl.selectionStart ?? 0;
    const end = editorEl.selectionEnd ?? 0;
    const selected = markdown.slice(start, end) || placeholder;
    applyEdit(
      start,
      end,
      `${prefix}${selected}${suffix}`,
      start + prefix.length,
      start + prefix.length + selected.length
    );
  }

  /** 行前缀语法（标题/引用/列表）：对选区覆盖的所有行加前缀。 */
  function prefixLines(prefix: string): void {
    if (!editorEl) return;
    const start = editorEl.selectionStart ?? 0;
    const end = editorEl.selectionEnd ?? start;
    const from = markdown.lastIndexOf('\n', Math.max(0, start - 1)) + 1;
    let to = markdown.indexOf('\n', end);
    if (to === -1) to = markdown.length;
    const replaced = markdown
      .slice(from, to)
      .split('\n')
      .map((line) => prefix + line)
      .join('\n');
    applyEdit(from, to, replaced, from + replaced.length);
  }

  /** 链接：[选中文字](url)——选中文字作链接文本，光标落在 URL 处。 */
  function insertLink(): void {
    if (!editorEl) return;
    const start = editorEl.selectionStart ?? 0;
    const end = editorEl.selectionEnd ?? 0;
    const selected = markdown.slice(start, end) || '链接文字';
    const replacement = `[${selected}](https://)`;
    const urlStart = start + selected.length + 3;
    applyEdit(start, end, replacement, urlStart, urlStart + 8);
  }

  /** M18-EDITOR-01：表格插入模板（对齐原型工具栏）。 */
  function insertTable(): void {
    if (!editorEl) return;
    const tableTemplate = '\n| 标题 1 | 标题 2 |\n| ------ | ------ |\n| 内容 1 | 内容 2 |\n';
    wrapSelection(tableTemplate, '', '');
  }

  /** 编辑器内快捷键：Ctrl/Cmd+B 加粗、Ctrl/Cmd+I 斜体。 */
  function handleEditorKeydown(event: KeyboardEvent): void {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    const key = event.key.toLowerCase();
    if (key === 'b') {
      event.preventDefault();
      wrapSelection('**', '**', '加粗文字');
    } else if (key === 'i') {
      event.preventDefault();
      wrapSelection('*', '*', '斜体文字');
    }
  }

  // ── 标签 / 摘要 / 付费价格（GAP-FIX 编辑器增强） ─────────────────────────

  /** 逗号（中英文都支持）分隔标签输入 → 提交用的数组（最多 8 个）。 */
  const parsedTags = $derived(
    tagsInput
      .split(/[,，]/)
      .map((t) => t.trim())
      .filter(Boolean)
      .slice(0, 8)
  );

  /** 付费价格校验：access_policy=paid 时必填，1-1000 整数（B 币）。 */
  const priceError = $derived.by(() => {
    if (accessPolicy !== 'paid') return null;
    const raw = priceCoinInput.trim();
    if (!raw) return `付费帖子需设定价格（${PRICE_MIN}-${PRICE_MAX} B币）`;
    const n = Number(raw);
    if (!Number.isInteger(n) || n < PRICE_MIN || n > PRICE_MAX) {
      return `价格需为 ${PRICE_MIN}-${PRICE_MAX} 之间的整数（B币）`;
    }
    return null;
  });

  function patchInput(): DraftPatchInput {
    const patch: DraftPatchInput = {
      title: title.trim() || undefined,
      markdown: markdown.trim() || undefined
    };
    if (boardId) patch.board_id = boardId;
    patch.visibility_level = visibilityLevel;
    patch.access_policy = accessPolicy;
    patch.search_index_opt_out = searchIndexOptOut;
    patch.ai_summary_opt_out = aiSummaryOptOut;
    const sched = scheduledToMs(scheduledAt);
    patch.scheduled_at = sched; // null 清除定时
    return patch;
  }

  async function saveDraft() {
    draftState = 'saving';
    try {
      if (draftId) {
        const updated = await updateDraft(fetch, draftId, patchInput(), draftVersion);
        draftVersion = updated.version;
      } else {
        const input: DraftCreateInput = {
          type: POST_TYPE,
          title: title.trim() || '未命名草稿',
          markdown: markdown.trim() || '（空草稿）',
          visibility_level: visibilityLevel,
          access_policy: accessPolicy,
          search_index_opt_out: searchIndexOptOut,
          ai_summary_opt_out: aiSummaryOptOut,
          client_request_id: newClientRequestId()
        };
        if (boardId) input.board_id = boardId;
        const sched = scheduledToMs(scheduledAt);
        if (sched !== null) input.scheduled_at = sched;
        const created = await createDraft(fetch, input);
        draftId = created.id;
        draftVersion = created.version;
      }
      lastSaved = currentSnapshot();
      dirty = false;
      draftState = 'saved';
      conflict = null;
    } catch (err: unknown) {
      const p = err as Problem;
      if (p.code === 'version_conflict') {
        draftState = 'conflict';
        conflict = p;
      } else {
        draftState = 'error';
      }
    }
  }

  /** 409 version_conflict：重新加载服务端草稿并覆盖本地（M04-UI-03 diff 提示）。 */
  async function reloadDraft() {
    if (!draftId) return;
    try {
      const draft = await getDraft(fetch, draftId);
      hydrateFromDraft(draft);
    } catch {
      conflict = { status: 0, code: 'version_conflict', detail: '草稿加载失败，请稍后重试' } as Problem;
    }
  }

  // ── AI 辅助（M09-UI-02/03/04/07） ────────────────────────────────────────
  // 确保草稿存在并返回 id（AI 格式化目标）。调用方先保存当前内容。
  async function ensureDraftForAi(): Promise<string | null> {
    if (!user) return null;
    if (title.trim() || markdown.trim()) {
      if (dirty && !restoring) await saveDraft();
      return draftId;
    }
    return null;
  }

  /** 字段级采纳：只更新本地表单（保存仍走草稿/发布流），绝不静默改写。 */
  function applyAiField(field: string, value: string) {
    if (field === 'title') {
      title = value;
    } else if (field === 'content' || field === 'markdown') {
      markdown = value;
    }
    // summary/tags 当前编辑器无对应字段，忽略（保持只读展示）。
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!user) {
      goto('/login');
      return;
    }
    if (!title.trim() || !markdown.trim() || !boardId) return;
    // 付费价格前端校验（1-1000 B币）；不合法则停在编辑器展示 inline 错误。
    if (priceError) return;
    submitting = true;
    error = null;
    published = null;
    // GAP-FIX 付费解锁：price_coin 随发布提交（access_policy=paid 时）。
    // TODO(BE-2a)：后端 CreatePostRequest 暂未接收 price_coin/summary 字段
    // （grep backend/src/routes/posts.rs 确认；serde 默认忽略未知字段，
    // 提交不报错），后端按 GAP-FIX-SPEC 落地后本字段即生效。
    const input: PostCreateInput & { price_coin?: number; summary?: string } = {
      type: POST_TYPE,
      title: title.trim(),
      markdown: markdown.trim(),
      board_id: boardId,
      visibility_level: visibilityLevel,
      access_policy: accessPolicy,
      search_index_opt_out: searchIndexOptOut,
      ai_summary_opt_out: aiSummaryOptOut,
      scheduled_at: scheduledToMs(scheduledAt),
      client_request_id: newClientRequestId()
    };
    if (accessPolicy === 'paid') input.price_coin = Number(priceCoinInput.trim());
    // 原型对齐：摘要对所有内容开放（不再限文章类型）。
    if (summaryInput.trim()) input.summary = summaryInput.trim();
    // 标签输入（tags）：TODO(BE-2a) 后端 CreatePostRequest 暂无 tags 字段，
    // 提交时静默丢弃（parsedTags 仅作输入预览），后端落地后改为随 body 提交。
    try {
      const result = await createPost(fetch, input);
      // 视频引用（M10-UI-02）：只提交 resolution_id + 允许字段；创建失败
      // 不阻塞发帖（VIDEO-PLUGIN.md §3）——有失败时留在编辑器提示并给出
      // 帖子的外链，用户可稍后重试或使用外链。
      let videoFailed = 0;
      if (videoResolutions.length > 0) {
        const withPolicy = videoResolutions.filter((r) => typeof r.policy_version === 'number');
        // 缺少策略版本的引用无法创建（expected_policy_version 必填），计入失败。
        videoFailed += videoResolutions.length - withPolicy.length;
        const settled = await Promise.allSettled(
          withPolicy.map((r) =>
            createVideoEmbed(fetch, {
              resolution_id: r.resolution_id,
              target_type: 'post',
              target_id: result.id,
              expected_policy_version: r.policy_version as number
            })
          )
        );
        videoFailed += settled.filter((s) => s.status === 'rejected').length;
      }
      // 发布成功即清掉草稿（best-effort）。
      if (draftId) {
        try {
          await deleteDraft(fetch, draftId);
        } catch {
          // 忽略清理失败：帖子已发布，草稿残留不影响。
        }
      }
      if (videoFailed > 0) {
        published = { id: result.id, videoFailed };
      } else {
        goto(`/posts/${result.id}`);
      }
    } catch (err: unknown) {
      error = err as Problem;
    }
    submitting = false;
  }

  // ── 视频引用（M10-UI-01/02） ────────────────────────────────────────────
  /** 触发后端 resolve（面板负责投影白名单挑选）。解析要求登录：401 由面板
   *  展示登录提示。 */
  async function handleResolveVideo(url: string): Promise<unknown> {
    return resolveVideoEmbed(fetch, url, 'post');
  }

  function acceptVideo(result: VideoResolveResult) {
    videoNotice = null;
    videoResolutions = [...videoResolutions, result];
  }

  function removeVideo(resolutionId: string) {
    videoResolutions = videoResolutions.filter((r) => r.resolution_id !== resolutionId);
  }

  /** datetime-local 输入 → Unix 毫秒（后端 scheduled_at 实现为毫秒，见报告）。 */
  function scheduledToMs(value: string): number | null {
    if (!value) return null;
    const ms = new Date(value).getTime();
    return Number.isFinite(ms) ? ms : null;
  }

  /** Unix 毫秒 → datetime-local 字符串（草稿恢复用）。 */
  function msToDatetimeLocal(ms: number | null | undefined): string {
    if (!ms) return '';
    const d = new Date(ms);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  const draftStateLabel = $derived.by(() => {
    switch (draftState) {
      case 'saving':
        return '草稿保存中…';
      case 'saved':
        return dirty ? '草稿有未保存修改' : '草稿已保存';
      case 'error':
        return '草稿保存失败';
      case 'conflict':
        return '草稿版本冲突';
      default:
        return null;
    }
  });
</script>

<svelte:head>
  <title>发布内容 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <!-- 原型对齐（prototype/pages/publish.html）：位置导航用 topic-context + sr-only h1，不用 .breadcrumb。 -->
  <nav class="topic-context" aria-label="发布位置">
    <a href="/">首页</a>
    <span aria-hidden="true">/</span>
    <a href="/me">我的</a>
    <span aria-hidden="true">/</span>
    <span class="topic-context__current">发布内容</span>
  </nav>
  <h1 class="sr-only" tabindex="-1">发布内容</h1>

  <form class="publish-layout" onsubmit={handleSubmit}>
    <div class="publish-main">
      {#if error}
        <div class="card" role="alert" style="border-color:var(--color-danger);margin-bottom:var(--space-4);">
          <div class="card-body" style="display:flex;gap:var(--space-2);align-items:flex-start;">
            <p style="margin:0;flex:1;">{problemText(error)}</p>
            {#if recovery.action === 'reload' || recovery.action === 'wait'}
              <Button text="重新加载" variant="secondary" size="sm" onclick={() => window.location.reload()} />
            {/if}
          </div>
        </div>
      {/if}

      <div class="publish-title-field">
        <label for="publish-title">标题</label>
        <div class="publish-title-control">
          <input
            type="text"
            class="input-field publish-title-input"
            placeholder="一句话说清你想表达什么…"
            bind:value={title}
            id="publish-title"
            maxlength={MAX_TITLE_CHARS}
            autocomplete="off"
          />
          <span class="publish-title-hint">{charCount(title)} / {MAX_TITLE_CHARS}</span>
        </div>
        {#if titleError}<p class="input-hint is-error" role="alert">{titleError}</p>{/if}
      </div>

      <!-- 内容摘要（GAP-FIX 编辑器增强）：原型对齐，对所有内容开放
           （文章类型移除后不再条件显示）。
           TODO(BE-2a)：后端暂无 summary 字段（grep posts.rs 确认），随发布
           提交、当前被忽略；落地后用于列表/搜索摘要展示。 -->
      <div class="publish-title-field">
        <label for="publish-summary">摘要（可选）</label>
        <textarea
          class="input-field"
          id="publish-summary"
          placeholder="一两句话概括内容，将展示在列表与搜索结果中…"
          bind:value={summaryInput}
          rows="2"
          maxlength={MAX_SUMMARY_CHARS}
        ></textarea>
        <p class="input-hint">最多 {MAX_SUMMARY_CHARS} 字；留空由系统自动截取。</p>
      </div>

      <div class="card">
        <div class="card-body" style="padding:0;">
          {#if previewMode}
            <div class="prose editor-preview" style="padding:var(--space-4);min-height:200px;">
              <!-- 仅编辑器内预览：客户端渲染本地 Markdown，永不持久化为 HTML。
                   发布后正文一律使用后端 body_html（M04-UI-01/02）。 -->
              <SafeHtml html={renderSafeMarkdown(markdown) || '<p class="text-tertiary">（空内容）</p>'} />
            </div>
          {:else}
            {#if toolbarMounted}
              <!-- Markdown 工具栏：依赖 Selection API（onMount 后才渲染），
                   无 JS 环境不出现，textarea 仍可直接书写 Markdown。 -->
              <div
                class="editor-toolbar"
                role="toolbar"
                aria-label="Markdown 格式化"
                style="display:flex;flex-wrap:wrap;gap:var(--space-1);padding:var(--space-2) var(--space-4);border-bottom:var(--border-default);"
              >
                <button type="button" class="btn btn-ghost btn-sm" title="加粗（Ctrl+B）" aria-label="加粗" onclick={() => wrapSelection('**', '**', '加粗文字')}><strong>B</strong></button>
                <button type="button" class="btn btn-ghost btn-sm" title="斜体（Ctrl+I）" aria-label="斜体" onclick={() => wrapSelection('*', '*', '斜体文字')}><em>I</em></button>
                <button type="button" class="btn btn-ghost btn-sm" title="标题（行前加 ##）" aria-label="标题" onclick={() => prefixLines('## ')}>H2</button>
                <button type="button" class="btn btn-ghost btn-sm" title="引用（行前加 >）" aria-label="引用" onclick={() => prefixLines('> ')}>&ldquo;&rdquo;</button>
                <button type="button" class="btn btn-ghost btn-sm" title="行内代码" aria-label="行内代码" onclick={() => wrapSelection('`', '`', '代码')}>&lt;/&gt;</button>
                <button type="button" class="btn btn-ghost btn-sm" title="链接" aria-label="链接" onclick={insertLink}>🔗</button>
                <button type="button" class="btn btn-ghost btn-sm" title="无序列表（行前加 -）" aria-label="无序列表" onclick={() => prefixLines('- ')}>•&mdash;</button>
                <!-- M18-EDITOR-01：表格工具按钮（对齐原型） -->
                <button type="button" class="btn btn-ghost btn-sm" title="插入表格" aria-label="表格" onclick={insertTable}>⊞</button>
              </div>
            {/if}
            <textarea
              class="editor-textarea"
              id="publish-content"
              placeholder="使用 Markdown 编写内容…（Ctrl+B 加粗 / Ctrl+I 斜体）"
              bind:value={markdown}
              bind:this={editorEl}
              onkeydown={handleEditorKeydown}
              rows="16"
              maxlength={MAX_MARKDOWN_CHARS}
            ></textarea>
          {/if}
          <div style="display:flex;align-items:center;gap:var(--space-2);padding:var(--space-2) var(--space-4);border-top:var(--border-default);">
            <span class="text-tertiary" style="font-size:var(--text-xs);">{charCount(markdown)} / {MAX_MARKDOWN_CHARS}</span>
            <Button text={previewMode ? '编辑' : '预览'} variant="ghost" size="sm" type="button" onclick={() => (previewMode = !previewMode)} />
            {#if !userLoaded}
              <span class="text-tertiary" style="font-size:var(--text-xs);margin-left:auto;">登录后自动保存草稿…</span>
            {:else if !user}
              <span class="text-tertiary" style="font-size:var(--text-xs);margin-left:auto;">
                <a href="/login" class="text-link">登录</a>后自动保存草稿
              </span>
            {:else if draftStateLabel}
              <span class="text-tertiary" style="font-size:var(--text-xs);margin-left:auto;" role="status">{draftStateLabel}</span>
            {/if}
          </div>
        </div>
        {#if markdownError}<p class="input-hint is-error" role="alert" style="padding:0 var(--space-4);">{markdownError}</p>{/if}
      </div>

      {#if conflict && draftState === 'conflict'}
        <div class="card" role="alert" style="border-color:var(--color-warning);margin-top:var(--space-4);">
          <div class="card-body" style="display:flex;gap:var(--space-2);align-items:flex-start;">
            <p style="margin:0;flex:1;">{problemRecovery(conflict).message}（本地内容与他人已保存的草稿不同）</p>
            <Button text="重新加载" variant="secondary" size="sm" onclick={reloadDraft} />
            <Button text="保留本地" variant="ghost" size="sm" onclick={() => { conflict = null; draftState = 'saved'; }} />
          </div>
        </div>
      {/if}
    </div>

    <div class="publish-sidebar">
      <div class="card">
        <div class="card-header"><span class="card-title">发布设置</span></div>
        <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
          <div class="input-wrapper">
            <label class="input-label" for="publish-board">板块</label>
            <select class="input-field" id="publish-board" bind:value={boardId}>
              {#each boards as board}
                <option value={board.id}>{board.name}</option>
              {/each}
            </select>
            {#if boardError}<p class="input-hint is-error" role="alert">{boardError}</p>{/if}
          </div>

          <div class="input-wrapper">
            <span class="input-label">内容可见性</span>
            <!-- M18-EDITOR-02：4 选项卡（对齐原型 radio 卡片设计） -->
            <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(130px,1fr));gap:var(--space-2);margin-top:var(--space-1);">
              {#each POLICY_OPTIONS as option}
                <label
                  style="display:flex;align-items:center;gap:6px;padding:8px 10px;border:1px solid {accessPolicy === option.value ? 'var(--color-brand)' : 'var(--color-border)'};border-radius:var(--radius-sm);background:{accessPolicy === option.value ? 'var(--color-bg-subtle)' : 'transparent'};cursor:pointer;font-size:var(--text-sm);user-select:none;"
                >
                  <input
                    type="radio"
                    name="access_policy_radio"
                    value={option.value}
                    checked={accessPolicy === option.value}
                    onchange={() => (accessPolicy = option.value)}
                  />
                  <span>{option.label}</span>
                </label>
              {/each}
            </div>
            {#if accessPolicy === 'level'}
              <label class="input-label" for="publish-level" style="margin-top:var(--space-3);">最低可见等级</label>
              <select class="input-field" id="publish-level" bind:value={visibilityLevel}>
                {#each levelOptions as level}
                  <option value={level} disabled={level > userLevel}>{level}（LV.{level}）</option>
                {/each}
              </select>
            {/if}
            {#if accessPolicy === 'paid'}
              <!-- 付费可见（GAP-FIX 付费解锁）：价格 1-1000 B币，前端校验；
                   price_coin 随发布提交（后端字段见上方 TODO 注释）。 -->
              <label class="input-label" for="publish-price" style="margin-top:var(--space-3);">价格（B币）</label>
              <input
                type="number"
                class="input-field"
                id="publish-price"
                bind:value={priceCoinInput}
                min={PRICE_MIN}
                max={PRICE_MAX}
                step="1"
                inputmode="numeric"
                placeholder="1-1000"
                aria-invalid={priceError ? 'true' : undefined}
              />
              {#if priceError}
                <p class="input-hint is-error" role="alert">{priceError}</p>
              {:else}
                <p class="input-hint">读者需支付 {PRICE_MIN}-{PRICE_MAX} B币解锁正文；余额不足时无法解锁。</p>
              {/if}
            {/if}
            {#if !user}
              <p class="input-hint">登录后可见等级选项按你的等级启用；越级提交仍会被服务端拒绝。</p>
            {:else if levelHint}
              <p class="input-hint">{levelHint}</p>
            {/if}
          </div>

          <div class="input-wrapper">
            <label class="input-label" for="publish-scheduled">定时发布（可选）</label>
            <input type="datetime-local" class="input-field" id="publish-scheduled" bind:value={scheduledAt} />
            <p class="input-hint">留空立即发布；填写后帖子将定时公开（需晚于当前时间）。</p>
          </div>

          <div class="input-wrapper">
            <span class="input-label" id="publish-index-label">索引与 AI 摘要</span>
            <div style="display:flex;flex-direction:column;gap:var(--space-2);" role="group" aria-labelledby="publish-index-label">
              <label class="input-label" style="display:flex;align-items:center;gap:var(--space-2);">
                <input type="checkbox" bind:checked={searchIndexOptOut} />
                从搜索引擎索引中排除
              </label>
              <label class="input-label" style="display:flex;align-items:center;gap:var(--space-2);">
                <input type="checkbox" bind:checked={aiSummaryOptOut} />
                不生成 AI 摘要
              </label>
            </div>
            <p class="input-hint">逐帖退出（M08-INDEX-03）：只影响本帖子在公开搜索索引与 AI 摘要中的出现；管理员全站/板块策略优先于你的选择。</p>
          </div>

          <VideoInsertPanel
            onResolve={handleResolveVideo}
            onAccept={acceptVideo}
          />

          {#if videoNotice}
            <p class="input-hint is-error" role="alert" style="margin:0;">{videoNotice}</p>
          {/if}
          {#if videoResolutions.length > 0}
            <div class="input-wrapper" style="display:flex;flex-direction:column;gap:var(--space-2);">
              <span class="input-label" id="publish-video-refs-label">待发布的视频引用（{videoResolutions.length}）</span>
              <ul
                style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-2);"
                aria-labelledby="publish-video-refs-label"
              >
                {#each videoResolutions as videoRef (videoRef.resolution_id)}
                  <li style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-2);display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
                    <span class="badge badge-neutral">{videoProviderLabel(videoRef.provider)}</span>
                    <span class="text-secondary" style="font-size:var(--text-sm);flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
                      {videoRef.title ?? '视频引用'}
                    </span>
                    {#if videoRef.embeddable === false}
                      <span class="badge badge-warning">外链</span>
                    {/if}
                    <button type="button" class="btn btn-ghost btn-sm" onclick={() => removeVideo(videoRef.resolution_id)}>移除</button>
                  </li>
                {/each}
              </ul>
              <p class="input-hint" style="margin:0;">发布时只提交 resolution_id 与允许字段；视频解析失败不影响发帖（降级为外链卡片）。</p>
            </div>
          {/if}

          {#if published}
            <div class="card" role="status" style="border-color:var(--color-warning);">
              <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
                <strong>帖子已发布</strong>
                <p class="input-hint" style="margin:0;">
                  {published.videoFailed} 个视频引用创建失败（不影响帖子发布）。你可以稍后在帖子里重试，或直接用外链打开：
                  <a class="text-link" href={`/posts/${published.id}`}>查看已发布的帖子</a>。
                </p>
              </div>
            </div>
          {/if}

          <EditorAssistantPanel
            {draftId}
            {title}
            {markdown}
            onApplyField={applyAiField}
            onEnsureDraft={ensureDraftForAi}
          />

          <div class="input-wrapper">
            <!-- 标签输入（GAP-FIX 编辑器增强）：逗号分隔（中英文逗号均可）。
                 TODO(BE-2a)：后端 CreatePostRequest 暂无 tags 字段（grep
                 backend/src/routes/posts.rs 确认），提交时静默丢弃；后端落地
                 后在 handleSubmit 中随 body 一并提交。 -->
            <label class="input-label" for="publish-tags">标签（可选）</label>
            <input
              type="text"
              class="input-field"
              id="publish-tags"
              bind:value={tagsInput}
              placeholder="逗号分隔，如：前端, Svelte, 教程"
              list="publish-tags-options"
              autocomplete="off"
            />
            <datalist id="publish-tags-options">
              {#each tags.slice(0, 30) as tag}
                <option value={tag.name}></option>
              {/each}
            </datalist>
            {#if parsedTags.length > 0}
              <div class="tag-cloud" style="margin-top:var(--space-2);" aria-label="已输入的标签">
                {#each parsedTags as tag}
                  <span class="tag-chip">{tag}</span>
                {/each}
              </div>
              <p class="input-hint">已识别 {parsedTags.length} 个标签（最多 8 个，超出部分忽略）；标签功能上线前仅作记录。</p>
            {:else}
              <p class="input-hint">多个标签用逗号分隔；标签功能上线后将展示在帖子与标签聚合页。</p>
            {/if}
          </div>

          <div class="input-wrapper">
            <label class="input-label" for="publish-cover">封面（可选）</label>
            <input type="file" class="input-field" id="publish-cover" disabled />
            <p class="input-hint">封面附件将在附件里程碑（M6）支持。</p>
          </div>
        </div>
      </div>

      <!-- M18-EDITOR-03：页脚操作区对齐原型（保存草稿 + 立即发布 + 草稿箱链接） -->
      <div style="display:flex;gap:var(--space-2);margin-top:var(--space-3);align-items:center;">
        {#if user}
          <button
            type="button"
            class="btn btn-secondary"
            style="flex:1;"
            onclick={() => void saveDraft()}
            disabled={draftState === 'saving' || (!title.trim() && !markdown.trim())}
          >
            {draftState === 'saving' ? '保存中…' : '保存草稿'}
          </button>
        {/if}
        <div style={user ? 'flex:2;' : 'width:100%;'}>
          <Button
            text={submitting ? '发布中…' : scheduledAt ? '定时发布' : '立即发布'}
            variant="primary"
            size="lg"
            type="submit"
            extraClass="btn-block"
            disabled={submitting}
          />
        </div>
      </div>
      <div style="display:flex;justify-content:space-between;align-items:center;margin-top:var(--space-2);">
        <span class="text-tertiary" style="font-size:var(--text-xs);">
          {draftStateLabel ?? '草稿自动保存已开启'}
        </span>
        <a class="text-link" style="font-size:var(--text-xs);" href="/me/drafts">进入草稿箱 →</a>
      </div>
      <p class="input-hint" style="margin-top:var(--space-2);">
        发布即表示你同意社区规范；发布后内容仍可编辑，正文以服务端渲染结果为准。
      </p>
    </div>
  </form>
</div>
