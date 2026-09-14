// M18-ADMIN-CONTENT：内容审核页 SSR 快照（版本对比数据驱动审核操作 + 无 JS 退化）。
// T-REVIEW-TABLE 更新：待审队列改为与其他管理页一致的 app-table 表格（?post= 原生
// 链接切换 + 当前行 aria-current 高亮）；「版本对比」详情卡片保留：修改 → 修改前/后
// 双栏差别；首次提交 → 「无先前版本」说明 + 类型徽章。
// 约定 A/D 保留：审核写操作 = 「⋮」三点菜单（通过/驳回两项；diff 不可用时菜单项
// 禁用）→ 点菜单项打开该动作的 Dialog（单一 reason；formaction 提交到既有
// ?/approve / ?/reject）。菜单关闭态与弹层关闭时菜单项/chips/表单/理由输入均不进
// SSR HTML；diff 不可用禁用占位保留。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminContentPage from '../../../routes/admin/content/+page.svelte';
import type { AdminContentPageData } from '../../../routes/admin/content/+page.server';

const pendingPost = {
  id: 'p-201',
  title: '使用 SvelteKit 构建博客与轻量论坛是否合理？',
  author_username: 'chaos',
  board_name: '站务',
  status: 'draft',
  review_status: 'pending_review',
  created_at: 1700000000000
};

const secondPost = {
  id: 'p-202',
  title: '第二篇待审帖：附件上传体验反馈',
  author_username: 'alice',
  board_name: '讨论区',
  status: 'draft',
  review_status: 'pending_review',
  created_at: 1700000000001
};

const okData: AdminContentPageData = {
  state: 'ok',
  error: null,
  diff_error: null,
  posts: [pendingPost, secondPost],
  current: pendingPost,
  diff: {
    from_version: 1,
    to_version: 2,
    reason: '补充 OIDC 接入说明',
    before_body: '正文第一版：SSR、SEO、CSRF 与表单接入。',
    after_body: '正文第二版：SSR、SEO、CSRF、表单与 OIDC 接入。'
  }
};

describe('M18-ADMIN-CONTENT 内容审核 SSR', () => {
  it('diff 可用 → 表格队列 + 修改前/后对比块 + 类型徽章，无禁用告警', () => {
    const { body } = render(AdminContentPage, { props: { data: okData, form: null } });
    // 表格队列：与其他管理页同款 app-table，当前行高亮
    expect(body).toContain('aria-label="待审队列"');
    expect(body).toContain('aria-current="page"');
    expect(body).toContain('href="?post=p-202#review-detail"');
    // 版本对比（修改：v1 → v2 双栏差别）
    expect(body).toContain('版本对比 · 使用 SvelteKit 构建博客与轻量论坛是否合理？');
    expect(body).toContain('修改前 · v1');
    expect(body).toContain('修改后 · v2');
    expect(body).toContain('正文第一版：SSR、SEO、CSRF 与表单接入。');
    expect(body).toContain('正文第二版：SSR、SEO、CSRF、表单与 OIDC 接入。');
    expect(body).toContain('v1 → v2');
    expect(body).toContain('修订说明：补充 OIDC 接入说明');
    expect(body).toContain('新增');
    expect(body).toContain('删除');
    expect(body).toContain('变更');
    expect(body).not.toContain('当前无法进行版本对比');
    // 审核操作放行 = 「⋮」三点菜单 + 弹层（约定 D）：触发按钮渲染（行语义 aria-label），
    // 菜单项（通过/驳回）与弹层内写表单均不进 SSR HTML。
    expect(body).toContain('aria-label="更多操作：待审帖 使用 SvelteKit 构建博客与轻量论坛是否合理？"');
    expect(body).not.toContain('row-actions__menu');
    expect(body).not.toContain('通过审核');
    expect(body).not.toContain('>驳回<');
    expect(body).not.toContain('action="?/approve"');
    expect(body).not.toContain('action="?/reject"');
    expect(body).not.toContain('review-entry--disabled');
  });

  it('首次提交（仅 v1）→ 类型徽章「首次提交」+ 修改前空态说明，对比可用', () => {
    const firstSubmission: AdminContentPageData = {
      ...okData,
      diff: {
        from_version: null,
        to_version: 1,
        reason: null,
        before_body: null,
        after_body: '首发正文内容。'
      }
    };
    const { body } = render(AdminContentPage, { props: { data: firstSubmission, form: null } });
    expect(body).toContain('首次提交');
    expect(body).toContain('（无先前版本，全部内容为新增）');
    expect(body).toContain('首发正文内容。');
    expect(body).not.toContain('当前无法进行版本对比');
  });

  it('diff 不可用 → 告警横幅 + 审核菜单禁用（避免按错误内容审批）；队列仍可切换', () => {
    const noDiff: AdminContentPageData = {
      ...okData,
      diff: null,
      diff_error: '该帖子没有任何修订快照，无法生成版本对比'
    };
    const { body } = render(AdminContentPage, { props: { data: noDiff, form: null } });
    expect(body).toContain('当前无法进行版本对比');
    expect(body).toContain('该帖子没有任何修订快照，无法生成版本对比');
    // 「⋮」菜单入口渲染但两项禁用（disabled 菜单项）+ 禁用占位提示
    //（弹层不放行，避免按错误内容审批；菜单关闭态 SSR 只见触发按钮与占位）。
    expect(body).toContain('aria-label="更多操作：待审帖 使用 SvelteKit 构建博客与轻量论坛是否合理？"');
    expect(body).not.toContain('row-actions__menu');
    expect(body).toContain('review-entry--disabled');
    expect(body).toContain('需要真实版本对比数据后才能审核');
    expect(body).not.toContain('action="?/reject"');
    expect(body).not.toContain('正文第二版');
    // 队列表格不受影响：仍可点「审核」切换到其他待审帖
    expect(body).toContain('aria-label="待审队列"');
    expect(body).toContain('href="?post=p-202#review-detail"');
  });

  it('待审队列 >1 篇 → 表格行 ?post= 原生链接切换（无 JS 可用），当前行高亮', () => {
    const { body } = render(AdminContentPage, { props: { data: okData, form: null } });
    expect(body).toContain('aria-label="待审队列"');
    expect(body).toContain('href="?post=p-202#review-detail"');
    expect(body).toContain('aria-current="page"');
    expect(body).toContain('第二篇待审帖：附件上传体验反馈');
  });

  it('单篇待审 → 队列表格仍渲染（1 行），详情区可用', () => {
    const single: AdminContentPageData = { ...okData, posts: [pendingPost] };
    const { body } = render(AdminContentPage, { props: { data: single, form: null } });
    expect(body).toContain('aria-label="待审队列"');
    expect(body).toContain('href="?post=p-201#review-detail"');
    expect(body).toContain('版本对比 · 使用 SvelteKit 构建博客与轻量论坛是否合理？');
  });

  it('审核写操作 = 「⋮」菜单 + 弹层：approve/reject 表单不进 SSR，理由必填移入弹层', () => {
    const { body } = render(AdminContentPage, { props: { data: okData, form: null } });
    // 新契约（约定 D）：唯一「⋮」三点入口（行语义 aria-label）；菜单项（通过/驳回）
    // 在菜单关闭态不渲染；写表单与理由输入收进弹层（Dialog 关闭时不渲染）。
    expect(body).toContain('aria-label="更多操作：待审帖 使用 SvelteKit 构建博客与轻量论坛是否合理？"');
    expect(body).not.toContain('row-actions__menu');
    expect(body).not.toContain('通过审核');
    expect(body).not.toContain('>驳回<');
    // 写表单与理由输入收进弹层（Dialog 关闭时不渲染）
    expect(body).not.toContain('action="?/approve"');
    expect(body).not.toContain('action="?/reject"');
    expect(body).not.toContain('name="reason"');
  });

  it('403 → 错误提示明确，不渲染空态/待审数据', () => {
    const forbidden: AdminContentPageData = {
      state: 'forbidden',
      posts: [],
      current: null,
      diff: null,
      diff_error: null,
      error: '没有审核权限'
    };
    const { body } = render(AdminContentPage, { props: { data: forbidden, form: null } });
    expect(body).toContain('没有审核权限');
    expect(body).not.toContain('没有待审核的内容');
    expect(body).not.toContain('使用 SvelteKit 构建博客与轻量论坛是否合理？');
  });

  it('空队列 → 空态提示，不渲染队列表格', () => {
    const empty: AdminContentPageData = {
      state: 'ok',
      posts: [],
      current: null,
      diff: null,
      diff_error: null,
      error: null
    };
    const { body } = render(AdminContentPage, { props: { data: empty, form: null } });
    expect(body).toContain('没有待审核的内容');
    expect(body).not.toContain('aria-label="待审队列"');
  });
});
