<script lang="ts">
  // M04-UI-02/03/04/05：内容编辑器
  // - M04-UI-02：Markdown 输入 / 安全预览（预览用客户端 renderSafeMarkdown，
  //   仅编辑器内展示、永不持久化为 HTML；正文发布后一律用后端 body_html）/
  //   Unicode 字数 / 服务端字段错误（422/400/409/429 映射）；
  // - M04-UI-03：登录后 1.5s 防抖自动保存草稿、beforeunload 离开提示、
  //   ?draft= 恢复、409 version_conflict diff 提示（重新加载）、删除（草稿列表页）；
  // - M04-UI-04：article/discussion 切换、板块、标签（多选，契约暂无字段 →
  //   置灰+提示）、封面（占位，附件 M6）、定时发布时间（datetime-local → 毫秒）；
  // - M04-UI-05：可见性选项只展示后端允许等级（≤ 作者当前等级），超等级选项
  //   置灰+提示；前端篡改仍由后端 422 visibility_level_exceeds_author 拒绝。
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

  const POLICY_OPTIONS = [
    { value: 'public', label: '公开' },
    { value: 'logged_in', label: '登录可见' },
    { value: 'after_reply', label: '回复解锁' },
    { value: 'level', label: '等级可见' }
  ] as const;

  let postType = $state<'article' | 'discussion'>('discussion');
  let title = $state('');
  let markdown = $state('');
  let boardId = $state('');
  let accessPolicy = $state<'public' | 'logged_in' | 'after_reply' | 'level'>('public');
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
    return `${postType}|${title}|${markdown}|${boardId}|${visibilityLevel}|${accessPolicy}|${scheduledAt}|${searchIndexOptOut}|${aiSummaryOptOut}`;
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
    postType = draft.type === 'article' ? 'article' : 'discussion';
    title = draft.title;
    markdown = draft.markdown;
    if (draft.board_id) boardId = draft.board_id;
    if (draft.visibility_level > 0) visibilityLevel = draft.visibility_level;
    if (draft.access_policy === 'public' || draft.access_policy === 'logged_in' ||
        draft.access_policy === 'after_reply' || draft.access_policy === 'level') {
      accessPolicy = draft.access_policy;
    }
    scheduledAt = msToDatetimeLocal(draft.scheduled_at);
    if (typeof draft.search_index_opt_out === 'boolean') searchIndexOptOut = draft.search_index_opt_out;
    if (typeof draft.ai_summary_opt_out === 'boolean') aiSummaryOptOut = draft.ai_summary_opt_out;
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
          type: postType,
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
    submitting = true;
    error = null;
    published = null;
    const input: PostCreateInput = {
      type: postType,
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
  <title>发布 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    {#if draftId}
      <a href="/me/drafts" class="breadcrumb-link">草稿</a>
      <span class="breadcrumb-sep">/</span>
    {/if}
    <span class="breadcrumb-current">发布新帖</span>
  </nav>

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

      <div class="card" style="margin-bottom:var(--space-4);">
        <div class="card-body" role="group" aria-label="帖子类型">
          <span class="card-title" style="display:block;margin-bottom:var(--space-2);">帖子类型</span>
          <label class="radio-inline" style="margin-right:var(--space-4);">
            <input type="radio" name="post_type" value="discussion" bind:group={postType} checked={postType === 'discussion'} />
            讨论
          </label>
          <label class="radio-inline">
            <input type="radio" name="post_type" value="article" bind:group={postType} checked={postType === 'article'} />
            文章
          </label>
        </div>
      </div>

      <div class="publish-title-field">
        <label for="publish-title">标题</label>
        <div class="publish-title-control">
          <input
            type="text"
            class="input-field publish-title-input"
            placeholder="一句话说清你想讨论什么…"
            bind:value={title}
            id="publish-title"
            maxlength={MAX_TITLE_CHARS}
            autocomplete="off"
          />
          <span class="publish-title-hint">{charCount(title)} / {MAX_TITLE_CHARS}</span>
        </div>
        {#if titleError}<p class="input-hint is-error" role="alert">{titleError}</p>{/if}
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
            <textarea
              class="editor-textarea"
              id="publish-content"
              placeholder="使用 Markdown 编写内容…"
              bind:value={markdown}
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
            <label class="input-label" for="publish-visibility">可见性</label>
            <select class="input-field" id="publish-visibility" bind:value={accessPolicy}>
              {#each POLICY_OPTIONS as option}
                <option value={option.value}>{option.label}</option>
              {/each}
              <option value="paid" disabled>付费可见（即将开放）</option>
            </select>
            {#if accessPolicy === 'level'}
              <label class="input-label" for="publish-level" style="margin-top:var(--space-2);">最低可见等级</label>
              <select class="input-field" id="publish-level" bind:value={visibilityLevel}>
                {#each levelOptions as level}
                  <option value={level} disabled={level > userLevel}>{level}（LV.{level}）</option>
                {/each}
              </select>
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
            <span class="input-label" id="publish-tags-label">标签</span>
            <div class="tag-cloud" role="group" aria-labelledby="publish-tags-label" aria-disabled="true">
              {#each tags.slice(0, 12) as tag}
                <button type="button" class="tag-chip" disabled title="标签功能将在后续版本开放（当前契约暂不支持帖内标签）">
                  {tag.name}
                </button>
              {/each}
            </div>
            <p class="input-hint">标签功能将在后续版本开放（当前契约暂不支持帖内标签）。</p>
          </div>

          <div class="input-wrapper">
            <label class="input-label" for="publish-cover">封面（可选）</label>
            <input type="file" class="input-field" id="publish-cover" disabled />
            <p class="input-hint">封面附件将在附件里程碑（M6）支持。</p>
          </div>
        </div>
      </div>

      {#if draftId}
        <a class="btn btn-secondary btn-sm" style="margin-top:var(--space-2);width:100%;text-align:center;" href="/me/drafts">查看我的草稿</a>
      {/if}
      <Button
        text={submitting ? '发布中…' : scheduledAt ? '定时发布' : '立即发布'}
        variant="primary"
        size="lg"
        type="submit"
        extraClass="btn-block"
        disabled={submitting}
      />
      <p class="input-hint" style="margin-top:var(--space-2);">
        发布即表示你同意社区规范；发布后内容仍可编辑，正文以服务端渲染结果为准。
      </p>
    </div>
  </form>
</div>
