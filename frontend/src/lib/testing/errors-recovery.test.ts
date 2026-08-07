// M04-UI-08：可恢复流程 —— problemRecovery 错误码 → 用户动作映射、
// postStatusNotice 状态提示、字段级错误映射。
import { describe, expect, it } from 'vitest';
import {
  problemRecovery,
  postStatusNotice,
  fieldError,
  problemMessage,
  type Problem
} from '$lib/errors';

describe('M04-UI-08 problemRecovery：错误码 → 用户动作', () => {
  it('version_conflict（409）→ reload，提示"重新加载"', () => {
    const hint = problemRecovery({ status: 409, code: 'version_conflict' });
    expect(hint.action).toBe('reload');
    expect(hint.message).toContain('重新加载');
  });

  it('visibility_level_exceeds_author（422 等级变化）→ adjust，提示调低可见等级', () => {
    const hint = problemRecovery({ status: 422, code: 'visibility_level_exceeds_author' });
    expect(hint.action).toBe('adjust');
    expect(hint.message).toContain('调低可见等级');
  });

  it('rate_limited（429）→ wait，提示稍后重试；带 Retry-After 秒数', () => {
    const hint = problemRecovery({ status: 429, code: 'rate_limited', retry_after: 42 });
    expect(hint.action).toBe('wait');
    expect(hint.message).toContain('操作过于频繁');
    expect(hint.message).toContain('42 秒');
  });

  it('仅 status=429（无 code）也映射为 wait', () => {
    const hint = problemRecovery({ status: 429 });
    expect(hint.action).toBe('wait');
  });

  it('idempotency_conflict → retry，提示使用新内容', () => {
    const hint = problemRecovery({ status: 409, code: 'idempotency_conflict' });
    expect(hint.action).toBe('retry');
    expect(hint.message).toContain('重试');
  });

  it('csrf_failed → reload（安全校验失败刷新）', () => {
    const hint = problemRecovery({ status: 403, code: 'csrf_failed' });
    expect(hint.action).toBe('reload');
  });

  it('未知错误 → none + problemMessage 兜底', () => {
    const hint = problemRecovery({ status: 400, code: 'invalid_request' });
    expect(hint.action).toBe('none');
    expect(hint.message).toBe(problemMessage({ status: 400, code: 'invalid_request' }));
  });

  it('null/undefined → none', () => {
    expect(problemRecovery(null).action).toBe('none');
    expect(problemRecovery(undefined).action).toBe('none');
  });
});

describe('M04-UI-08 postStatusNotice：内容状态 → 审核中提示', () => {
  it('pending_review → "内容审核中"', () => {
    expect(postStatusNotice('pending_review')).toContain('审核中');
  });

  it('draft → 草稿提示', () => {
    expect(postStatusNotice('draft')).toContain('草稿');
  });

  it('published/其他/null → null（无提示）', () => {
    expect(postStatusNotice('published')).toBeNull();
    expect(postStatusNotice('hidden')).toBeNull();
    expect(postStatusNotice(null)).toBeNull();
    expect(postStatusNotice(undefined)).toBeNull();
  });
});

describe('M04-UI-02/08 fieldError：服务端字段错误映射', () => {
  const problem: Problem = {
    status: 422,
    code: 'invalid_request',
    errors: [
      { field: 'title', code: 'invalid', message_key: 'title_too_long' },
      { field: 'markdown', code: 'too_long', message_key: 'content_too_long' },
      { field: 'board_id', code: 'invalid_uuid', message_key: 'invalid_board' }
    ]
  };

  it('title / markdown / board_id 字段命中中文文案', () => {
    expect(fieldError(problem, 'title')).toContain('标题');
    expect(fieldError(problem, 'markdown')).toContain('正文');
    expect(fieldError(problem, 'board_id')).toContain('板块');
  });

  it('未列出的字段 → null', () => {
    expect(fieldError(problem, 'unknown_field')).toBeNull();
    expect(fieldError(null, 'title')).toBeNull();
  });
});
