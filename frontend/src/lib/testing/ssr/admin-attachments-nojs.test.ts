// M18-ADMIN-OPS（约定 D）：附件管理页 SSR 快照——行「删除」=「⋮」三点菜单触发
// 按钮（aria-label 含行语义）；菜单关闭态不渲染列表项；删除走常驻 SSR 隐藏表单
// （id/reason 审计要素）+ DangerConfirm（关闭态不渲染）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminAttachmentsPage from '../../../routes/admin/attachments/+page.svelte';
import type { AdminAttachmentsPageData } from '../../../routes/admin/attachments/+page.server';

const okData: AdminAttachmentsPageData = {
  state: 'ok',
  items: [
    {
      id: 'att-1',
      filename: 'incident-report.pdf',
      uploader_username: 'alice',
      size_bytes: 2048,
      created_at: 1700000000000
    }
  ],
  nextCursor: null,
  q: '',
  after: null,
  error: null
};

describe('M18-ADMIN-OPS 附件管理 SSR（约定 D）', () => {
  it('ok → 列表行渲染 + 行「⋮」菜单触发按钮 + 隐藏删除表单（审计要素）', () => {
    const { body } = render(AdminAttachmentsPage, { props: { data: okData, form: null } });
    // 列表行完整渲染
    expect(body).toContain('incident-report.pdf');
    expect(body).toContain('alice');
    // 约定 D：行「删除」=「⋮」菜单触发按钮（aria-label 含行语义）
    expect(body).toContain('aria-label="更多操作：附件 incident-report.pdf"');
    // 菜单关闭态不渲染列表项「删除附件」（DangerConfirm 标题同文案，关闭态也不渲染）
    expect(body).not.toContain('删除附件');
    // 隐藏删除表单常驻 SSR（id/reason 审计要素）；批量删除 Dialog 未选中时不渲染
    expect(body).toContain('action="?/delete"');
    expect(body).toContain('name="reason"');
    expect(body).not.toMatch(/<form[^>]*action="\?\/batchDelete"/);
    // 搜索为 GET 表单（SSR 基线保留）+ 选择列 aria 标签（批量契约）
    expect(body).toContain('aria-label="搜索当前列表"');
    expect(body).toContain('aria-label="选中附件 incident-report.pdf"');
  });

  it('403 → 无权限态', () => {
    const { body } = render(AdminAttachmentsPage, {
      props: {
        data: { state: 'forbidden', items: null, nextCursor: null, q: '', after: null, error: 'forbidden' },
        form: null
      }
    });
    expect(body).toContain('无权限');
    expect(body).not.toContain('incident-report.pdf');
  });

  it('空状态 → 提示无附件', () => {
    const { body } = render(AdminAttachmentsPage, {
      props: { data: { state: 'ok', items: [], nextCursor: null, q: '', after: null, error: null }, form: null }
    });
    expect(body).toContain('暂无附件');
  });

  it('合规与风控治理能力：渲染行内附件预览与快速封禁上传者入口', () => {
    const { body } = render(AdminAttachmentsPage, { props: { data: okData, form: null } });
    // 渲染预览快捷入口
    expect(body).toContain('预览');
    // 渲染封禁恶意上传者快捷入口
    expect(body).toContain('快速封禁上传者：alice');
    expect(body).toContain('封禁');
    // 渲染封禁用户表单（action="?/banUser"）
    expect(body).toContain('action="?/banUser"');
    expect(body).toContain('name="username"');
  });

  it('403 step_up_required → 渲染 re-auth 密码确认弹窗（M02-MFA-07，不再只报错）', () => {
    const { body } = render(AdminAttachmentsPage, {
      props: {
        data: okData,
        form: {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        }
      }
    });
    expect(body).toContain('需要重新验证身份');
    expect(body).toContain('name="password"');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/reauth"/);
    expect(body).toContain('重新验证');
  });

  it('form 为 null → 不渲染 re-auth 弹窗', () => {
    const { body } = render(AdminAttachmentsPage, { props: { data: okData, form: null } });
    expect(body).not.toContain('需要重新验证身份');
    expect(body).not.toMatch(/<form[^>]*action="\?\/reauth"/);
  });
});
