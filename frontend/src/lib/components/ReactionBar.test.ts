// M07-UI-07：ReactionBar 组件测试——
// 新交互：无反应时不展示左侧图标；有反应时仅显示「图标 + 次数」，点击图标弹出明细弹窗
// （Tab 切换 + 用户列表）；添加/撤销统一走右侧「+ 表情」选择器。
// 另覆盖 429 限流提示、403 权限错误、未登录提示与作者/非作者通知文案。
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import ReactionBar from './ReactionBar.svelte';
import * as client from '$lib/api/client';

vi.mock('$lib/api/client', () => ({
  addPostReaction: vi.fn(),
  removePostReaction: vi.fn(),
  addCommentReaction: vi.fn(),
  removeCommentReaction: vi.fn(),
  getPostReactions: vi.fn(),
  getCommentReactions: vi.fn()
}));

const mocked = client as unknown as {
  addPostReaction: ReturnType<typeof vi.fn>;
  removePostReaction: ReturnType<typeof vi.fn>;
  getPostReactions: ReturnType<typeof vi.fn>;
  getCommentReactions: ReturnType<typeof vi.fn>;
};

const baseProps = {
  targetType: 'post' as const,
  targetId: 'post-1',
  reactions: [
    { reaction: '👍', count: 3, active: false },
    { reaction: '🎉', count: 1, active: true }
  ]
};

const detailFixture = {
  target_type: 'post',
  target_id: 'post-1',
  total: 4,
  counts: { '👍': 3, '🎉': 1 },
  viewer_reactions: ['🎉'],
  users: [
    {
      user_id: 'u-1',
      username: 'lyfmya',
      display_name: 'lyfmya',
      reaction: '👍',
      created_at: 1000
    },
    {
      user_id: 'u-2',
      username: 'liyunxuan',
      display_name: '尘埃落定',
      reaction: '🎉',
      created_at: 2000
    }
  ]
};

beforeEach(() => vi.clearAllMocks());

describe('M07-UI-07 ReactionBar', () => {
  it('无反应时左侧不展示任何图标（+ 表情入口保留）', () => {
    const { container, getByRole } = render(ReactionBar, {
      props: {
        ...baseProps,
        reactions: [
          { reaction: 'like', count: 0, active: false },
          { reaction: 'doge', count: 0, active: false }
        ]
      }
    });
    expect(container.querySelector('.rx-pill')).toBeNull();
    expect(getByRole('button', { name: '添加表情反应' })).toBeTruthy();
  });

  it('有反应时仅展示「图标 + 次数」，点击图标弹出明细弹窗并定位对应 Tab', async () => {
    mocked.getPostReactions.mockResolvedValueOnce(detailFixture);
    const { container, getByRole, findByRole, findByText } = render(ReactionBar, { props: baseProps });

    // 两个有计数的反应各一个紧凑 Pill（图标 + 次数），无零计数项
    const likePill = getByRole('button', { name: '查看 点赞 反应明细（3 次）' });
    const partyPill = getByRole('button', { name: '查看 庆祝 反应明细（1 次）' });
    expect(likePill.textContent).toContain('3');
    expect(partyPill.textContent).toContain('1');
    expect(container.querySelectorAll('.rx-pill').length).toBe(2);

    // 点击 👍 图标 → 明细弹窗打开，默认展示该表情的用户
    await fireEvent.click(likePill);
    await findByRole('dialog', { name: '收到的表情' });
    expect(await findByText('lyfmya')).toBeTruthy();
    expect(await findByText('@lyfmya')).toBeTruthy();

    // 切换到 🎉 Tab → 用户列表过滤
    await fireEvent.click(partyPill);
    expect(await findByText('尘埃落定')).toBeTruthy();
    expect(await findByText('@liyunxuan')).toBeTruthy();
  });

  it('自己已反应但计数为 0 的项仍然展示图标（乐观态）', () => {
    const { container } = render(ReactionBar, {
      props: {
        ...baseProps,
        reactions: [{ reaction: 'like', count: 0, active: true }]
      }
    });
    const pills = container.querySelectorAll('.rx-pill');
    expect(pills.length).toBe(1);
    expect(pills[0].textContent).toContain('0');
  });

  it('再次点击同一图标收起明细弹窗', async () => {
    const { getByRole, queryByRole, findByRole } = render(ReactionBar, { props: baseProps });
    const likePill = getByRole('button', { name: /查看 点赞 反应明细/ });
    await fireEvent.click(likePill);
    await findByRole('dialog', { name: '收到的表情' });
    await fireEvent.click(likePill);
    expect(queryByRole('dialog', { name: '收到的表情' })).toBeNull();
  });

  it('「+ 表情」选择器：点击 狗头 发送 doge 反应，左侧出现对应图标', async () => {
    mocked.addPostReaction.mockResolvedValueOnce({ reaction: 'doge', active: true, count: 1 });
    const { getByRole, findByRole } = render(ReactionBar, { props: baseProps });
    const pickerBtn = getByRole('button', { name: '添加表情反应' });
    await fireEvent.click(pickerBtn);
    // portal 到 body：弹层不被卡片 overflow:hidden 祖先裁剪（弹窗在父元素内的回归防护）
    expect(document.querySelector('.reaction-picker-popover')?.parentElement).toBe(document.body);
    const dogeBtn = await findByRole('button', { name: '狗头' });
    await fireEvent.click(dogeBtn);
    await waitFor(() => expect(mocked.addPostReaction).toHaveBeenCalledWith(expect.anything(), 'post-1', 'doge'));
    // 添加成功后左侧出现 狗头 图标（次数 1）
    await findByRole('button', { name: '查看 狗头 反应明细（1 次）' });
  });

  it('「+ 表情」选择器：已激活反应再次点击 → DELETE 撤销，图标消失', async () => {
    mocked.removePostReaction.mockResolvedValueOnce(undefined);
    const { getByRole, queryByRole, findByRole } = render(ReactionBar, { props: baseProps });
    expect(getByRole('button', { name: '查看 庆祝 反应明细（1 次）' })).toBeTruthy();
    await fireEvent.click(getByRole('button', { name: '添加表情反应' }));
    await fireEvent.click(await findByRole('button', { name: '庆祝' }));
    await waitFor(() => expect(mocked.removePostReaction).toHaveBeenCalledWith(expect.anything(), 'post-1', 'party'));
    // 撤销后计数归零，该图标不再展示
    expect(queryByRole('button', { name: /查看 庆祝 反应明细/ })).toBeNull();
  });

  it('单激活切换：已有激活反应时选择新表情 → 旧反应乐观移除，响应 counts 回同步并回传父级', async () => {
    mocked.addPostReaction.mockResolvedValueOnce({
      reaction: 'party',
      active: true,
      count: 1,
      counts: { '👍': 2, '🎉': 1 }
    });
    const onMutated = vi.fn();
    const { getByRole, findByRole } = render(ReactionBar, {
      props: {
        ...baseProps,
        reactions: [{ reaction: '👍', count: 3, active: true }],
        onReactionMutated: onMutated
      }
    });
    // 初始只有 点赞 图标（3 次）
    expect(getByRole('button', { name: '查看 点赞 反应明细（3 次）' })).toBeTruthy();
    await fireEvent.click(getByRole('button', { name: '添加表情反应' }));
    await fireEvent.click(await findByRole('button', { name: '庆祝' }));
    await waitFor(() =>
      expect(mocked.addPostReaction).toHaveBeenCalledWith(expect.anything(), 'post-1', 'party')
    );
    // 切换走 add（服务端原子删旧加新），不单独发 DELETE
    expect(mocked.removePostReaction).not.toHaveBeenCalled();
    // 响应 counts 回同步：点赞 2 次、庆祝 1 次，且只有新反应激活
    await waitFor(() =>
      expect(getByRole('button', { name: '查看 点赞 反应明细（2 次）' })).toBeTruthy()
    );
    expect(getByRole('button', { name: '查看 庆祝 反应明细（1 次）' })).toBeTruthy();
    // 回传父级联动
    expect(onMutated).toHaveBeenCalledWith({
      reaction: 'party',
      active: true,
      count: 1,
      counts: { '👍': 2, '🎉': 1 }
    });
  });

  it('撤销反应：响应含 counts 时以服务端计数回同步', async () => {
    mocked.removePostReaction.mockResolvedValueOnce({
      reaction: 'like',
      active: false,
      count: 2,
      counts: { '👍': 2, '🎉': 1 }
    });
    const onMutated = vi.fn();
    const { getByRole, findByRole } = render(ReactionBar, {
      props: {
        ...baseProps,
        reactions: [
          { reaction: 'like', count: 3, active: true },
          { reaction: '🎉', count: 1, active: false }
        ],
        onReactionMutated: onMutated
      }
    });
    await fireEvent.click(getByRole('button', { name: '添加表情反应' }));
    await fireEvent.click(await findByRole('button', { name: '点赞' }));
    await waitFor(() =>
      expect(mocked.removePostReaction).toHaveBeenCalledWith(expect.anything(), 'post-1', 'like')
    );
    // 响应 count=2（服务端）回同步
    expect(getByRole('button', { name: '查看 点赞 反应明细（2 次）' })).toBeTruthy();
    expect(onMutated).toHaveBeenCalledWith(
      expect.objectContaining({ reaction: 'like', active: false, count: 2 })
    );
  });

  it('切换请求失败：回滚乐观移除，恢复原激活项', async () => {
    mocked.addPostReaction.mockRejectedValueOnce({ status: 403, detail: 'forbidden' });
    const { getByRole, queryByRole, findByRole, findByRole: findRole } = render(ReactionBar, {
      props: {
        ...baseProps,
        reactions: [{ reaction: '👍', count: 3, active: true }]
      }
    });
    await fireEvent.click(getByRole('button', { name: '添加表情反应' }));
    await fireEvent.click(await findByRole('button', { name: '庆祝' }));
    const alert = await findRole('alert');
    expect(alert.textContent).toContain('你没有权限');
    // 乐观切换被回滚：点赞仍在（3 次），庆祝未出现
    expect(getByRole('button', { name: '查看 点赞 反应明细（3 次）' })).toBeTruthy();
    expect(queryByRole('button', { name: /查看 庆祝 反应明细/ })).toBeNull();
  });

  it('429 → 显示限流提示并禁用按钮（冷却期）', async () => {
    mocked.addPostReaction.mockRejectedValueOnce({ status: 429, detail: 'rate', retry_after: 60 });
    const { getByRole, findByRole } = render(ReactionBar, { props: baseProps });
    await fireEvent.click(getByRole('button', { name: '添加表情反应' }));
    await fireEvent.click(await findByRole('button', { name: '点赞' }));
    const alert = await findByRole('alert');
    expect(alert.textContent).toContain('操作过于频繁');
    expect(alert.textContent).toContain('60 秒');
    expect(getByRole('button', { name: '添加表情反应' }).hasAttribute('disabled')).toBe(true);
  });

  it('403 → 目标权限错误提示', async () => {
    mocked.addPostReaction.mockRejectedValueOnce({ status: 403, detail: 'forbidden' });
    const { getByRole, findByRole } = render(ReactionBar, { props: baseProps });
    await fireEvent.click(getByRole('button', { name: '添加表情反应' }));
    await fireEvent.click(await findByRole('button', { name: '点赞' }));
    const alert = await findByRole('alert');
    expect(alert.textContent).toContain('你没有权限');
  });

  it('未登录 → 提示登录且不发请求', async () => {
    const { getByRole, queryByRole, findByText } = render(ReactionBar, {
      props: { ...baseProps, authed: false }
    });
    await fireEvent.click(getByRole('button', { name: '登录后添加表情反应' }));
    expect(await findByText('登录后即可给内容添加表情互动')).toBeTruthy();
    expect(queryByRole('button', { name: '点赞' })).toBeNull();
    expect(mocked.addPostReaction).not.toHaveBeenCalled();
  });

  it('任何视角都不渲染通知提示（产品移除「可在通知设置中关闭」）', () => {
    const { container } = render(ReactionBar, { props: baseProps });
    expect(container.querySelectorAll('button.rx-pill').length).toBe(2);
    expect(container.textContent).not.toContain('反应可能通知作者');
    expect(container.textContent).not.toContain('通知设置');
    expect(container.textContent).not.toContain('收到反应会向你发送通知');
    expect(container.querySelector('a[href="/settings#settings-notifications"]')).toBeNull();
  });

  it('弹窗内 Tab 切换过滤用户（所有 / 单表情）', async () => {
    mocked.getPostReactions.mockResolvedValueOnce(detailFixture);
    const { getByRole, findByRole, findByText } = render(ReactionBar, { props: baseProps });
    await fireEvent.click(getByRole('button', { name: /查看 点赞 反应明细/ }));
    await findByRole('dialog', { name: '收到的表情' });
    // 弹层 portal 到 body（组件容器外），内容断言用 document.body
    const bodyText = () => document.body.textContent ?? '';
    // 默认在 👍 Tab：只见 lyfmya
    expect(await findByText('lyfmya')).toBeTruthy();
    expect(bodyText()).not.toContain('尘埃落定');
    // 切到 所有：两个用户都在
    await fireEvent.click(getByRole('button', { name: '所有' }));
    expect(await findByText('尘埃落定')).toBeTruthy();
    expect(bodyText()).toContain('lyfmya');
    // 切到 🎉 Tab：只剩 尘埃落定
    await fireEvent.click(getByRole('button', { name: '庆祝（1 次）' }));
    expect(bodyText()).not.toContain('lyfmya');
  });
});
