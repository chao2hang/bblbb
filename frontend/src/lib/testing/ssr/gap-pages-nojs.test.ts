// GAP-FIX（本批页面）：/discover、/tags/[slug]、/marketplace、
// /me/billing 无 JS SSR 基线——页面在 load 数据（含空态/失败态）下可直出，
// 表单在 SSR 中可提交（原生 form action）。只验证渲染不抛错 + 关键内容存在。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import Discover from '../../../routes/discover/+page.svelte';
import TagDetail from '../../../routes/tags/[slug]/+page.svelte';
import Marketplace from '../../../routes/marketplace/+page.svelte';
import Billing from '../../../routes/me/billing/+page.svelte';

const TS = 1_700_000_000_000;

/** Board 完整投影（types.ts Board 必填字段齐备）。 */
const board = {
  id: 'b1',
  slug: 'tech',
  name: '技术',
  description: '技术讨论',
  icon: null,
  version: 1,
  created_at: TS,
  updated_at: TS,
  post_count: 5
};

describe('GAP-FIX /discover SSR', () => {
  it('推荐流：为你推荐 + 帖子行（无标签入口、无排序 tab）', () => {
    const { body } = render(Discover, {
      props: {
        data: {
          posts: [
            {
              id: 'p1',
              title: '热门讨论',
              board_id: 'b1',
              board_slug: 'tech',
              board_name: '技术',
              author: { id: 'u1', username: 'alice' },
              post_type: 'discussion',
              reply_count: 3,
              view_count: 10,
              pinned: false,
              created_at: TS,
              last_reply_at: null
            }
          ],
          boards: [board],
          error: null
        }
      }
    });
    expect(body).toContain('为你推荐');
    expect(body).toContain('热门讨论');
    expect(body).toContain('技术');
    // 算法推送产品决策：不再有手动排序 tab 与热门标签入口
    expect(body).not.toMatch(/href="\/discover\?sort=/);
    expect(body).not.toMatch(/href="\/tags\//);
    expect(body).not.toContain('热门标签');
  });

  it('空态：推荐流为空不抛错（给出发布引导）', () => {
    const { body } = render(Discover, { props: { data: { posts: [], boards: [], error: null } } });
    expect(body).toContain('还没有可推荐的内容');
  });
});

describe('GAP-FIX /tags/[slug] SSR', () => {
  it('标签标题 + 帖子列表 + 分页', () => {
    const { body } = render(TagDetail, {
      props: {
        data: {
          slug: 'rust',
          tag: { id: 't1', slug: 'rust', name: 'Rust', description: 'Rust 话题', color: null, group_id: null, usage_count: 9 },
          posts: [
            { id: 'p1', title: '标签下的帖子', author: { id: 'u1', username: 'bob' }, reply_count: 1, view_count: 2, created_at: TS, last_reply_at: null }
          ],
          hasMore: true,
          nextCursor: 'cursor-1',
          unavailable: false
        }
      }
    });
    expect(body).toContain('Rust');
    expect(body).toContain('标签下的帖子');
    expect(body).toMatch(/href="\/tags\/rust\?after=cursor-1"/);
  });

  it('端点不可用：空态 + 发布引导（不报错）', () => {
    const { body } = render(TagDetail, {
      props: { data: { slug: 'gone', tag: null, posts: [], hasMore: false, nextCursor: null, unavailable: true } }
    });
    expect(body).toContain('这个标签下还没有内容');
    expect(body).toContain('标签内容暂时加载失败');
    expect(body).toContain('登录后发布');
    expect(body).toMatch(/href="\/login\?next=%2Feditor"/);
  });
});

describe('GAP-FIX /marketplace SSR', () => {
  it('静态精选卡 + 交易摘要表 + 查看全部购买', () => {
    const { body } = render(Marketplace, {
      props: {
        data: {
          purchases: [
            {
              id: 'buy1',
              intent_id: 'i1',
              client_id: 'client-uuid-1234',
              user_id: 'u1',
              offer_id: 'o1',
              offer_version: 1,
              quantity: 1,
              amount: 10,
              fee_amount: 1,
              merchant_net: 9,
              currency_id: 'coin',
              status: 'succeeded',
              refunded_amount: 0,
              merchant_order_id: 'm1',
              created_at: TS,
              updated_at: TS
            }
          ],
          totals: { count: 1, spent: 10, fees: 1, currency: 'coin' },
          error: null
        }
      }
    });
    expect(body).toContain('市场应用由管理员配置');
    expect(body).toContain('BBLBB CLI 工具');
    expect(body).toMatch(/href="\/marketplace\/purchases"/);
    expect(body).toContain('交易成功');
  });

  it('空态：暂无市场交易', () => {
    const { body } = render(Marketplace, {
      props: { data: { purchases: [], totals: { count: 0, spent: 0, fees: 0, currency: null }, error: null } }
    });
    expect(body).toContain('暂无市场交易');
  });
});

describe('GAP-FIX /me/billing SSR', () => {
  const row = {
    id: 'tx1',
    created_at: TS,
    currency: 'coin',
    amount: 10,
    balance_after: 90,
    attachment_id: 'att1',
    attachment_name: 'report.zip',
    authorization_id: null
  };

  it('统计卡 + 流水表（时间/附件/金额/状态/操作）+ 安全提示卡', () => {
    const { body } = render(Billing, {
      props: {
        data: {
          summary: { checked_in_today: true, streak_days: 2, balances: [{ currency: 'coin', amount: 328 }] },
          rows: [row],
          totals: { count: 1, spentCoin: 10, lastAt: TS },
          error: null
        },
        form: null
      }
    });
    // 币种显示「B币」（对齐原型，原断言为英文 key「COIN」）
    expect(body).toContain('328 B币');
    expect(body).toContain('report.zip');
    expect(body).toContain('10 B币');
    expect(body).toMatch(/action="\?\/sign"/);
    expect(body).toContain('重新下载');
    expect(body).toContain('前端永远不会接触对象存储 Secret');
  });

  it('空态：暂无下载记录', () => {
    const { body } = render(Billing, {
      props: {
        data: { summary: null, rows: [], totals: { count: 0, spentCoin: 0, lastAt: null }, error: null },
        form: null
      }
    });
    expect(body).toContain('暂无下载记录');
  });
});
