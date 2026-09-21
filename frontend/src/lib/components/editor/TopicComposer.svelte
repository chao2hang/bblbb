<script lang="ts">
  // M04-UI-02/03/04/05：内容编辑器
  // - M04-UI-02：Markdown 输入 / 安全预览（预览用客户端 renderSafeMarkdown，
  //   仅编辑器内展示、永不持久化为 HTML；正文发布后一律用后端 body_html）/
  //   Unicode 字数 / 服务端字段错误（422/400/409/429 映射）；
  // - M04-UI-03：登录后 1.5s 防抖自动保存草稿、beforeunload 离开提示、
  //   ?draft= 恢复、409 version_conflict diff 提示（重新加载）、删除（草稿列表页）；
  // - M04-UI-04：板块、标签（发布请求支持；草稿流尚未接线）、定时发布时间
  //   （datetime-local → 毫秒）。封面字段暂不在前端展示：附件（M6）落地前
  //   不渲染任何"敬请期待"占位，避免用户看到不可用的输入；M6 后再随
  //   AttachmentPicker 接入。
  //   原型对齐：文章/讨论类型切换已移除（产品不再区分文章类型），新内容统一
  //   以 post_type=discussion 提交（post_type 仍是数据层字段，历史内容不受影响）。
  // - M04-UI-05：可见性选项只展示后端允许等级（≤ 作者当前等级），超等级选项
  //   置灰+提示；前端篡改仍由后端 422 visibility_level_exceeds_author 拒绝。
  // - GAP-FIX 编辑器增强：Markdown 工具栏（加粗/斜体/标题/引用/代码/链接/
  //   列表 + Ctrl+B/I；无 JS 保留可读的原生编辑字段，发布动作提示需启用 JS；标签输入（逗号分隔；当前
  //   仅作预览，尚未放入发布 body）、付费可见 + 价格输入（1-1000 B币，
  //   price_coin 随发布提交）、内容摘要（summary 随发布提交）。
  // - 布局重构（Flarum 式 Composer）：单一栏位——标题 + 元信息行 + 编辑器占满
  //   主区；发布设置 / AI 辅助收进底部工具条的弹层（popover）；视频引用入口
  //   在编辑器工具栏（附件按钮旁），面板锚定在工具栏正下方（snippet 传入
  //   RichTextEditor 的 header 内渲染），不再出现在底部工具条。
  //   不再出现重复的板块/标签输入与三处草稿状态。
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import {
    listBoards,
    listTags,
    createPost,
    updatePost,
    getPost,
    createDraft,
    updateDraft,
    getDraft,
    deleteDraft,
    getAiCapabilities,
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
  import Icon from '$lib/components/ui/Icon.svelte';
  import TagItem from '$lib/components/ui/Tag.svelte';
  import RichTextEditor from '$lib/components/editor/RichTextEditor.svelte';
  import EditorAssistantPanel from '$lib/components/ai/EditorAssistantPanel.svelte';
  import VideoInsertPanel from '$lib/components/video/VideoInsertPanel.svelte';
  import { videoProviderLabel } from '$lib/video/labels';
  import { charCount } from '$lib/utils';
  import { SHEET_MEDIA_QUERY, sheetDrag } from '$lib/utils/sheet-drag';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';

  const MAX_TITLE_CHARS = 200;
  const MAX_MARKDOWN_CHARS = 50_000; // 后端 PostContent 权威上限（Unicode 字符）
  const AUTOSAVE_DEBOUNCE_MS = 1500;
  /** 内容摘要上限（GAP-FIX 编辑器增强；发布字段由后端持久化）。 */
  const MAX_SUMMARY_CHARS = 300;
  /** 付费帖子价格区间（B 币，GAP-FIX-SPEC 付费解锁：1-1000）。 */
  const PRICE_MIN = 1;
  const PRICE_MAX = 1000;

  const POLICY_OPTIONS = [
    { value: 'public', label: '公开' },
    { value: 'logged_in', label: '登录可见' },
    { value: 'after_reply', label: '整篇回复解锁' },
    { value: 'level', label: '等级可见' },
    { value: 'paid', label: '付费可见' }
  ] as const;

  // 原型对齐：产品不再区分文章/讨论类型——编辑器无类型选择，新内容统一按
  // discussion 提交（post_type 仍为契约必填字段；?draft= 恢复时草稿内容
  // 优先，见 hydrateFromDraft）。
  const POST_TYPE = 'discussion' as const;

  let title = $state('');
  let markdown = $state('');
  let selectedTags = $state<string[]>([]);
  let tagInputText = $state('');
  let tagBoxFocused = $state(false);
  const tagsInput = $derived(selectedTags.join(', '));
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
  // AI 助手入口门控：未开放（Feature Flag 关闭/管理员禁用）或无权限（未登录，
  // AI 任务接口均要求登录）时不渲染「AI 助手」入口，避免打开后只见
  // 「AI 辅助未开放」占位。能力未知/请求失败一律按不可用处理（fail-closed）。
  let aiAvailable = $state(false);

  let submitting = $state(false);
  let error = $state<Problem | null>(null);

  // ── 视频引用（M10-UI-01/02） ──
  let videoResolutions = $state<VideoResolveResult[]>([]);
  let videoNotice = $state<string | null>(null);
  /** 发布成功但视频引用有失败时的停留态（不阻塞发帖，提示外链）。 */
  let published = $state<{ id: string; videoFailed: number } | null>(null);

  // ── 编辑已有帖子状态（?post_id=<id>）──
  let editPostId = $state<string | null>(null);
  let editPostVersion = $state<number>(1);
  let editPostAuthor = $state<string | null>(null);
  let isDelegatedEdit = $derived.by(() => {
    if (!editPostId || !user) return false;
    return editPostAuthor !== user.username;
  });
  let editReason = $state('管理员更新帖子内容');

  // ── 草稿状态（M04-UI-03） ──
  let draftId = $state<string | null>(null);
  let draftVersion = $state(1);
  let draftState = $state<'idle' | 'saved' | 'saving' | 'error' | 'conflict'>('idle');
  let dirty = $state(false);
  let conflict = $state<Problem | null>(null);

  // —— step-up 重新验证（M02-MFA-07）：管理员代改命中 403 step_up_required 时弹窗 ——
  let reauthOpen = $state(false);
  let reauthPassword = $state('');
  let reauthLoading = $state(false);
  let reauthError = $state<string | null>(null);

  async function handleReauthSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!reauthPassword) return;
    reauthLoading = true;
    reauthError = null;
    try {
      const res = await fetch('/api/v1/auth/re-auth', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ password: reauthPassword })
      });
      if (res.ok) {
        reauthOpen = false;
        reauthPassword = '';
        error = null;
        // 自动重试提交编辑
        await handleSubmit();
      } else {
        const data = await res.json().catch(() => ({}));
        reauthError = data.detail || data.message || '密码验证失败，请重试';
      }
    } catch {
      reauthError = '网络错误，请稍后重试';
    } finally {
      reauthLoading = false;
    }
  }
  let restoring = $state(true); // 挂载/恢复期间抑制自动保存
  let setupLoading = $state(true);
  let setupProblem = $state<Problem | null>(null);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  /** 最近一次保存/恢复时的表单快照（与其相等不视为脏）。 */
  let lastSaved = $state('');

  const userLevel = $derived(user?.level ?? 1);
  const titleError = $derived(fieldError(error, 'title'));
  const markdownError = $derived(fieldError(error, 'markdown'));
  const boardError = $derived(fieldError(error, 'board_id'));
  const recovery = $derived(problemRecovery(error));

  function currentSnapshot(): string {
    return `${title}|${markdown}|${tagsInput}|${priceCoinInput}|${boardId}|${visibilityLevel}|${accessPolicy}|${scheduledAt}|${searchIndexOptOut}|${aiSummaryOptOut}`;
  }

  /** M04-UI-05：可见等级选项只展示到作者当前等级，超等级选项置灰。 */
  const levelOptions = $derived.by(() => {
    const max = Math.max(1, userLevel);
    return Array.from({ length: max }, (_, i) => i + 1);
  });
  const levelHint = $derived(
    accessPolicy === 'level' && user
      ? `可选的可见等级上限为你当前信任等级 TL${userLevel}`
      : null
  );

  onMount(async () => {
    try {
      // 并行拉取基础数据；getMe 决定是否启用草稿自动保存与可见等级上限。
      user = await getMe(fetch);
    userLoaded = true;
    // M04-UI-05：恢复的草稿/初始值若超出当前等级，收敛到作者等级
    // （后端仍会以 visibility_level_exceeds_author 拒绝越级提交）。
    if (user && visibilityLevel > (user.level ?? 1)) visibilityLevel = Math.max(1, user.level ?? 1);
    const [boardResult, tagResult, aiCapsResult] = await Promise.allSettled([
      listBoards(fetch),
      listTags(fetch),
      // 未登录不请求能力声明（无权限，入口保持隐藏）。
      user ? getAiCapabilities(fetch) : Promise.resolve(null)
    ]);
    if (boardResult.status === 'fulfilled') {
      boards = boardResult.value.items;
      if (!boardId && boards.length > 0) boardId = boards[0].id;
    }
      if (tagResult.status === 'fulfilled') tags = tagResult.value.items;
      // 仅登录用户且站点 AI 能力开放时展示入口；管理员禁用（admin_forbidden）视为未开放。
      if (user && aiCapsResult.status === 'fulfilled') {
        const caps = aiCapsResult.value;
        aiAvailable = caps?.enabled === true && caps.admin_forbidden !== true;
      }
      const failedSetup = [boardResult, tagResult].find((result) => result.status === 'rejected');
      if (failedSetup && failedSetup.status === 'rejected') setupProblem = failedSetup.reason as Problem;

      // ?post_id=<id> 编辑已有帖子（作者重新编辑 / 管理员代改）
    const postIdParam = page.url.searchParams.get('post_id');
    if (postIdParam) {
      try {
        const postData = await getPost(fetch, postIdParam);
        editPostId = postData.id;
        editPostVersion = postData.version ?? 1;
        editPostAuthor = postData.author?.username ?? null;
        title = postData.title;
        markdown = postData.markdown || '';
        if (postData.board_id) {
          boardId = postData.board_id;
        }
        if (postData.tags && Array.isArray(postData.tags)) {
          selectedTags = postData.tags
            .map((t: any) => (typeof t === 'string' ? t : t.name || ''))
            .filter(Boolean);
        }
        if (postData.access_summary?.policy) {
          const pol = postData.access_summary.policy;
          if (pol === 'public' || pol === 'logged_in' || pol === 'after_reply' || pol === 'level' || pol === 'paid') {
            accessPolicy = pol;
          }
        }
        lastSaved = currentSnapshot();
        dirty = false;
      } catch (err: any) {
        error = err as Problem;
      }
    } else {
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
    }
      restoring = false;
    } catch (err: unknown) {
      setupProblem = err as Problem;
    } finally {
      restoring = false;
      setupLoading = false;
    }
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
    if (draft.tags && Array.isArray(draft.tags)) {
      selectedTags = draft.tags
        .map((t: any) => (typeof t === 'string' ? t : t.name || ''))
        .filter(Boolean);
    }
    lastSaved = currentSnapshot();
    dirty = false;
    draftState = 'saved';
    conflict = null;
  }

  /** 字段变化 → 标记脏 + 防抖自动保存（仅在新建/草稿阶段，编辑已有帖子不自动覆盖草稿）。 */
  $effect(() => {
    const snapshot = currentSnapshot();
    if (restoring || !userLoaded || !user) return;
    if (editPostId) return; // 编辑已有帖子时不触发新建草稿自动保存
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

  // ── 标签 / 摘要 / 付费价格（GAP-FIX 编辑器增强） ─────────────────────────

  const parsedTags = $derived(selectedTags);

  const availableSuggestions = $derived(
    tags
      .filter((t) => !selectedTags.includes(t.name))
      .slice(0, 8)
  );

  function addTag(raw: string) {
    const parts = raw.split(/[,，]/);
    for (const part of parts) {
      const val = part.trim().replace(/^#/, '');
      if (!val || val.length > 32) continue;
      if (selectedTags.length >= 8) break;
      if (!selectedTags.includes(val)) {
        selectedTags = [...selectedTags, val];
      }
    }
    tagInputText = '';
  }

  function removeTag(tagToRemove: string) {
    selectedTags = selectedTags.filter((t) => t !== tagToRemove);
  }

  function handleTagKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',' || e.key === '，') {
      e.preventDefault(); // 关键：阻止回车提交整个发帖表单
      if (tagInputText.trim()) {
        addTag(tagInputText);
      }
    } else if (e.key === 'Backspace' && !tagInputText && selectedTags.length > 0) {
      selectedTags = selectedTags.slice(0, -1);
    }
  }

  function handleTagInput() {
    if (tagInputText.includes(',') || tagInputText.includes('，')) {
      addTag(tagInputText);
    }
  }

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
    patch.tags = selectedTags;
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
          tags: selectedTags.length > 0 ? selectedTags : undefined,
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

  async function handleSubmit(e?: SubmitEvent) {
    if (e) e.preventDefault();
    if (!user) {
      goto('/login');
      return;
    }
    if (!title.trim()) {
      error = { status: 422, code: 'validation_error', detail: '请填写标题' } as Problem;
      return;
    }
    if (!markdown.trim()) {
      error = { status: 422, code: 'validation_error', detail: '请填写内容' } as Problem;
      return;
    }
    if (!boardId) {
      error = { status: 422, code: 'validation_error', detail: '请选择板块' } as Problem;
      return;
    }
    // 若输入框残留未按回车的标签文本，自动收录
    if (tagInputText.trim()) {
      addTag(tagInputText);
    }
    // 付费价格前端校验（1-1000 B币）；不合法则停在编辑器展示 inline 错误。
    if (priceError) {
      openPanel = 'settings'; // 价格错误位于设置弹层内，展开让用户看到
      return;
    }
    submitting = true;
    error = null;
    published = null;
    // GAP-FIX 付费解锁/摘要：后端 CreatePostRequest 已接收并持久化
    // price_coin/summary（price_coin 仅在 access_policy=paid 时提交）。
    // 编辑已有帖子分支（PATCH /api/v1/posts/:id）
    if (editPostId) {
      try {
        await updatePost(
          fetch,
          editPostId,
          {
            title: title.trim(),
            markdown: markdown.trim(),
            tags: selectedTags,
            reason: isDelegatedEdit ? editReason.trim() : undefined
          },
          editPostVersion
        );
        dirty = false;
        goto(`/posts/${encodeURIComponent(editPostId)}`);
      } catch (err: unknown) {
        error = err as Problem;
        if (error?.code === 'step_up_required') {
          reauthOpen = true;
        }
      }
      submitting = false;
      return;
    }

    // 新建发布分支
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
    if (selectedTags.length > 0) input.tags = selectedTags;
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

  // ── 底部工具面板（Flarum 式弹层）：同一时刻只开一个，Esc 关闭 ──
  type PanelId = 'settings' | 'video' | 'ai';
  let openPanel = $state<PanelId | null>(null);

  function togglePanel(id: PanelId) {
    openPanel = openPanel === id ? null : id;
  }

  // 手机端断点（≤767px）：面板以 Bottom Sheet 模态呈现（docs/MOBILE-SHEET.md）。
  let isNarrow = $state(false);
  $effect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia(SHEET_MEDIA_QUERY);
    isNarrow = mq.matches;
    const onChange = (e: MediaQueryListEvent) => (isNarrow = e.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  });

  function handleCancel() {
    if (dirty) {
      if (!confirm('当前内容尚未保存，确定退出发帖吗？')) {
        return;
      }
    }
    if (editPostId) {
      goto(`/posts/${encodeURIComponent(editPostId)}`);
    } else {
      goto('/');
    }
  }

  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (openPanel) {
          openPanel = null;
        } else {
          handleCancel();
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  /** 发布设置是否偏离默认值（用于工具按钮上的小圆点提示）。 */
  const settingsTouched = $derived(
    accessPolicy !== 'public' ||
      !!scheduledAt ||
      searchIndexOptOut ||
      aiSummaryOptOut
  );
</script>

<PageTitle title={editPostId ? (isDelegatedEdit ? '管理代改内容' : '编辑内容') : '发布内容'} />

<div class="container page-content app-page linuxdo-composer-page" id="page-publish">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="composer-stage"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) {
        handleCancel();
      }
    }}
  >
    <section class="composer-window" aria-labelledby="composer-window-title">
      <header class="composer-titlebar">
        <span class="composer-titlebar__identity">
          <Icon name="pen-line" size={15} />
          <strong id="composer-window-title">{editPostId ? (isDelegatedEdit ? '管理代改' : '编辑内容') : '创建话题'}</strong>
        </span>
        <div class="composer-titlebar__meta">
          <span class="composer-draft-status {user ? 'is-active' : 'is-muted'}" role="status">{draftStateLabel ?? (user ? '草稿自动保存已开启' : '登录后保存草稿')}</span>
          <button
            type="button"
            class="composer-window-close"
            onclick={handleCancel}
            aria-label="关闭编辑器"
            title="返回社区"
          >
            <Icon name="x" size={18} />
          </button>
        </div>
      </header>

      <h1 class="sr-only" tabindex="-1">{editPostId ? '编辑内容' : '发布内容'}</h1>

      <form class="composer-form" aria-busy={submitting} onsubmit={handleSubmit}>
        <div class="composer-body">
          {#if setupLoading}
            <div class="app-notice" role="status" aria-live="polite">正在准备发布设置…</div>
          {:else if setupProblem}
            <div class="app-notice is-danger" role="alert">
              <span>{problemText(setupProblem) || '发布设置加载失败，请重试'}</span>
              <Button text="重新加载" variant="secondary" size="sm" onclick={() => window.location.reload()} />
            </div>
          {/if}

          {#if error}
            <div class="composer-notice composer-notice--danger" role="alert">
              <p>{problemText(error)}</p>
              {#if recovery.action === 'reload' || recovery.action === 'wait'}
                <Button text="重新加载" variant="secondary" size="sm" onclick={() => window.location.reload()} />
              {/if}
            </div>
          {/if}

          {#if isDelegatedEdit}
            <div class="composer-notice composer-notice--brand">
              <label class="input-label" for="edit-reason">代改原因 *（将写入管理审计日志）</label>
              <input
                type="text"
                id="edit-reason"
                class="input-field"
                bind:value={editReason}
                placeholder="例如：修正排版/移除违规敏感信息"
                required
              />
            </div>
          {/if}

          <div class="publish-title-field">
            <label for="publish-title">标题</label>
            <div class="publish-title-control">
              <input
                type="text"
                class="input-field publish-title-input"
                placeholder="话题标题…"
                bind:value={title}
                id="publish-title"
                name="title"
                required
                aria-invalid={titleError ? 'true' : undefined}
                maxlength={MAX_TITLE_CHARS}
                autocomplete="off"
              />
              <span class="publish-title-hint">{charCount(title)} / {MAX_TITLE_CHARS}</span>
            </div>
            {#if titleError}<p class="input-hint is-error" role="alert">{titleError}</p>{/if}
          </div>

          <div class="composer-meta-row">
            <div class="composer-meta-field">
              <label for="publish-board">板块</label>
              <div class="composer-select-wrapper">
                <select class="input-field" id="publish-board" name="board_id" bind:value={boardId} disabled={setupLoading || boards.length === 0}>
                  {#if boards.length === 0}<option value="">暂无可用板块</option>{/if}
                  {#each boards as board}
                    <option value={board.id}>{board.name}</option>
                  {/each}
                </select>
                <span class="composer-select-icon" aria-hidden="true">
                  <Icon name="chevron-down" size={14} />
                </span>
              </div>
              {#if boardError}<p class="input-hint is-error" role="alert">{boardError}</p>{/if}
            </div>

            <div class="composer-meta-field composer-meta-field--tags">
              <label for="publish-tags">
                标签
                <span>回车添加，最多 8 个</span>
              </label>
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <div
                class="composer-tag-input-box {tagBoxFocused ? 'is-focused' : ''}"
                role="group"
                aria-label="标签选择"
                onclick={() => {
                  const el = document.getElementById('publish-tags');
                  if (el) el.focus();
                }}
              >
                {#each selectedTags as tag (tag)}
                  <TagItem name={tag} onremove={() => removeTag(tag)} />
                {/each}
                {#if selectedTags.length < 8}
                  <input
                    type="text"
                    class="composer-tag-input"
                    id="publish-tags"
                    bind:value={tagInputText}
                    onkeydown={handleTagKeyDown}
                    oninput={handleTagInput}
                    onfocus={() => (tagBoxFocused = true)}
                    onblur={() => {
                      tagBoxFocused = false;
                      if (tagInputText.trim()) addTag(tagInputText);
                    }}
                    placeholder={selectedTags.length === 0 ? '输入标签后回车创建…' : '添加标签…'}
                    maxlength={32}
                    autocomplete="off"
                  />
                {/if}
              </div>

              {#if availableSuggestions.length > 0 && selectedTags.length < 8}
                <div class="composer-tag-suggestions" aria-label="快捷推荐标签">
                  <span class="composer-tag-suggestions__label">推荐：</span>
                  {#each availableSuggestions as tag (tag.id)}
                    <button
                      type="button"
                      class="composer-tag-suggestion-pill"
                      onclick={() => addTag(tag.name)}
                    >
                      +{tag.name}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>

          <div class="composer-editor">
            <RichTextEditor
              bind:value={markdown}
              placeholder="使用 Markdown 或富文本编写内容…（支持表格、代码块、Ctrl+B/I 等快捷键）"
              maxChars={MAX_MARKDOWN_CHARS}
              id="publish-content"
              name="markdown"
              oninsertvideo={() => togglePanel('video')}
              videoOpen={openPanel === 'video'}
              videoBadge={videoResolutions.length}
            >
              {#snippet insertPanel()}
                {#if openPanel === 'video'}
                  <div
                    class="composer-popover composer-popover--editor"
                    id="composer-panel-video"
                    role="dialog"
                    aria-label="视频引用"
                    aria-modal={isNarrow ? 'true' : undefined}
                  >
                    <div class="app-sheet-grab" aria-hidden="true" use:sheetDrag={{ onClose: () => (openPanel = null) }}></div>
                    <div class="composer-popover__head">
                      <span>视频引用</span>
                      <button type="button" class="composer-popover__close" aria-label="关闭视频引用" onclick={() => (openPanel = null)}>
                        <Icon name="x" size={14} />
                      </button>
                    </div>
                    <div class="composer-popover__body">
                      <VideoInsertPanel
                        onResolve={handleResolveVideo}
                        onAccept={acceptVideo}
                        embedded
                      />
                      {#if videoNotice}
                        <p class="input-hint is-error" role="alert">{videoNotice}</p>
                      {/if}
                      {#if videoResolutions.length > 0}
                        <div class="composer-field">
                          <span class="input-label" id="publish-video-refs-label">待发布的视频引用（{videoResolutions.length}）</span>
                          <ul class="composer-video-list" aria-labelledby="publish-video-refs-label">
                            {#each videoResolutions as videoRef (videoRef.resolution_id)}
                              <li>
                                <span class="badge badge-neutral">{videoProviderLabel(videoRef.provider)}</span>
                                <span class="composer-video-list__title">{videoRef.title ?? '视频引用'}</span>
                                {#if videoRef.embeddable === false}
                                  <span class="badge badge-warning">外链</span>
                                {/if}
                                <button type="button" class="btn btn-ghost btn-sm" onclick={() => removeVideo(videoRef.resolution_id)}>移除</button>
                              </li>
                            {/each}
                          </ul>
                          <p class="input-hint">发布时只提交 resolution_id 与允许字段；视频解析失败不影响发帖（降级为外链卡片）。</p>
                        </div>
                      {/if}
                    </div>
                  </div>
                {/if}
              {/snippet}
            </RichTextEditor>
            <div class="composer-editor__status">
              <span>{charCount(markdown)} / {MAX_MARKDOWN_CHARS}</span>
              {#if userLoaded && !user}
                <span class="composer-editor__status-right">
                  <a href="/login" class="text-link">登录</a>后自动保存草稿
                </span>
              {/if}
            </div>
            {#if markdownError}<p class="input-hint is-error" role="alert">{markdownError}</p>{/if}
          </div>

          {#if conflict && draftState === 'conflict'}
            <div class="composer-notice composer-notice--warning" role="alert">
              <p>{problemRecovery(conflict).message}（本地内容与他人已保存的草稿不同）</p>
              <span class="composer-notice__actions">
                <Button text="重新加载" variant="secondary" size="sm" onclick={reloadDraft} />
                <Button text="保留本地" variant="ghost" size="sm" onclick={() => { conflict = null; draftState = 'saved'; }} />
              </span>
            </div>
          {/if}

          {#if published}
            <div class="composer-notice composer-notice--warning" role="status">
              <p>
                <strong>帖子已发布。</strong>
                {published.videoFailed} 个视频引用创建失败（不影响帖子发布）。你可以稍后在帖子里重试，或
                <a class="text-link" href={`/posts/${published.id}`}>查看已发布的帖子</a>。
              </p>
            </div>
          {/if}
        </div>

        <footer class="composer-footer">
          <div class="composer-tools" role="group" aria-label="发布工具">
            <button
              type="button"
              class="composer-tool-btn"
              class:is-active={openPanel === 'settings'}
              aria-label="发布设置"
              aria-expanded={openPanel === 'settings'}
              aria-controls="composer-panel-settings"
              onclick={() => togglePanel('settings')}
            >
              <Icon name="settings" size={15} />
              <span>发布设置</span>
              {#if settingsTouched}<i class="composer-tool-dot" title="已调整发布设置"></i>{/if}
            </button>
            {#if aiAvailable}
              <button
                type="button"
                class="composer-tool-btn"
                class:is-active={openPanel === 'ai'}
                aria-label="AI 助手"
                aria-expanded={openPanel === 'ai'}
                aria-controls="composer-panel-ai"
                onclick={() => togglePanel('ai')}
              >
                <Icon name="sparkles" size={15} />
                <span>AI 助手</span>
              </button>
            {/if}
            <a class="composer-drafts-link" href="/me/drafts" aria-label="草稿箱">
              <Icon name="inbox" size={14} />
              <span>草稿箱</span>
            </a>
          </div>

          <div class="composer-actions">
            {#if user && !editPostId}
              <button
                type="button"
                class="btn btn-ghost"
                onclick={() => void saveDraft()}
                disabled={draftState === 'saving' || (!title.trim() && !markdown.trim())}
              >
                {draftState === 'saving' ? '保存中…' : '存草稿'}
              </button>
            {/if}
            <Button
              text={submitting ? '保存中…' : editPostId ? (isDelegatedEdit ? '保存代改内容' : '保存修改') : scheduledAt ? '定时发布' : '立即发布'}
              variant="primary"
              size="lg"
              type="submit"
              disabled={submitting}
            />
          </div>

          {#if openPanel}
            <!-- 手机端 Bottom Sheet scrim（桌面 display:none，docs/MOBILE-SHEET.md） -->
            <button type="button" class="app-sheet-backdrop" aria-label="关闭弹层" onclick={() => (openPanel = null)}></button>
          {/if}

          {#if openPanel === 'settings'}
            <div
              class="composer-popover"
              id="composer-panel-settings"
              role="dialog"
              aria-label="发布设置"
              aria-modal={isNarrow ? 'true' : undefined}
            >
              <div class="app-sheet-grab" aria-hidden="true" use:sheetDrag={{ onClose: () => (openPanel = null) }}></div>
              <div class="composer-popover__head">
                <span>发布设置</span>
                <button type="button" class="composer-popover__close" aria-label="关闭发布设置" onclick={() => (openPanel = null)}>
                  <Icon name="x" size={14} />
                </button>
              </div>
              <div class="composer-popover__body">
                <div class="composer-field">
                  <span class="input-label" id="publish-visibility-label">内容可见性</span>
                  <div class="vis-options" role="radiogroup" aria-labelledby="publish-visibility-label">
                    {#each POLICY_OPTIONS as option}
                      <label class="vis-option" class:is-active={accessPolicy === option.value}>
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
                  <p class="input-hint" style="margin: 8px 0 0; font-size: 12px; line-height: 1.5; color: var(--color-text-secondary);">
                    提示：如需在正文任意位置单独插入某一段回复后可见的内容，请保持此可见性为「公开」，并在上方编辑器工具栏点击「回复可见（锁头）」按钮插入专属区块。
                  </p>
                  {#if accessPolicy === 'level'}
                    <label class="input-label composer-field__sub" for="publish-level">最低可见等级</label>
                    <div class="composer-select-wrapper">
                      <select class="input-field" id="publish-level" bind:value={visibilityLevel}>
                        {#each levelOptions as level}
                          <option value={level} disabled={level > userLevel}>{level}（LV.{level}）</option>
                        {/each}
                      </select>
                      <span class="composer-select-icon" aria-hidden="true">
                        <Icon name="chevron-down" size={14} />
                      </span>
                    </div>
                  {/if}
                  {#if accessPolicy === 'paid'}
                    <!-- 付费可见（GAP-FIX 付费解锁）：价格 1-1000 B币，前端校验；
                         price_coin 随发布提交并由后端持久化。 -->
                    <label class="input-label composer-field__sub" for="publish-price">价格（B币）</label>
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

                <div class="composer-field">
                  <label class="input-label" for="publish-scheduled">定时发布 <span class="composer-field__opt">可选</span></label>
                  <input type="datetime-local" class="input-field" id="publish-scheduled" bind:value={scheduledAt} />
                  <p class="input-hint">留空立即发布；填写后帖子将定时公开（需晚于当前时间）。</p>
                </div>

                <div class="composer-field">
                  <span class="input-label" id="publish-index-label">索引与 AI 摘要</span>
                  <div class="composer-checks" role="group" aria-labelledby="publish-index-label">
                    <label>
                      <input type="checkbox" bind:checked={searchIndexOptOut} />
                      从搜索引擎索引中排除
                    </label>
                    <label>
                      <input type="checkbox" bind:checked={aiSummaryOptOut} />
                      不生成 AI 摘要
                    </label>
                  </div>
                  <p class="input-hint">只影响本帖在公开搜索索引与 AI 摘要中的出现；管理员全站/板块策略优先。</p>
                </div>

                <p class="composer-popover__legal">发布即表示你同意社区规范；发布后内容仍可编辑，正文以服务端渲染结果为准。</p>
              </div>
            </div>
          {:else if openPanel === 'ai' && aiAvailable}
            <!-- 入口已按能力/权限门控；此处双重保险，能力关闭时弹层一并隐藏。 -->
            <div
              class="composer-popover"
              id="composer-panel-ai"
              role="dialog"
              aria-label="AI 助手"
              aria-modal={isNarrow ? 'true' : undefined}
            >
              <div class="app-sheet-grab" aria-hidden="true" use:sheetDrag={{ onClose: () => (openPanel = null) }}></div>
              <div class="composer-popover__head">
                <span>AI 助手</span>
                <button type="button" class="composer-popover__close" aria-label="关闭 AI 助手" onclick={() => (openPanel = null)}>
                  <Icon name="x" size={14} />
                </button>
              </div>
              <div class="composer-popover__body">
                <EditorAssistantPanel
                  {draftId}
                  {title}
                  {markdown}
                  onApplyField={applyAiField}
                  onEnsureDraft={ensureDraftForAi}
                />
              </div>
            </div>
          {/if}
        </footer>
      </form>
    </section>
  </div>
</div>

<!-- step-up 重新验证（M02-MFA-07）：管理员代改命中 403 step_up_required 时展示。 -->
<Dialog
  open={reauthOpen}
  title="需要重新验证身份"
  description="管理员代改帖子属于高风险管理操作，要求近期重新认证。输入当前账号密码完成重新验证后，将自动继续保存修改。"
  onclose={() => (reauthOpen = false)}
>
  {#if reauthError}
    <div class="alert alert-danger" role="alert" style="margin-bottom:10px;padding:8px 12px;font-size:12px;">
      {reauthError}
    </div>
  {/if}
  <form
    onsubmit={handleReauthSubmit}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <div>
      <label class="input-label" for="composer-reauth-password">当前账号密码</label>
      <input
        class="input-field"
        type="password"
        id="composer-reauth-password"
        name="password"
        autocomplete="current-password"
        bind:value={reauthPassword}
        required
      />
    </div>
    <div style="display:flex;gap:8px;">
      <Button text={reauthLoading ? '验证中…' : '重新验证'} variant="primary" type="submit" disabled={reauthLoading} />
      <button type="button" class="btn ghost sm" onclick={() => (reauthOpen = false)}>取消</button>
    </div>
  </form>
</Dialog>
