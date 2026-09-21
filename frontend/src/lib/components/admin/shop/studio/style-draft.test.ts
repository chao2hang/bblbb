// 工作台设计草稿 → style JSON mapper 与预检的单元测试。
// 用例对齐 backend/src/shop/cosmetics.rs validate_style 的测试：
// 同样的草稿必须产出服务端 schema 能接受的结构化样式。
import { describe, expect, it } from 'vitest';
import {
  STYLE_LIMITS,
  createDraft,
  draftFromDef,
  draftToPresentation,
  draftToStyle,
  validateDraft
} from './style-draft';

describe('draftToStyle（草稿 → style JSON）', () => {
  it('昵称纯色：只保留 mode/color', () => {
    const draft = createDraft('nickname_color');
    draft.mode = 'solid';
    draft.color = '#F472B6';
    expect(draftToStyle(draft)).toEqual({ mode: 'solid', color: '#F472B6' });
  });

  it('昵称渐变：colors + animate flow，静止时 animate=none', () => {
    const draft = createDraft('nickname_color');
    draft.mode = 'gradient';
    draft.stops = ['#ff0000', '#00ff00', '#0000ff'];
    draft.animate = 'flow';
    draft.durationMs = 5000;
    expect(draftToStyle(draft)).toEqual({
      mode: 'gradient',
      colors: ['#ff0000', '#00ff00', '#0000ff'],
      animate: 'flow',
      durationMs: 5000
    });
    draft.animate = 'breathe';
    expect(draftToStyle(draft).animate).toBe('none');
  });

  it('昵称发光：animate 只允许 none/breathe（pulse 归一为 breathe）', () => {
    const draft = createDraft('nickname_color');
    draft.mode = 'glow';
    draft.animate = 'pulse';
    expect(draftToStyle(draft)).toMatchObject({ mode: 'glow', animate: 'breathe' });
    draft.animate = 'none';
    expect(draftToStyle(draft)).toMatchObject({ mode: 'glow', animate: 'none' });
  });

  it('头像框：ring 模式带 width/glow/shape/animate', () => {
    const draft = createDraft('avatar_frame');
    draft.widthPx = 4;
    draft.glowPx = 12;
    draft.shape = 'rounded';
    draft.animate = 'pulse';
    expect(draftToStyle(draft)).toEqual({
      mode: 'ring',
      color: '#f472b6',
      widthPx: 4,
      glowPx: 12,
      shape: 'rounded',
      animate: 'pulse',
      durationMs: 5000
    });
    draft.animate = 'flow';
    expect(draftToStyle(draft).animate).toBe('none');
  });

  it('主页装饰：breathe → shimmer，其余 → none，并支持 Steam 动态背景', () => {
    const draft = createDraft('profile_effect');
    draft.texture = 'dark_stars';
    draft.baseColor = '#101827';
    draft.accentColor = '#8b5cf6';
    draft.animate = 'breathe';
    draft.profileBackgroundUrl = '/api/v1/steam-assets/backgrounds/test.jpg';
    draft.profileBackgroundWebm = 'https://steam/test.webm';
    expect(draftToStyle(draft)).toEqual({
      mode: 'profile',
      texture: 'dark_stars',
      baseColor: '#101827',
      accentColor: '#8b5cf6',
      animate: 'shimmer',
      durationMs: 5000,
      url: '/api/v1/steam-assets/backgrounds/test.jpg',
      webm: 'https://steam/test.webm'
    });
  });
});

describe('validateDraft（前端预检，边界与后端一致）', () => {
  it('默认草稿通过', () => {
    expect(validateDraft({ ...createDraft(), name: '樱花粉渐变' })).toEqual({});
  });

  it('名称必填且 ≤32 字符', () => {
    expect(validateDraft(createDraft()).name).toBeTruthy();
    const long = createDraft();
    long.name = '超'.repeat(STYLE_LIMITS.nameMax + 1);
    expect(validateDraft(long).name).toBeTruthy();
    const ok = createDraft();
    ok.name = '超'.repeat(STYLE_LIMITS.nameMax);
    expect(validateDraft(ok).name).toBeUndefined();
  });

  it('非法色值被拒绝（CSS 注入防线）', () => {
    const draft = createDraft();
    draft.name = 'x';
    draft.color = 'url(evil)';
    expect(validateDraft(draft).color).toBeTruthy();
    draft.color = 'red; background:url(x)';
    expect(validateDraft(draft).color).toBeTruthy();
  });

  it('渐变色标数量与格式校验', () => {
    const draft = createDraft();
    draft.name = 'x';
    draft.mode = 'gradient';
    draft.stops = ['#ff0000'];
    expect(validateDraft(draft).stops).toBeTruthy();
    draft.stops = ['#ff0000', '#00ff00', '#0000ff', '#123456', '#abcdef', '#ffffff'];
    expect(validateDraft(draft).stops).toBeTruthy();
    draft.stops = ['#ff0000', 'red'];
    expect(validateDraft(draft).stops).toBeTruthy();
    draft.stops = ['#ff0000', '#00ff00'];
    expect(validateDraft(draft).stops).toBeUndefined();
  });

  it('时长/宽度/光晕/次数边界', () => {
    const draft = createDraft('avatar_frame');
    draft.name = 'x';
    draft.durationMs = 100;
    expect(validateDraft(draft).durationMs).toBeTruthy();
    draft.durationMs = 99999;
    expect(validateDraft(draft).durationMs).toBeTruthy();
    draft.durationMs = 3000;
    draft.widthPx = 0;
    expect(validateDraft(draft).widthPx).toBeTruthy();
    draft.widthPx = 9;
    expect(validateDraft(draft).widthPx).toBeTruthy();
    draft.widthPx = 3;
    draft.glowPx = 25;
    expect(validateDraft(draft).glowPx).toBeTruthy();
    draft.glowPx = 24;
    expect(validateDraft(draft)).toEqual({});
  });
});

describe('draftFromDef（编辑还原）', () => {
  it('还原渐变昵称定义', () => {
    const draft = draftFromDef({
      id: 'c1',
      kind: 'nickname_color',
      name: '落日渐变',
      status: 'active',
      style: { mode: 'gradient', colors: ['#ff0000', '#00ff00'], animate: 'flow', durationMs: 5000 }
    });
    expect(draft.kind).toBe('nickname_color');
    expect(draft.mode).toBe('gradient');
    expect(draft.stops).toEqual(['#ff0000', '#00ff00']);
    expect(draftToStyle(draft)).toEqual({
      mode: 'gradient',
      colors: ['#ff0000', '#00ff00'],
      animate: 'flow',
      durationMs: 5000
    });
  });

  it('未知 kind 回退昵称颜色；非法字段回退默认值', () => {
    const draft = draftFromDef({
      id: 'c2',
      kind: 'unknown_feature',
      name: 'x',
      status: 'active',
      style: { mode: 'weird', color: 'not-a-color' }
    });
    expect(draft.kind).toBe('nickname_color');
    expect(draft.mode).toBe('solid');
    expect(draft.color).toBe('#f472b6');
  });
});

describe('draftToPresentation（预览 Token）', () => {
  it('各装扮槽位生成对应 presentation 键', () => {
    const nick = createDraft('nickname_color');
    nick.name = '樱花粉';
    expect(draftToPresentation(nick)).toMatchObject({
      nickname_color: 'preview',
      nickname_color_name: '樱花粉'
    });
    const frame = createDraft('avatar_frame');
    expect(draftToPresentation(frame)).toMatchObject({ avatar_frame: 'preview' });
    const profile = createDraft('profile_effect');
    expect(draftToPresentation(profile)).toMatchObject({ profile_effect: 'preview' });
  });

  it('名称为空时预览名回退「预览」', () => {
    const draft = createDraft();
    expect(draftToPresentation(draft).nickname_color_name).toBe('预览');
  });

  it('开放自定义 CSS 与 JS 脚本在 draftToStyle 与 validateDraft 中正确处理', () => {
    const draft = createDraft('avatar_frame');
    draft.name = '赛博炫光框';
    draft.color = '#8b5cf6';
    draft.borderStyle = 'dashed';
    draft.animate = 'spin';
    draft.glowSpread = 5;
    draft.css = '.avatar-frame-custom { transform: rotate(10deg); }';
    draft.js = 'console.log("spin frame initialized");';

    const style = draftToStyle(draft);
    expect(style.borderStyle).toBe('dashed');
    expect(style.animate).toBe('spin');
    expect(style.glowSpread).toBe(5);
    expect(style.css).toBe('.avatar-frame-custom { transform: rotate(10deg); }');
    expect(style.js).toBe('console.log("spin frame initialized");');

    // 预检无错误
    expect(validateDraft(draft)).toEqual({});

    // 超长 CSS 触发报错
    draft.css = 'a'.repeat(8193);
    expect(validateDraft(draft).css).toContain('8192');

    // 超长 JS 触发报错
    draft.css = '';
    draft.js = 'b'.repeat(8193);
    expect(validateDraft(draft).js).toContain('8192');
  });
});
