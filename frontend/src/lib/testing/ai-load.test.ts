// M09-UI-01/03/04/05/06：AI 页面 load 投影白名单测试。
//
// 验证各 server load 的投影挑选：
// - /ai：409 feature_disabled / 501 → disabled 态；403 → forbidden；
//   ok → 只保留展示字段（密钥/内部字段丢弃）；
// - /ai/suggestions/[id]：moderation 只保留公开摘要，内部 Prompt/举报/证据
//   字段丢弃；
// - /ai/tasks/[id]：只保留任务展示字段（内部 Prompt/Provider 原文丢弃）；
// - /admin/ai：配置投影只保留脱敏字段。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { getAuthed } from '$lib/api/server';
import { load as aiLoad, type AiPageData } from '../../routes/ai/+page.server';
import { load as suggestionLoad, type AiSuggestionPageData } from '../../routes/ai/suggestions/[id]/+page.server';
import { load as taskLoad, type AiTaskPageData } from '../../routes/ai/tasks/[id]/+page.server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function event(overrides: Record<string, unknown> = {}) {
  const headers = new Headers();
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers },
    url: new URL('http://test.local/ai'),
    params: { id: 'x-1' },
    ...overrides
  } as unknown as Parameters<typeof aiLoad>[0] & Parameters<typeof suggestionLoad>[0] & Parameters<typeof taskLoad>[0];
}

afterEach(() => vi.clearAllMocks());

describe('M09-UI-01 /ai load', () => {
  it('409 feature_disabled → disabled 态（默认关闭说明）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 409, message: 'x', requestId: 'r', retryAfterSecs: null, code: 'feature_disabled' });
    const data = (await aiLoad(event())) as AiPageData;
    expect(data.state).toBe('disabled');
    expect(data.disabledMessage).toContain('默认关闭');
  });

  it('501 not_implemented → disabled 态（后端未就绪降级）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 501, message: 'x', requestId: 'r', retryAfterSecs: null, code: null });
    const data = (await aiLoad(event())) as AiPageData;
    expect(data.state).toBe('disabled');
    expect(data.capabilities).toBeNull();
  });

  it('403 → forbidden 态', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 403, message: 'forbidden', requestId: 'r', retryAfterSecs: null, code: 'forbidden' });
    const data = (await aiLoad(event())) as AiPageData;
    expect(data.state).toBe('forbidden');
  });

  it('ok → 投影白名单：密钥/内部字段不进输出', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        enabled: true,
        data_mode: 'redacted',
        purposes: ['formatting', 'seo'],
        providers: [
          {
            id: 'prov-1',
            name: 'P',
            secret_configured: true,
            api_key: 'SECRET-KEY-1',
            api_key_secret: 'SECRET-KEY-2',
            base_url: 'https://internal.example',
            available: true
          }
        ],
        consents: [
          { provider_id: 'prov-1', purpose: 'formatting', data_mode: 'full_with_consent', disclosure_version: 1, granted_at: 0 }
        ]
      }
    });
    const data = (await aiLoad(event())) as AiPageData;
    expect(data.state).toBe('ok');
    expect(data.capabilities?.enabled).toBe(true);
    const provider = data.capabilities!.providers![0];
    expect(provider.secret_configured).toBe(true);
    expect(provider).not.toHaveProperty('api_key');
    expect(provider).not.toHaveProperty('api_key_secret');
    expect(provider).not.toHaveProperty('base_url');
    expect(data.capabilities?.consents).toHaveLength(1);
  });
});

describe('M09-UI-05 /ai/suggestions/[id] load', () => {
  it('moderation 建议：只保留公开摘要，内部 Prompt/证据/风险信号丢弃', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        id: 's-1',
        type: 'moderation',
        status: 'pending',
        base_version: 2,
        created_at: 0,
        internal_prompt: 'INTERNAL-PROMPT',
        report_evidence: 'EVIDENCE',
        risk_signals: ['RISK-1'],
        moderation: { target_type: 'post', summary: '公开摘要' }
      }
    });
    const data = (await suggestionLoad(event())) as AiSuggestionPageData;
    expect(data.suggestion?.moderation?.summary).toBe('公开摘要');
    expect(data.suggestion).not.toHaveProperty('internal_prompt');
    expect(data.suggestion).not.toHaveProperty('report_evidence');
    expect(data.suggestion).not.toHaveProperty('risk_signals');
  });

  it('formatting 建议：字段只保留 current/proposed/reason/selectable', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        id: 's-2',
        type: 'formatting',
        status: 'pending',
        base_version: 5,
        created_at: 0,
        fields: [
          { field: 'title', current: '旧', proposed: '新', reason: '更简洁', selectable: true, raw_model_output: 'RAW' }
        ]
      }
    });
    const data = (await suggestionLoad(event())) as AiSuggestionPageData;
    expect(data.suggestion?.fields[0]).toMatchObject({ field: 'title', proposed: '新' });
    expect(data.suggestion?.fields[0]).not.toHaveProperty('raw_model_output');
  });
});

describe('M09-UI-03 /ai/tasks/[id] load', () => {
  it('任务投影：只保留展示字段；内部 Prompt/Provider 原文丢弃', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        id: 't-1',
        task_type: 'formatting',
        status: 'running',
        source_revision: 3,
        created_at: 0,
        provider_response: 'RAW-PROVIDER',
        prompt: 'INTERNAL-PROMPT'
      }
    });
    const data = (await taskLoad(event())) as AiTaskPageData;
    expect(data.task?.source_revision).toBe(3);
    expect(data.task).not.toHaveProperty('provider_response');
    expect(data.task).not.toHaveProperty('prompt');
  });
});
