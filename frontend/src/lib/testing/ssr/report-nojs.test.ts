// M05-UI-09：举报表单无 JavaScript 退化——SSR 渲染出可提交表单（action +
// 全部字段 + 成功/错误状态），即使 JS 禁用也能举报。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import ReportPage from '../../../routes/moderation/report/+page.svelte';

describe('M05-UI-09 举报表单无 JS SSR', () => {
  it('渲染可提交的举报表单（action + 全部必填字段）', () => {
    const { body } = render(ReportPage, {
      props: { data: { items: [], submitted: null }, form: null }
    });
    expect(body).toMatch(/<form[^>]*action="\?\/report"/);
    expect(body).toContain('name="target_type"');
    expect(body).toContain('name="target_id"');
    expect(body).toContain('name="reason"');
    expect(body).toContain('name="detail"');
    // 原因下拉包含全部合法 reason_code（与后端 ReportReasonCode 一致）
    for (const reason of ['spam', 'harassment', 'illegal', 'nsfw', 'misinformation', 'impersonation', 'other']) {
      expect(body).toContain(`value="${reason}"`);
    }
  });

  it('提交成功渲染统一成功状态（可撤回入口）', () => {
    const { body } = render(ReportPage, {
      props: { data: { items: [] }, form: { submitted: { id: 'r1', status: 'submitted' } } }
    });
    expect(body).toContain('举报已提交');
    expect(body).toContain('r1');
    expect(body).toContain('撤回');
  });

  it('API 拒绝（自身/跨板块/非法目标）渲染稳定错误而非猜测原因', () => {
    const { body } = render(ReportPage, {
      props: { data: { items: [] }, form: { message: 'cannot report your own content' } }
    });
    expect(body).toContain('cannot report your own content');
    expect(body).toContain('举报');
  });

  it('我的举报列表渲染撤回入口（未撤回项）', () => {
    const { body } = render(ReportPage, {
      props: {
        data: {
          items: [
            { id: 'r1', target_type: 'post', target_id: 'p1', reason_code: 'spam', status: 'open', created_at: 0, updated_at: 0 },
            { id: 'r2', target_type: 'user', target_id: 'u1', reason_code: 'harassment', status: 'withdrawn', created_at: 0, updated_at: 0 }
          ],
          submitted: null
        },
        form: null
      }
    });
    // 未撤回项有撤回表单
    expect(body).toMatch(/<form[^>]*action="\?\/withdraw"/);
    // 已撤回项不再显示撤回按钮
    expect(body).not.toContain('name="report_id" value="r2"');
  });
});
