// M07-SHOP-STUDIO-UX：设计/发布草稿的 localStorage 保护。
// 工作台是长表单设计工具，刷新/误关不该丢稿：按 `studio:{kind}:{defId|new}`
// 键防抖自动保存；重进同键且有更完整草稿时由页面顶部「恢复条」人工确认
// （绝不静默覆盖服务端 data——编辑已有 def 时以 data 为准）。
// 纯函数 + 注入 Storage 便于单测；localStorage 不可用（隐私模式/配额）时静默降级。

import { createDraft, type StyleDraft, type StyleKind } from './style-draft';
import { createPublishDraft, type PublishDraft } from './publish-draft';

const KEY_PREFIX = 'bblbb:studio:';
/** 快照保鲜期：超过 7 天视为废弃（样式库可能已变化）。 */
export const SNAPSHOT_TTL_MS = 7 * 24 * 3600 * 1000;

export interface StudioDraftSnapshot {
  draft: StyleDraft;
  publish: PublishDraft;
  savedAt: number;
}

/** 注入式最小 Storage 接口（localStorage / 测试 mock 通用）。 */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function storageKey(kind: StyleKind, defId: string): string {
  return `${KEY_PREFIX}${kind}:${defId || 'new'}`;
}

/** 保存快照（序列化失败/配额满时静默忽略——草稿保护不该打断设计）。 */
export function saveSnapshot(
  storage: StorageLike,
  kind: StyleKind,
  defId: string,
  draft: StyleDraft,
  publish: PublishDraft,
  now = Date.now()
): void {
  try {
    const snap: StudioDraftSnapshot = { draft, publish, savedAt: now };
    storage.setItem(storageKey(kind, defId), JSON.stringify(snap));
  } catch {
    /* quota / 序列化异常：忽略 */
  }
}

/**
 * 读取快照：过期/损坏/结构不符一律返回 null。
 * 结构防御：draft 合并到 createDraft（缺字段回退默认，kind 必须匹配），
 * publish 合并到 createPublishDraft——与 draftFromDef 的弹性策略一致。
 */
export function loadSnapshot(
  storage: StorageLike,
  kind: StyleKind,
  defId: string,
  now = Date.now()
): StudioDraftSnapshot | null {
  let raw: string | null = null;
  try {
    raw = storage.getItem(storageKey(kind, defId));
  } catch {
    return null;
  }
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<StudioDraftSnapshot> | null;
    if (!parsed || typeof parsed !== 'object') return null;
    if (typeof parsed.savedAt !== 'number' || now - parsed.savedAt > SNAPSHOT_TTL_MS) return null;
    const d = parsed.draft;
    if (!d || typeof d !== 'object' || (d as StyleDraft).kind !== kind) return null;
    const draft: StyleDraft = { ...createDraft(kind), ...(d as Partial<StyleDraft>), kind };
    const publish: PublishDraft = { ...createPublishDraft(), ...(parsed.publish as Partial<PublishDraft> | undefined) };
    return { draft, publish, savedAt: parsed.savedAt };
  } catch {
    return null;
  }
}

export function clearSnapshot(storage: StorageLike, kind: StyleKind, defId: string): void {
  try {
    storage.removeItem(storageKey(kind, defId));
  } catch {
    /* 忽略 */
  }
}

/** 快照是否「比初始草稿更有内容」（避免刚打开空表单就弹恢复条）。 */
export function snapshotHasContent(snap: StudioDraftSnapshot, initial: StyleDraft): boolean {
  if (snap.draft.name.trim() && snap.draft.name.trim() !== initial.name.trim()) return true;
  return JSON.stringify(snap.draft) !== JSON.stringify(initial);
}
