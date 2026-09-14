// M18-ADMIN-BATCH-03：批量操作辅助单元测试（ids/versions 解析与结果汇总）。
import { describe, expect, it } from 'vitest';
import {
  batchResult,
  emptyBatchSelection,
  parseBatchEntries,
  parseBatchIds,
  type BatchOutcome
} from './admin-batch';

describe('parseBatchIds', () => {
  it('解析逗号分隔 ids 并去重保序', () => {
    const form = new FormData();
    form.set('ids', 'a, b ,a,c');
    expect(parseBatchIds(form)).toEqual(['a', 'b', 'c']);
  });

  it('兼容重复字段（getAll）与混合提交', () => {
    const form = new FormData();
    form.append('ids', 'a');
    form.append('ids', 'b,c');
    expect(parseBatchIds(form)).toEqual(['a', 'b', 'c']);
  });

  it('空/纯逗号输入返回空数组', () => {
    const form = new FormData();
    form.set('ids', ',,');
    expect(parseBatchIds(form)).toEqual([]);
    expect(parseBatchIds(new FormData())).toEqual([]);
  });
});

describe('parseBatchEntries', () => {
  it('versions 与 ids 按位置配对，缺失版本为 null', () => {
    const form = new FormData();
    form.set('ids', 'a,b,c');
    form.set('versions', '1,,3');
    expect(parseBatchEntries(form)).toEqual([
      { id: 'a', version: '1' },
      { id: 'b', version: null },
      { id: 'c', version: '3' }
    ]);
  });

  it('无 versions 字段时全部为 null（不加 If-Match）', () => {
    const form = new FormData();
    form.set('ids', 'a,b');
    expect(parseBatchEntries(form)).toEqual([
      { id: 'a', version: null },
      { id: 'b', version: null }
    ]);
  });
});

describe('batchResult', () => {
  it('全部成功 → ok 汇总文案', () => {
    const outcome: BatchOutcome = { okCount: 3, failures: [] };
    const r = batchResult(outcome, '批量启用');
    expect(r.ok).toBe(true);
    expect(r.status).toBe(200);
    expect(r.message).toContain('成功 3 项');
  });

  it('部分失败 → fail 汇总文案含首个失败详情', () => {
    const outcome: BatchOutcome = {
      okCount: 2,
      failures: [
        { id: 'x', message: '版本冲突' },
        { id: 'y', message: '无权限' }
      ]
    };
    const r = batchResult(outcome, '批量禁用');
    expect(r.ok).toBe(false);
    expect(r.status).toBe(500);
    expect(r.message).toContain('成功 2 项');
    expect(r.message).toContain('失败 2 项');
    expect(r.message).toContain('x：版本冲突');
  });

  it('全部失败 → fail 文案', () => {
    const outcome: BatchOutcome = { okCount: 0, failures: [{ id: 'z', message: '网络错误' }] };
    const r = batchResult(outcome, '批量删除');
    expect(r.ok).toBe(false);
    expect(r.message).toContain('均未成功');
  });
});

describe('emptyBatchSelection', () => {
  it('空选择产出失败项（未选择任何条目）', () => {
    const outcome = emptyBatchSelection();
    expect(outcome.okCount).toBe(0);
    expect(outcome.failures[0]?.message).toContain('未选择');
  });
});
