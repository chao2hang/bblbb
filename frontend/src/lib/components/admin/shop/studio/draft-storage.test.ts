import { describe, expect, it } from 'vitest';
import {
  SNAPSHOT_TTL_MS,
  clearSnapshot,
  loadSnapshot,
  saveSnapshot,
  snapshotHasContent,
  storageKey,
  type StorageLike
} from './draft-storage';
import { createDraft } from './style-draft';
import { createPublishDraft } from './publish-draft';

function mockStorage(): StorageLike & { map: Map<string, string> } {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (k) => map.get(k) ?? null,
    setItem: (k, v) => void map.set(k, v),
    removeItem: (k) => void map.delete(k)
  };
}

describe('draft-storage', () => {
  it('键格式：studio:{kind}:{defId|new}', () => {
    expect(storageKey('nickname_color', '')).toBe('bblbb:studio:nickname_color:new');
    expect(storageKey('avatar_frame', 'c-1')).toBe('bblbb:studio:avatar_frame:c-1');
  });

  it('保存 → 读取往返（draft/publish 完整还原）', () => {
    const s = mockStorage();
    const draft = { ...createDraft('nickname_color'), name: '我的渐变', stops: ['#111111', '#222222'] };
    const publish = { ...createPublishDraft(), title: '商品A', unitPrice: 120 };
    saveSnapshot(s, 'nickname_color', '', draft, publish, 1000);
    const snap = loadSnapshot(s, 'nickname_color', '', 2000);
    expect(snap).not.toBeNull();
    expect(snap!.draft.name).toBe('我的渐变');
    expect(snap!.draft.stops).toEqual(['#111111', '#222222']);
    expect(snap!.publish.title).toBe('商品A');
    expect(snap!.publish.unitPrice).toBe(120);
    expect(snap!.savedAt).toBe(1000);
  });

  it('kind 不匹配 → null（同键不同类的草稿不被误恢复）', () => {
    const s = mockStorage();
    saveSnapshot(s, 'nickname_color', '', createDraft('nickname_color'), createPublishDraft());
    expect(loadSnapshot(s, 'avatar_frame', '')).toBeNull();
  });

  it('过期快照 → null', () => {
    const s = mockStorage();
    saveSnapshot(s, 'nickname_color', '', createDraft('nickname_color'), createPublishDraft(), 1000);
    expect(loadSnapshot(s, 'nickname_color', '', 1000 + SNAPSHOT_TTL_MS + 1)).toBeNull();
  });

  it('损坏 JSON / 结构不符 → null', () => {
    const s = mockStorage();
    s.map.set(storageKey('avatar_frame', ''), 'not-json{');
    expect(loadSnapshot(s, 'avatar_frame', '')).toBeNull();
    s.map.set(storageKey('avatar_frame', ''), JSON.stringify({ draft: null, savedAt: 1 }));
    expect(loadSnapshot(s, 'avatar_frame', '')).toBeNull();
  });

  it('缺字段回退默认值（结构弹性）', () => {
    const s = mockStorage();
    s.map.set(
      storageKey('avatar_frame', ''),
      JSON.stringify({ draft: { kind: 'avatar_frame', name: '银河' }, savedAt: 1000 })
    );
    const snap = loadSnapshot(s, 'avatar_frame', '', 2000);
    expect(snap!.draft.name).toBe('银河');
    expect(snap!.draft.shape).toBe('circle'); // createDraft 默认
    expect(snap!.publish.unitPrice).toBe(0); // createPublishDraft 默认
  });

  it('clearSnapshot 删除键；异常 Storage 静默降级', () => {
    const s = mockStorage();
    saveSnapshot(s, 'avatar_frame', '', createDraft('avatar_frame'), createPublishDraft());
    clearSnapshot(s, 'avatar_frame', '');
    expect(loadSnapshot(s, 'avatar_frame', '')).toBeNull();

    const broken: StorageLike = {
      getItem: () => { throw new Error('denied'); },
      setItem: () => { throw new Error('denied'); },
      removeItem: () => { throw new Error('denied'); }
    };
    expect(() => saveSnapshot(broken, 'avatar_frame', '', createDraft('avatar_frame'), createPublishDraft())).not.toThrow();
    expect(loadSnapshot(broken, 'avatar_frame', '')).toBeNull();
    expect(() => clearSnapshot(broken, 'avatar_frame', '')).not.toThrow();
  });

  it('snapshotHasContent：与初始草稿一致 → false；有名称或参数差异 → true', () => {
    const initial = createDraft('nickname_color');
    const same = { draft: createDraft('nickname_color'), publish: createPublishDraft(), savedAt: 1 };
    expect(snapshotHasContent(same, initial)).toBe(false);
    const named = { ...same, draft: { ...createDraft('nickname_color'), name: '新设计' } };
    expect(snapshotHasContent(named, initial)).toBe(true);
    const tweaked = { ...same, draft: { ...createDraft('nickname_color'), glowPx: 20 } };
    expect(snapshotHasContent(tweaked, initial)).toBe(true);
  });
});
