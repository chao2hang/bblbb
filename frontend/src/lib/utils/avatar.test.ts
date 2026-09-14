import { describe, expect, it } from 'vitest';
import {
  AVATAR_PALETTES,
  getAvatarInitial,
  getAvatarColorIndex,
  getAvatarGradient,
  resolveAvatarSeed
} from './avatar';

describe('avatar utils', () => {
  describe('getAvatarInitial', () => {
    it('英文转为大写首字母', () => {
      expect(getAvatarInitial('alice')).toBe('A');
      expect(getAvatarInitial('Bob')).toBe('B');
      expect(getAvatarInitial(' charlie ')).toBe('C');
    });

    it('去除前导 @ 与 # 符号', () => {
      expect(getAvatarInitial('@nina')).toBe('N');
      expect(getAvatarInitial('@@ken')).toBe('K');
      expect(getAvatarInitial('#rust')).toBe('R');
    });

    it('支持中文字符并原样保留', () => {
      expect(getAvatarInitial('张三')).toBe('张');
      expect(getAvatarInitial('李白')).toBe('李');
      expect(getAvatarInitial('小明')).toBe('小');
    });

    it('正确处理 Emoji 与 Unicode surrogate pairs', () => {
      expect(getAvatarInitial('🚀rocket')).toBe('🚀');
      expect(getAvatarInitial('✨sparkles')).toBe('✨');
    });

    it('空值或纯空白安全降级为问号', () => {
      expect(getAvatarInitial('')).toBe('?');
      expect(getAvatarInitial('   ')).toBe('?');
      expect(getAvatarInitial(null)).toBe('?');
      expect(getAvatarInitial(undefined)).toBe('?');
      expect(getAvatarInitial('@@@')).toBe('?');
    });
  });

  describe('getAvatarColorIndex 与 getAvatarGradient', () => {
    it('同一用户名称始终得到完全相同的色盘索引（按用户固定）', () => {
      const idx1 = getAvatarColorIndex('Alice');
      const idx2 = getAvatarColorIndex('Alice');
      const idx3 = getAvatarColorIndex('Alice');
      expect(idx1).toBe(idx2);
      expect(idx2).toBe(idx3);

      const grad1 = getAvatarGradient('Alice');
      const grad2 = getAvatarGradient('Alice');
      expect(grad1.gradient).toBe(grad2.gradient);
      expect(grad1.c1).toBe(grad2.c1);
    });

    it('不区分大小写，相同 handle 保持相同配色', () => {
      expect(getAvatarColorIndex('Alice')).toBe(getAvatarColorIndex('alice'));
      expect(getAvatarColorIndex('BOB')).toBe(getAvatarColorIndex('bob'));
      expect(getAvatarColorIndex('  Charlie  ')).toBe(getAvatarColorIndex('charlie'));
    });

    it('不同用户在 16 种色盘中呈现丰富的离散度（随机感）', () => {
      const users = [
        'Alice',
        'Bob',
        'Charlie',
        'David',
        'Emma',
        'Frank',
        'Grace',
        'Henry',
        'Ivy',
        'Jack',
        'Karen',
        'Leo',
        'Mia',
        'Noah',
        'Olivia',
        'Peter'
      ];
      const indices = users.map((u) => getAvatarColorIndex(u));
      const uniqueIndices = new Set(indices);
      // 16 个典型名字至少命中 8 种以上不同色盘，保证列表/头像流不单调
      expect(uniqueIndices.size).toBeGreaterThanOrEqual(8);
    });

    it('色盘数组均包含合法 hex 颜色并提供 16 种渐变组合', () => {
      expect(AVATAR_PALETTES.length).toBe(16);
      for (const [c1, c2] of AVATAR_PALETTES) {
        expect(c1).toMatch(/^#[0-9a-fA-F]{6}$/);
        expect(c2).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });
  });

  describe('resolveAvatarSeed', () => {
    it('优先使用全站稳定且唯一的 username', () => {
      expect(
        resolveAvatarSeed({
          username: 'Chaos',
          id: 'u_1001',
          display_name: '站长'
        })
      ).toBe('Chaos');
    });

    it('缺省 username 时回退到 id', () => {
      expect(
        resolveAvatarSeed({
          username: null,
          id: 'u_1002',
          display_name: '匿名用户'
        })
      ).toBe('u_1002');
    });

    it('缺省 username 与 id 时回退到 display_name / name', () => {
      expect(
        resolveAvatarSeed({
          display_name: '访客'
        })
      ).toBe('访客');
    });

    it('支持直接传入字符串 key', () => {
      expect(resolveAvatarSeed('  alice  ')).toBe('alice');
    });

    it('空值或未识别对象回退为问号', () => {
      expect(resolveAvatarSeed(null)).toBe('?');
      expect(resolveAvatarSeed(undefined)).toBe('?');
      expect(resolveAvatarSeed({})).toBe('?');
    });
  });
});
