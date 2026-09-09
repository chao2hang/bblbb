export interface ReactionDef {
  key: string;
  emoji: string;
  label: string;
  desc?: string;
  customIcon?: 'doge' | 'huaji';
}

/** 社区常用 Reaction 表情清单（首推滑稽狗头、滑稽脸，及经典互动表情）。 */
export const AVAILABLE_REACTIONS: ReactionDef[] = [
  { key: 'doge', emoji: '🐶', label: '狗头', desc: '经典滑稽狗头 / 狗头保命', customIcon: 'doge' },
  { key: 'huaji', emoji: '😏', label: '滑稽', desc: '经典贴吧滑稽脸 / 意味深长', customIcon: 'huaji' },
  { key: 'like', emoji: '👍', label: '点赞', desc: '赞同支持' },
  { key: 'heart', emoji: '❤️', label: '喜欢', desc: '十分喜爱' },
  { key: 'party', emoji: '🎉', label: '庆祝', desc: '撒花贺喜' },
  { key: 'laugh', emoji: '🤣', label: '搞笑', desc: '笑出强大' },
  { key: 'fire', emoji: '🔥', label: '给力', desc: '热烈强推' },
  { key: 'clap', emoji: '👏', label: '鼓掌', desc: '精彩喝彩' },
  { key: 'mindblown', emoji: '🤯', label: '震惊', desc: '大开眼界' },
  { key: 'thinking', emoji: '🤔', label: '深思', desc: '值得琢磨' }
];

export function getReactionDef(keyOrEmoji: string): ReactionDef {
  const found = AVAILABLE_REACTIONS.find(
    (r) => r.key === keyOrEmoji || r.emoji === keyOrEmoji
  );
  if (found) return found;
  return { key: keyOrEmoji, emoji: keyOrEmoji, label: keyOrEmoji };
}
