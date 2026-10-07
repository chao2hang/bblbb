// 积分日志页测试：全站流水查询、B币归一化与筛选参数。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type AdminPointsLogsPageData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn(),
  authedPost: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function createLoadEvent(queryString = '') {
  const url = new URL(`http://localhost/admin/points/logs${queryString ? `?${queryString}` : ''}`);
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers: new Headers() },
    url
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

describe('积分日志（全站流水）', () => {
  it('load: 资产类型 asset=b_coin 自动归一化为 coin 传给后端', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: { items: [], next_cursor: null }
    });

    const event = createLoadEvent('asset=b_coin');
    const data = (await load(event)) as AdminPointsLogsPageData;

    expect(data.ledger.state).toBe('ok');
    expect(data.ledger.filters.asset).toBe('coin');
    const ledgerApiCall = getAuthedMock.mock.calls[0][1] as string;
    expect(ledgerApiCall).toContain('asset=coin');
    expect(ledgerApiCall).not.toContain('asset=b_coin');
  });

  it('load: 透传 username, kind, from, to 过滤参数', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          {
            id: 'tx-1',
            username: 'chaos',
            kind: 'credit',
            currency: 'coin',
            amount: 50,
            balance_after: 150,
            source_type: 'check_in',
            memo: '签到奖励',
            created_at: 1700000000000
          }
        ],
        next_cursor: 'cursor-123'
      }
    });

    const event = createLoadEvent('username=chaos&kind=credit&from=2026-01-01&to=2026-01-02');
    const data = (await load(event)) as AdminPointsLogsPageData;

    expect(data.ledger.state).toBe('ok');
    expect(data.ledger.items).toHaveLength(1);
    expect(data.ledger.nextCursor).toBe('cursor-123');
    expect(data.ledger.filters.username).toBe('chaos');
    expect(data.ledger.filters.kind).toBe('credit');

    const ledgerApiCall = getAuthedMock.mock.calls[0][1] as string;
    expect(ledgerApiCall).toContain('username=chaos');
    expect(ledgerApiCall).toContain('kind=credit');
  });

  it('load: 403 权限不足返回 forbidden 状态', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      message: 'Forbidden'
    });

    const event = createLoadEvent();
    const data = (await load(event)) as AdminPointsLogsPageData;

    expect(data.ledger.state).toBe('forbidden');
    expect(data.ledger.items).toEqual([]);
    expect(data.ledger.message).toBe('Forbidden');
  });

  it('formatLedgerAction: 正确将英文操作明细本地化为中文', async () => {
    const { formatLedgerAction } = await import('$lib/points/format');
    expect(formatLedgerAction('shop purchase 磁带 x1', 'shop_purchase', -100)).toBe('商城购买 磁带 x1');
    expect(formatLedgerAction('check_in 奖励', 'award', 10)).toBe('签到奖励');
    expect(formatLedgerAction('shop purchase 梦幻色彩 x1', 'shop_purchase', -2000)).toBe('商城购买 梦幻色彩 x1');
  });
});
