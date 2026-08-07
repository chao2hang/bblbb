// M09-UI-02/03/04/07：编辑器 AI 辅助面板组件测试。
//
// - 能力声明关闭/失败 → 降级提示（普通发帖不受影响）；
// - 无同意 → 先展示 ConsentPanel，确认后 grantAiConsent 再格式化；
// - 格式化结果只以 diff 预览呈现；字段级采纳触发 accept + onApplyField；
// - 409 版本冲突 → 冲突提示；处理中可取消任务。
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import EditorAssistantPanel from './EditorAssistantPanel.svelte';
import * as client from '$lib/api/client';
import type { AiCapabilities, AiSuggestion } from '$lib/api/types';

vi.mock('$lib/api/client', () => ({
  getAiCapabilities: vi.fn(),
  grantAiConsent: vi.fn(),
  revokeAiConsent: vi.fn(),
  requestDraftFormat: vi.fn(),
  getAiTask: vi.fn(),
  cancelAiTask: vi.fn(),
  getAiSuggestion: vi.fn(),
  acceptAiSuggestion: vi.fn(),
  newClientRequestId: vi.fn(() => 'req-key-0000000000000001'),
  aiPurposeLabel: (p: string) => p,
  aiDataModeLabel: (m: string | null | undefined) => m ?? 'none'
}));

const mocked = client as unknown as {
  getAiCapabilities: ReturnType<typeof vi.fn>;
  grantAiConsent: ReturnType<typeof vi.fn>;
  revokeAiConsent: ReturnType<typeof vi.fn>;
  requestDraftFormat: ReturnType<typeof vi.fn>;
  getAiTask: ReturnType<typeof vi.fn>;
  cancelAiTask: ReturnType<typeof vi.fn>;
  getAiSuggestion: ReturnType<typeof vi.fn>;
  acceptAiSuggestion: ReturnType<typeof vi.fn>;
};

const enabledCaps = (overrides: Partial<AiCapabilities> = {}): AiCapabilities => ({
  enabled: true,
  data_mode: 'full_with_consent',
  purposes: ['formatting'],
  providers: [
    { id: 'prov-1', name: '测试提供商', secret_configured: true, available: true, retention: '不保存', training: '不用于训练' }
  ],
  ...overrides
});

const suggestion: AiSuggestion = {
  id: 's-1',
  type: 'formatting',
  status: 'pending',
  base_version: 5,
  created_at: 0,
  fields: [
    { field: 'title', current: '旧标题', proposed: '新标题', selectable: true },
    { field: 'markdown', current: '旧正文', proposed: '新正文', selectable: true }
  ]
};

const baseProps = {
  draftId: 'draft-1',
  title: '旧标题',
  markdown: '旧正文',
  onApplyField: vi.fn(),
  onEnsureDraft: vi.fn(async () => 'draft-1')
};

beforeEach(() => vi.clearAllMocks());

describe('M09-UI-07 降级', () => {
  it('能力声明失败/关闭 → 显示降级提示，不渲染 AI 操作', async () => {
    mocked.getAiCapabilities.mockResolvedValueOnce(null);
    const { container, queryByRole } = render(EditorAssistantPanel, { props: baseProps });
    await waitFor(() => expect(container.textContent).toContain('AI 辅助未开放。发布与编辑不受影响。'));
    expect(queryByRole('button', { name: 'AI 格式化（diff 预览）' })).toBeNull();
  });

  it('能力声明 enabled=false → 同降级提示', async () => {
    mocked.getAiCapabilities.mockResolvedValueOnce({ enabled: false });
    const { container, queryByRole } = render(EditorAssistantPanel, { props: baseProps });
    await waitFor(() => expect(container.textContent).toContain('AI 辅助未开放。发布与编辑不受影响。'));
    expect(queryByRole('button', { name: 'AI 格式化（diff 预览）' })).toBeNull();
  });
});

describe('M09-UI-02/03 同意流程', () => {
  it('已同意 → 直接可格式化（不展示同意面板）', async () => {
    mocked.getAiCapabilities.mockResolvedValueOnce(
      enabledCaps({
        consents: [
          { provider_id: 'prov-1', purpose: 'formatting', data_mode: 'full_with_consent', disclosure_version: 1 }
        ]
      })
    );
    mocked.requestDraftFormat.mockResolvedValueOnce({
      task_id: 'task-1',
      status: 'queued',
      poll_url: '/ai/tasks/task-1',
      suggestion
    });
    const { findByRole, findByText } = render(EditorAssistantPanel, { props: baseProps });
    const button = await findByRole('button', { name: 'AI 格式化（diff 预览）' });
    await fireEvent.click(button);
    await waitFor(() => expect(mocked.requestDraftFormat).toHaveBeenCalledWith(expect.anything(), 'draft-1', 'req-key-0000000000000001'));
    await findByText('新标题');
    expect(mocked.grantAiConsent).not.toHaveBeenCalled();
  });

  it('无同意 → 点击格式化先展示完整披露，勾选并确认后 grantAiConsent 再格式化', async () => {
    mocked.getAiCapabilities.mockResolvedValueOnce(enabledCaps({ consents: [] }));
    mocked.grantAiConsent.mockResolvedValueOnce({ ok: true });
    mocked.requestDraftFormat.mockResolvedValueOnce({
      task_id: 'task-1',
      status: 'queued',
      poll_url: '/ai/tasks/task-1',
      suggestion
    });
    const { findByRole, container, findByText } = render(EditorAssistantPanel, { props: baseProps });
    await fireEvent.click(await findByRole('button', { name: 'AI 格式化（diff 预览）' }));
    // 披露面板出现（含完整披露文案）。
    expect(await findByText(/发送前请阅读并确认/)).toBeTruthy();
    expect(container.textContent).toContain('测试提供商');
    expect(container.textContent).toContain('v1');
    const confirm = await findByRole('button', { name: '同意并继续' });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    const checkbox = container.querySelector('#ai-consent-ack') as HTMLInputElement;
    await fireEvent.click(checkbox);
    await fireEvent.click(confirm);
    await waitFor(() =>
      expect(mocked.grantAiConsent).toHaveBeenCalledWith(expect.anything(), {
        provider_id: 'prov-1',
        purpose: 'formatting',
        data_mode: 'full_with_consent',
        disclosure_version: 1,
        disclosure_hash: expect.any(String)
      })
    );
    await waitFor(() => expect(mocked.requestDraftFormat).toHaveBeenCalled());
  });
});

describe('M09-UI-04 diff 预览与字段级采纳', () => {
  it('渲染 diff 预览；点击「采纳此字段」→ acceptAiSuggestion + onApplyField', async () => {
    mocked.getAiCapabilities.mockResolvedValueOnce(enabledCaps({ consents: [{ provider_id: 'prov-1', purpose: 'formatting', data_mode: 'full_with_consent', disclosure_version: 1 }] }));
    mocked.requestDraftFormat.mockResolvedValueOnce({ task_id: 'task-1', status: 'queued', poll_url: '', suggestion });
    mocked.acceptAiSuggestion.mockResolvedValueOnce(suggestion);
    const onApplyField = vi.fn();
    const { findByRole, findAllByRole } = render(EditorAssistantPanel, { props: { ...baseProps, onApplyField } });
    await fireEvent.click(await findByRole('button', { name: 'AI 格式化（diff 预览）' }));
    // diff 预览（- 旧标题 / + 新标题）与采纳按钮（title + markdown 两个字段）。
    const adopt = (await findAllByRole('button', { name: '采纳此字段' }))[0];
    await fireEvent.click(adopt);
    await waitFor(() =>
      expect(mocked.acceptAiSuggestion).toHaveBeenCalledWith(expect.anything(), 's-1', {
        expected_base_version: 5,
        selected_fields: ['title']
      })
    );
    await waitFor(() => expect(onApplyField).toHaveBeenCalledWith('title', '新标题'));
  });

  it('采纳 409 version_conflict → 冲突提示与「加载最新建议」', async () => {
    mocked.getAiCapabilities.mockResolvedValueOnce(enabledCaps({ consents: [{ provider_id: 'prov-1', purpose: 'formatting', data_mode: 'full_with_consent', disclosure_version: 1 }] }));
    mocked.requestDraftFormat.mockResolvedValueOnce({ task_id: 'task-1', status: 'queued', poll_url: '', suggestion });
    mocked.acceptAiSuggestion.mockRejectedValueOnce({ status: 409, code: 'version_conflict' });
    const { findByRole, findAllByRole, findByText } = render(EditorAssistantPanel, { props: baseProps });
    await fireEvent.click(await findByRole('button', { name: 'AI 格式化（diff 预览）' }));
    const adopt = (await findAllByRole('button', { name: '采纳此字段' }))[0];
    await fireEvent.click(adopt);
    expect(await findByText(/建议已过期/)).toBeTruthy();
    expect(await findByRole('button', { name: '加载最新建议' })).toBeTruthy();
  });
});

describe('M09-UI-03 取消任务', () => {
  it('处理中可取消 → cancelAiTask', async () => {
    mocked.getAiCapabilities.mockResolvedValueOnce(enabledCaps({ consents: [{ provider_id: 'prov-1', purpose: 'formatting', data_mode: 'full_with_consent', disclosure_version: 1 }] }));
    // 返回无同步建议的 202 → 进入轮询态。
    mocked.requestDraftFormat.mockResolvedValueOnce({
      task_id: 'task-9',
      status: 'queued',
      poll_url: '/ai/tasks/task-9'
    });
    mocked.getAiTask.mockResolvedValue({ id: 'task-9', task_type: 'formatting', status: 'queued', created_at: 0 });
    mocked.cancelAiTask.mockResolvedValueOnce({ ok: true });
    const { findByRole, findByText } = render(EditorAssistantPanel, { props: baseProps });
    await fireEvent.click(await findByRole('button', { name: 'AI 格式化（diff 预览）' }));
    const cancel = await findByRole('button', { name: '取消任务' });
    await fireEvent.click(cancel);
    await waitFor(() => expect(mocked.cancelAiTask).toHaveBeenCalledWith(expect.anything(), 'task-9', 'req-key-0000000000000001'));
    expect(await findByText(/已发送取消请求/)).toBeTruthy();
  });
});
