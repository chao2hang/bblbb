// M00-FRONTEND-08：无 JavaScript 基线 —— SSR 输出可读性测试。
// 在 ssr vitest 项目（node 环境）中用 svelte/server 渲染得到纯 HTML 字符串，
// 模拟禁用 JS 后浏览器看到的内容。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import PostList from '$lib/components/PostList.svelte';
import BoardCard from '$lib/components/BoardCard.svelte';
import ArticleCard from '$lib/components/ArticleCard.svelte';
import NoJsNotice from '$lib/components/ui/NoJsNotice.svelte';
import LoadingState from '$lib/components/ui/LoadingState.svelte';

describe('无 JS 基线：公开阅读 SSR 可读', () => {
  it('PostList SSR HTML 包含帖子标题与作者', () => {
    const { body } = render(PostList, {
      props: {
        posts: [
          {
            id: 'p1',
            title: '你好 BBLBB',
            author_name: 'alice',
            reply_count: 2,
            view_count: 10,
            created_at: 0
          }
        ]
      }
    });
    expect(body).toContain('你好 BBLBB');
    expect(body).toContain('alice');
  });

  it('BoardCard SSR HTML 包含板块名与描述', () => {
    const { body } = render(BoardCard, {
      props: {
        slug: 'tech',
        name: '技术讨论',
        description: '编程与开发',
        post_count: 1,
        icon: 'code',
        color: '#0088CC'
      }
    });
    expect(body).toContain('技术讨论');
    expect(body).toContain('编程与开发');
  });

  it('ArticleCard SSR HTML 包含文章标题', () => {
    const { body } = render(ArticleCard, {
      props: {
        id: 'a1',
        title: 'Rust 入门指南',
        summary: '从所有权开始理解 Rust。'
      }
    });
    expect(body).toContain('Rust 入门指南');
    expect(body).toContain('从所有权开始理解 Rust。');
  });

  it('LoadingState SSR HTML 播报加载状态（初始态可感知）', () => {
    const { body } = render(LoadingState, { props: { title: '加载中…' } });
    expect(body).toContain('加载中…');
    expect(body).toContain('role="status"');
  });
});

describe('无 JS 基线：关键表单给服务端可理解退化', () => {
  it('NoJsNotice 的降级文案进入 SSR HTML（<noscript>）', () => {
    const { body } = render(NoJsNotice, {
      props: { message: '登录需要启用 JavaScript。' }
    });
    expect(body).toContain('<noscript>');
    expect(body).toContain('需要启用 JavaScript');
    expect(body).toContain('登录需要启用 JavaScript。');
  });
});