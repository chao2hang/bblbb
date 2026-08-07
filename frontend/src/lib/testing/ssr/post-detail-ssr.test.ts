// M04-UI-09：帖子详情无 JS 公开阅读与表单合理退化 SSR 测试。
//
// 覆盖：
// 1. 公开帖：SSR 输出包含后端 body_html（正文完整可读，无 JS 也能看）；
// 2. 受限帖：即使数据里混入隐藏正文，未解锁（unlocked=false）也绝不进入
//    SSR HTML（页面级兜底；白名单在 +page.server.ts load 内再做一层）；
// 3. 已认证上下文：回复 `<form>` 标记（textarea 字段）出现在 SSR HTML；
// 4. 锁帖（closed_at 置位）：SSR 不渲染回复表单，渲染锁定提示；
// 5. load 白名单：mocked fetch 对抗性响应 → 白名单只留公开字段，且 body_html
//    仅在 access_summary.unlocked=true 时挑选（M04-VISIBILITY-07）。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import PostPage from '../../../routes/posts/[id]/+page.svelte';
import { load } from '../../../routes/posts/[id]/+page.server';
import type { PostDetailPageData } from '../../../routes/posts/[id]/+page.server';

const publicPost: PostDetailPageData = {
  post: {
    id: 'post-1',
    post_type: 'discussion',
    title: '公开讨论：无 JS 可读',
    status: 'published',
    author: { id: 'u1', username: 'alice', display_name: '爱丽丝', level: 3 },
    access_summary: { policy: 'public', unlocked: true },
    capabilities: [],
    reply_count: 2,
    view_count: 10,
    created_at: 0,
    updated_at: 0,
    body_html: '<p>这是公开正文，仅来自后端 body_html。</p>'
  },
  authed: false,
  error: null
};

const restrictedData: PostDetailPageData = {
  post: {
    id: 'post-2',
    post_type: 'article',
    title: '受限内容',
    status: 'hidden',
    author: { id: 'u2', username: 'bob' },
    access_summary: { policy: 'level', unlocked: false, required_level: 5 },
    capabilities: [],
    reply_count: 0,
    view_count: 3,
    created_at: 0,
    updated_at: 0,
    // 对抗性：即使数据对象混入隐藏正文，页面也不得渲染（unlocked=false）。
    body_html: '<p>RESTRICTED-BODY-CANARY 隐藏正文</p>'
  },
  authed: false,
  error: null
};

const lockedData: PostDetailPageData = {
  post: {
    id: 'post-3',
    post_type: 'discussion',
    title: '已锁定讨论',
    status: 'published',
    author: { id: 'u1', username: 'alice' },
    access_summary: { policy: 'public', unlocked: true },
    capabilities: [],
    reply_count: 5,
    view_count: 20,
    created_at: 0,
    updated_at: 0,
    closed_at: 1700000000000,
    body_html: '<p>正文可读但已锁帖。</p>'
  },
  authed: true,
  error: null
};

describe('M04-UI-09 帖子详情无 JS 公开阅读（SSR）', () => {
  it('公开帖：SSR 输出标题/作者/后端 body_html，正文完整可读', () => {
    const { body } = render(PostPage, { props: { data: publicPost } });
    expect(body).toContain('公开讨论：无 JS 可读');
    expect(body).toContain('爱丽丝');
    expect(body).toContain('这是公开正文，仅来自后端 body_html。');
    expect(body).toContain('2 回复');
  });

  it('受限帖：unlocked=false 时隐藏正文不进入 SSR HTML，渲染可访问占位', () => {
    const { body } = render(PostPage, { props: { data: restrictedData } });
    expect(body).not.toContain('RESTRICTED-BODY-CANARY');
    expect(body).not.toContain('隐藏正文');
    expect(body).toContain('LV.5');
    expect(body).toContain('内容需达到');
  });

  it('未认证且未锁帖：渲染"登录后即可回复"而非回复表单', () => {
    const { body } = render(PostPage, { props: { data: publicPost } });
    expect(body).toContain('登录后即可回复');
    expect(body).not.toContain('name="comment-markdown"');
  });
});

describe('M04-UI-07 可见性可访问占位：hidden/after_reply/level/paid 不把正文放入 DOM', () => {
  // 每种策略构造对抗性数据：即使 body_html 混入隐藏正文，unlocked=false 时
  // 页面只渲染占位文案，正文/隐藏正文绝不进入 SSR HTML。
  const policyCases: Array<{
    policy: 'logged_in' | 'after_reply' | 'level' | 'paid';
    required_level?: number;
    expectText: string;
  }> = [
    { policy: 'logged_in', expectText: '内容仅对登录用户开放' },
    { policy: 'after_reply', expectText: '回复后可解锁剩余内容' },
    { policy: 'level', required_level: 7, expectText: '内容需达到 LV.7 后开放' },
    { policy: 'paid', expectText: '付费内容，解锁后可查看' }
  ];

  for (const c of policyCases) {
    it(`[${c.policy}] 占位可访问且隐藏正文不进入 DOM`, () => {
      const data: PostDetailPageData = {
        post: {
          id: `post-${c.policy}`,
          post_type: 'article',
          title: `${c.policy} 受限内容`,
          status: 'published',
          author: { id: 'u1', username: 'alice' },
          access_summary: {
            policy: c.policy,
            unlocked: false,
            ...(c.required_level !== undefined ? { required_level: c.required_level } : {})
          },
          capabilities: [],
          reply_count: 0,
          view_count: 1,
          created_at: 0,
          updated_at: 0,
          // 对抗性：即使投影层漏放正文，页面也不得渲染（双重防线）。
          body_html: `<p>UI-07-${c.policy.toUpperCase()}-CANARY-隐藏正文</p>`
        },
        authed: false,
        error: null
      };
      const { body } = render(PostPage, { props: { data } });
      expect(body).toContain(c.expectText);
      expect(body).toContain(`可见性：`);
      expect(body).not.toContain(`UI-07-${c.policy.toUpperCase()}-CANARY`);
      expect(body).not.toContain('隐藏正文');
      expect(body).toContain('正文不可见');
    });
  }
});

describe('M04-UI-09 回复表单合理退化（SSR）', () => {
  it('已认证上下文（authed=true）：SSR 输出回复 <form> 与 textarea 字段', () => {
    const { body } = render(PostPage, {
      props: {
        data: {
          ...publicPost,
          authed: true
        }
      }
    });
    expect(body).toMatch(/<form[^>]*method="POST"/);
    expect(body).toContain('发表回复');
    expect(body).toContain('<textarea');
    expect(body).toContain('id="comment-input"');
  });

  it('锁帖（closed_at 置位）：SSR 输出锁定提示，不渲染回复表单', () => {
    const { body } = render(PostPage, { props: { data: lockedData } });
    expect(body).toContain('该帖已锁定，不能继续回复。');
    expect(body).toContain('已锁定');
    expect(body).toContain('正文可读但已锁帖。');
    expect(body).not.toMatch(/<form[^>]*method="POST"/);
    expect(body).not.toContain('id="comment-input"');
  });
});

describe('M04-UI-01/+page.server 公开字段白名单（对抗性响应）', () => {
  function jsonResponse(data: unknown, status = 200): Response {
    return new Response(JSON.stringify(data), {
      status,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  function mockFetch(data: unknown, status = 200) {
    const fn = vi.fn(async () => jsonResponse(data, status));
    vi.stubGlobal('fetch', fn);
    return fn;
  }

  it('公开帖：load 输出白名单字段，body_html 随 unlocked=true 保留', async () => {
    mockFetch({
      id: 'post-1',
      post_type: 'discussion',
      title: '公开标题',
      status: 'published',
      author: { id: 'u1', username: 'alice', display_name: '爱丽丝', level: 3 },
      access_summary: { policy: 'public', unlocked: true },
      capabilities: ['comment.create'],
      reply_count: 2,
      view_count: 10,
      created_at: 1700000000000,
      updated_at: 1700000000000,
      body_html: '<p>公开正文</p>',
      excerpt: '公开摘要',
      board_id: 'board-1',
      slug: 'public-title'
    });
    const data = (await load({
      params: { id: 'post-1' },
      cookies: { get: () => null },
      request: { headers: new Headers() }
    } as never)) as PostDetailPageData;
    const post = data.post!;
    expect(post.id).toBe('post-1');
    expect(post.title).toBe('公开标题');
    expect(post.body_html).toBe('<p>公开正文</p>');
    expect(post.author).toEqual({ id: 'u1', username: 'alice', display_name: '爱丽丝', level: 3 });
    expect(post.access_summary).toEqual({ policy: 'public', unlocked: true });
    // 白名单外字段不泄漏。
    expect('excerpt' in post).toBe(false);
    expect('board_id' in post).toBe(false);
    expect('slug' in post).toBe(false);
  });

  it('受限帖：unlocked=false 时 body_html 不被挑选（防隐藏正文泄漏）', async () => {
    mockFetch({
      id: 'post-2',
      title: '受限标题',
      status: 'hidden',
      access_summary: { policy: 'level', unlocked: false, required_level: 5 },
      created_at: 1700000000000,
      updated_at: 1700000000000,
      body_html: '<p>CANARY-RESTRICTED-BODY</p>'
    });
    const data = (await load({
      params: { id: 'post-2' },
      cookies: { get: () => null },
      request: { headers: new Headers() }
    } as never)) as PostDetailPageData;
    expect(data.post).not.toBeNull();
    expect(data.post!.body_html).toBeUndefined();
    // 隐藏正文 canary 不得出现在任何输出字段。
    expect(JSON.stringify(data)).not.toContain('CANARY-RESTRICTED-BODY');
  });

  it('404 → load 抛错（不渲染详情）', async () => {
    mockFetch({ status: 404, detail: 'post not found' }, 404);
    await expect(
      load({
        params: { id: 'missing' },
        cookies: { get: () => null },
        request: { headers: new Headers() }
      } as never)
    ).rejects.toMatchObject({ status: 404 });
  });

  it('会话 Cookie 存在时 authed=true（SSR 回复表单提示）', async () => {
    mockFetch({ id: 'post-1', title: '标题', created_at: 0, updated_at: 0 });
    const data = (await load({
      params: { id: 'post-1' },
      cookies: { get: (name: string) => (name === '__Host-bblbb_session' ? 'sess-1' : null) },
      request: { headers: new Headers() }
    } as never)) as PostDetailPageData;
    expect(data.authed).toBe(true);
  });
});
