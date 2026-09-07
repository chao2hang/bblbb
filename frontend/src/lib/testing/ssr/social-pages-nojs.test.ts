// GAP-FIX（社交域页面）：/messages、/favorites、/achievements、/apikeys
// 无 JS SSR 基线——页面在 load 数据（含空态/失败态）下可直出，表单在
// SSR 中可提交（原生 form action）。动作分支由 route-matrix 与后端测试
// 覆盖，这里只验证渲染不抛错 + 关键内容存在。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import Messages from '../../../routes/messages/+page.svelte';
import Favorites from '../../../routes/favorites/+page.svelte';
import Achievements from '../../../routes/achievements/+page.svelte';
import ApiKeys from '../../../routes/apikeys/+page.svelte';

const TS = 1_700_000_000_000;

const conversations = [
  {
    id: 'c1',
    other: { username: 'alice', display_name: 'Alice', level: 3 },
    last_message: { body: '你好呀', created_at: TS, sender_username: 'alice' },
    unread_count: 2,
    updated_at: TS
  },
  {
    id: 'c2',
    other: { username: 'bob', display_name: null, level: 1 },
    last_message: { body: '在吗？', created_at: TS, sender_username: 'bob' },
    unread_count: 0,
    updated_at: TS
  }
];

describe('GAP-FIX /messages SSR', () => {
  it('无选中会话：渲染会话列表 + 未读角标 + 空线程引导', () => {
    const { body } = render(Messages, {
      props: {
        data: {
          conversations,
          conversationId: null,
          conversation: null,
          messages: [],
          problem: null,
          error: null,
          threadProblem: null,
          threadError: null,
          clientRequestId: 'csr-test-000000000001'
        }
      }
    });
    expect(body).toContain('alice');
    expect(body).toContain('你好呀');
    expect(body).toContain('选择一个会话');
    // 会话链接（?c=）供无 JS 导航。
    expect(body).toMatch(/href="\/messages\?c=c1"/);
  });

  it('选中会话：渲染线程气泡 + 发送表单（?/send，textarea 1-2000）', () => {
    const { body } = render(Messages, {
      props: {
        data: {
          conversations,
          conversationId: 'c1',
          conversation: conversations[0],
          messages: [
            { id: 'm1', sender_username: 'alice', body: '你好呀', created_at: TS },
            { id: 'm2', sender_username: 'me', body: '在的，什么事？', created_at: TS + 1000 }
          ],
          problem: null,
          error: null,
          threadProblem: null,
          threadError: null,
          clientRequestId: 'csr-test-000000000002'
        }
      }
    });
    expect(body).toContain('在的，什么事？');
    expect(body).toMatch(/<form[^>]*action="\?\/send"/);
    expect(body).toContain('name="body"');
    expect(body).toContain('maxlength="2000"');
    expect(body).toContain('name="conversation_id" value="c1"');
  });

  it('空态：还没有私信 + 引导', () => {
    const { body } = render(Messages, {
      props: {
        data: {
          conversations: [],
          conversationId: null,
          conversation: null,
          messages: [],
          problem: null,
          error: null,
          threadProblem: null,
          threadError: null,
          clientRequestId: 'csr-test-000000000003'
        }
      }
    });
    expect(body).toContain('还没有私信');
    expect(body).toContain('去关注的人那里打个招呼');
  });

  it('失败态（瞬态 503）：页面只留「加载失败·重试」占位，不整页渲染错误态', () => {
    const { body } = render(Messages, {
      props: {
        data: {
          conversations: [],
          conversationId: null,
          conversation: null,
          messages: [],
          problem: { status: 503, detail: '服务暂不可用' },
          error: '服务暂不可用',
          threadProblem: null,
          threadError: null,
          clientRequestId: 'csr-test-000000000004'
        }
      }
    });
    // 瞬态服务端错误（5xx/429）→ 全局 Toast 提示（有 JS 时弹，含请求号）；
    // 页面（含无 JS 基线）只留中性占位 + 重试入口，错误细节不进页面主体。
    expect(body).toContain('加载失败');
    expect(body).toContain('重试');
    expect(body).not.toContain('服务暂不可用');
  });

  it('失败态（持续性 403）：仍整页渲染 ProblemState', () => {
    const { body } = render(Messages, {
      props: {
        data: {
          conversations: [],
          conversationId: null,
          conversation: null,
          messages: [],
          problem: { status: 403, code: 'forbidden' },
          error: '你没有权限执行此操作',
          threadProblem: null,
          threadError: null,
          clientRequestId: 'csr-test-000000000008'
        }
      }
    });
    expect(body).toContain('没有权限');
    expect(body).toContain('你没有权限执行此操作');
  });
});

describe('GAP-FIX /favorites SSR', () => {
  const base = {
    nextCursor: 'cursor-1',
    hasMore: true,
    after: null,
    problem: null,
    error: null
  };

  it('渲染收藏行 + 取消收藏表单 + 加载更多', () => {
    const { body } = render(Favorites, {
      props: {
        data: {
          ...base,
          items: [
            {
              id: 'p1',
              title: '收藏的帖子',
              board_slug: 'tech',
              board_name: '技术',
              author: { id: 'u1', username: 'alice' },
              reply_count: 3,
              view_count: 10,
              created_at: TS,
              last_reply_at: null
            }
          ]
        }
      }
    });
    expect(body).toContain('收藏的帖子');
    expect(body).toMatch(/<form[^>]*action="\?\/unfavorite"/);
    expect(body).toContain('name="post_id" value="p1"');
    expect(body).toContain('加载更多');
    expect(body).toMatch(/after=cursor-1/);
  });

  it('空态：引导去首页', () => {
    const { body } = render(Favorites, {
      props: {
        data: { ...base, items: [], nextCursor: null, hasMore: false }
      }
    });
    expect(body).toContain('还没有收藏');
    expect(body).toContain('去首页逛逛');
  });

  it('失败态（瞬态 500）：只留「加载失败·重试」占位，不整页渲染错误态', () => {
    const { body } = render(Favorites, {
      props: {
        data: {
          ...base,
          items: [],
          nextCursor: null,
          hasMore: false,
          problem: { status: 500, code: 'internal_error' },
          error: '服务器开小差了，请稍后重试'
        }
      }
    });
    expect(body).toContain('加载失败');
    expect(body).toContain('重试');
    expect(body).not.toContain('服务器开小差了');
  });
});

describe('GAP-FIX /achievements SSR', () => {
  const cards = [
    {
      code: 'first_post',
      name: '首发帖',
      description: '发布第一篇帖子',
      category: 'community',
      rewardExp: 10,
      rewardCoin: 5,
      isHidden: false,
      unlocked: true,
      unlockedAt: TS,
      progress: 1,
      target: 1,
      equipped: false
    },
    {
      code: 'streak7',
      name: '连续签到 7 天',
      description: '连续签到一周',
      category: 'activity',
      rewardExp: 30,
      rewardCoin: 15,
      isHidden: false,
      unlocked: true,
      unlockedAt: TS,
      progress: 7,
      target: 7,
      equipped: true
    },
    {
      code: 'hundred_posts',
      name: '百帖',
      description: '发布 100 篇帖子',
      category: 'community',
      rewardExp: 100,
      rewardCoin: 50,
      isHidden: false,
      unlocked: false,
      unlockedAt: null,
      progress: 30,
      target: 100,
      equipped: false
    },
    {
      code: 'secret',
      name: '???',
      description: '隐藏成就：达成条件保密，解锁后揭晓',
      category: 'secret',
      rewardExp: 20,
      rewardCoin: 20,
      isHidden: true,
      unlocked: false,
      unlockedAt: null,
      progress: 0,
      target: 0,
      equipped: false
    }
  ];

  it('总览卡（解锁/装备槽/进度）+ 卡片网格（装备/卸下/进行中/???）', () => {
    const { body } = render(Achievements, {
      props: {
        data: {
          cards,
          stats: { unlocked: 2, total: 4, equipped: 1, maxSlots: 3 },
          problem: null,
          error: null
        }
      }
    });
    // 总览：2/4 已解锁、1/3 已装备。
    expect(body).toContain('2/4');
    expect(body).toContain('1/3');
    // 未装备的已解锁卡：装备表单；已装备卡：卸下表单 + 徽章。
    expect(body).toMatch(/<form[^>]*action="\?\/equip"/);
    expect(body).toMatch(/<form[^>]*action="\?\/unequip"/);
    expect(body).toContain('已装备');
    // 进行中：进度 30/100。
    expect(body).toContain('30/100');
    // 隐藏未解锁：???。
    expect(body).toContain('???');
    expect(body).toContain('隐藏成就：达成条件保密');
  });

  it('空态：成就未配置', () => {
    const { body } = render(Achievements, {
      props: {
        data: {
          cards: [],
          stats: { unlocked: 0, total: 0, equipped: 0, maxSlots: 3 },
          problem: null,
          error: null
        }
      }
    });
    expect(body).toContain('暂无成就');
  });
});

describe('GAP-FIX /apikeys SSR', () => {
  const keys = [
    {
      id: 'k1',
      name: '我的脚本',
      prefix: 'bblbb_ab12',
      scopes: ['posts:read', 'me:read'],
      created_at: TS,
      last_used_at: TS + 5000,
      revoked_at: null
    },
    {
      id: 'k2',
      name: '旧密钥',
      prefix: 'bblbb_cd34',
      scopes: [],
      created_at: TS - 100000,
      last_used_at: null,
      revoked_at: TS - 50000
    }
  ];

  it('表格（名称/前缀/权限/状态）+ 创建表单（scope 复选）+ 撤销', () => {
    const { body } = render(ApiKeys, {
      props: {
        data: { keys, problem: null, error: null, clientRequestId: 'csr-test-000000000005' }
      }
    });
    expect(body).toContain('我的脚本');
    expect(body).toContain('bblbb_ab12');
    expect(body).toContain('posts:read');
    expect(body).toContain('有效');
    expect(body).toContain('已撤销');
    // 创建表单：名称 + 4 个 scope 复选 + 幂等键。
    expect(body).toMatch(/<form[^>]*action="\?\/create"/);
    expect(body).toContain('name="name"');
    expect(body).toContain('name="scopes" value="drafts:write"');
    expect(body).toContain('name="client_request_id"');
    // 未撤销行才有撤销表单。
    expect(body).toMatch(/<form[^>]*action="\?\/revoke"/);
    expect(body).toContain('name="id" value="k1"');
    expect(body).not.toContain('name="id" value="k2"');
  });

  it('一次性密钥展示框（仅此一次 + 复制）', () => {
    const { body } = render(ApiKeys, {
      props: {
        data: { keys, problem: null, error: null, clientRequestId: 'csr-test-000000000006' },
        form: {
          ok: true,
          message: '密钥已创建',
          created: {
            id: 'k3',
            name: '新密钥',
            prefix: 'bblbb_ef56',
            scopes: ['posts:read'],
            key: 'bblbb_plaintext_secret_key'
          }
        }
      }
    });
    expect(body).toContain('仅此一次显示');
    expect(body).toContain('bblbb_plaintext_secret_key');
    expect(body).toContain('复制密钥');
  });

  it('空态：还没有密钥', () => {
    const { body } = render(ApiKeys, {
      props: {
        data: { keys: [], problem: null, error: null, clientRequestId: 'csr-test-000000000007' }
      }
    });
    expect(body).toContain('还没有 API 密钥');
  });
});
