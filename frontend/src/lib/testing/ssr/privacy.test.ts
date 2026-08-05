// M00-FRONTEND-09：隐私守卫（SSR 输出）—— 组件只渲染投影字段，
// 即使数据对象里混入凭据/令牌/隐藏正文，SSR HTML 也不会包含它们。
// 在 ssr vitest 项目（node 环境）中用 svelte/server 渲染得到纯 HTML。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import PostList from '$lib/components/PostList.svelte';
import BoardCard from '$lib/components/BoardCard.svelte';
import ArticleCard from '$lib/components/ArticleCard.svelte';

describe('M00-FRONTEND-09 隐私守卫：SSR HTML 不含私密字段值', () => {
  it('PostList SSR 不渲染邮箱/凭据/隐藏正文', () => {
    const { body } = render(PostList, {
      props: {
        posts: [
          {
            id: 'p1',
            title: '你好 BBLBB',
            author_name: 'alice',
            reply_count: 2,
            view_count: 10,
            created_at: 0,
            email: 'alice@example.com',
            password_hash: 'SSR-HASH-SECRET',
            session_token: 'SSR-TOKEN-SECRET',
            hidden_body: '<p>SSR-HIDDEN-BODY</p>'
          } as never
        ]
      }
    });
    expect(body).toContain('你好 BBLBB');
    expect(body).toContain('alice');
    expect(body).not.toContain('alice@example.com');
    expect(body).not.toContain('SSR-HASH-SECRET');
    expect(body).not.toContain('SSR-TOKEN-SECRET');
    expect(body).not.toContain('SSR-HIDDEN-BODY');
  });

  it('BoardCard SSR 不渲染邮箱/密钥', () => {
    const { body } = render(BoardCard, {
      props: {
        slug: 'tech',
        name: '技术讨论',
        description: '编程与开发',
        post_count: 1,
        icon: 'code',
        color: '#0088CC',
        email: 'mod@example.com',
        secret_field: 'BOARD-SSR-SECRET'
      } as never
    });
    expect(body).toContain('技术讨论');
    expect(body).not.toContain('mod@example.com');
    expect(body).not.toContain('BOARD-SSR-SECRET');
  });

  it('ArticleCard SSR 不渲染正文外的隐藏内容', () => {
    const { body } = render(ArticleCard, {
      props: {
        id: 'a1',
        title: 'Rust 入门指南',
        summary: '从所有权开始理解 Rust。',
        body_html: '<p>ARTICLE-SSR-BODY</p>',
        hidden_body: '<p>ARTICLE-SSR-HIDDEN</p>'
      } as never
    });
    expect(body).toContain('Rust 入门指南');
    expect(body).not.toContain('ARTICLE-SSR-BODY');
    expect(body).not.toContain('ARTICLE-SSR-HIDDEN');
  });
});