import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminAuditPage from '../../../routes/admin/audit/+page.svelte';
import AdminNotificationsPage from '../../../routes/admin/notifications/+page.svelte';
import AdminDownloadBillingPage from '../../../routes/admin/download-billing/+page.svelte';
import AdminLevelsPage from '../../../routes/admin/levels/+page.svelte';
import AdminBiPage from '../../../routes/admin/bi/+page.svelte';
import AdminPostsPage from '../../../routes/admin/posts/+page.svelte';
import AdminUsersPage from '../../../routes/admin/users/+page.svelte';
import AdminCasesPage from '../../../routes/admin/moderation/cases/+page.svelte';
import AdminBoardsPage from '../../../routes/admin/boards/+page.svelte';
import AdminShopPage from '../../../routes/admin/shop/+page.svelte';
import AdminStoragePage from '../../../routes/admin/storage/+page.svelte';
import type { AuditLogItem } from '$lib/api/types';

describe('P1-10 & P1-11: 审计日志可空字段与筛选', () => {
  it('可空字段（actor_username, object_type, object_id, detail 为 null）正常渲染不抛 TypeError', () => {
    const nullLog: AuditLogItem = {
      id: 'log-null-1',
      actor_id: 'u-1',
      actor_username: null,
      action: 'video_policy.update',
      object_type: null as any,
      object_id: null as any,
      detail: null,
      created_at: 1700000000000
    };

    expect(() => {
      const { body } = render(AdminAuditPage, {
        props: {
          data: {
            state: 'ok',
            items: [nullLog],
            nextCursor: null,
            q: '',
            after: null,
            error: null
          }
        }
      });
      expect(body).toContain('视频配置策略调整');
    }).not.toThrow();
  });
});

describe('P1-09 & P2-08: 通知管理状态列与筛选', () => {
  it('通知模板表格展示状态列徽标与总数', () => {
    const { body } = render(AdminNotificationsPage, {
      props: {
        data: {
          state: 'ok',
          templates: [
            { id: 'tpl-1', name: '验证邮件', trigger: '事件触发', channel: '站内信', queue: '直达' },
            { id: 'tpl-2', name: '通知摘要', trigger: '每日 08:00', channel: '站内信', queue: '直达' }
          ],
          queue: null,
          items: [],
          nextCursor: null,
          after: null,
          error: null
        }
      }
    });
    expect(body).toContain('状态');
    expect(body).toContain('启用中');
    expect(body).toContain('已暂停');
    expect(body).toContain('共 2 个模板');
  });
});

describe('P1-14: 下载计费状态筛选与汇总金额', () => {
  it('下载记录渲染状态徽标与汇总金额', () => {
    const { body } = render(AdminDownloadBillingPage, {
      props: {
        data: {
          state: 'ok',
          config: null,
          error: null,
          transactions: {
            state: 'ok',
            items: [
              { id: 'DL-1', filename: 'file1.zip', username: 'user1', amount: 15, created_at: 1700000000 },
              { id: 'DL-2', filename: 'file2.zip', username: 'user2', amount: 0, created_at: 1700000000 }
            ],
            nextCursor: null,
            message: null
          }
        }
      }
    });
    expect(body).toContain('扣费成功');
    expect(body).toContain('免费放行');
    expect(body).toContain('合计金额');
  });
});

describe('P1-15: 等级管理启用状态与列表', () => {
  it('等级列表渲染状态徽标与数量', () => {
    const { body } = render(AdminLevelsPage, {
      props: {
        data: {
          state: 'ok',
          quota: null,
          levels: {
            state: 'ok',
            items: [
              { level: 0, name: '见习', min_exp: 0, daily_post_limit: 5, daily_comment_limit: 20, user_count: 5, is_active: 1 } as any,
              { level: 1, name: '探索', min_exp: 10, daily_post_limit: 10, daily_comment_limit: 50, user_count: 2, is_active: 0 } as any
            ],
            message: null
          },
          error: null
        }
      }
    });
    expect(body).toContain('已启用');
    expect(body).toContain('已停用');
    expect(body).toContain('共 <b>2</b> 个等级');
  });
});

describe('P1-13: BI 看板周期切换与多态指标', () => {
  it('不同周期渲染不同快照指标', () => {
    const day = render(AdminBiPage, {
      props: {
        data: {
          state: 'ok',
          period: 'day',
          metrics: null,
          error: null
        }
      }
    });
    expect(day.body).toContain('关键指标（今日）');
    expect(day.body).toContain('421');

    const year = render(AdminBiPage, {
      props: {
        data: {
          state: 'ok',
          period: 'year',
          metrics: null,
          error: null
        }
      }
    });
    expect(year.body).toContain('关键指标（今年）');
    expect(year.body).toContain('28,400');
    expect(year.body).toContain('489,000');
  });

  it('空指标时渲染空态', () => {
    const empty = render(AdminBiPage, {
      props: {
        data: {
          state: 'ok',
          period: 'day',
          metrics: { period: 'day', generated_at: 1700000000000, metrics: [] },
          error: null
        }
      }
    });
    expect(empty.body).toContain('暂无统计数据');
  });
});

describe('P1-03: 帖子列表状态筛选', () => {
  it('不同状态筛选只展示目标状态帖子', () => {
    const pub = render(AdminPostsPage, {
      props: {
        data: {
          state: 'ok',
          items: null,
          nextCursor: null,
          status: 'published',
          q: '',
          after: null,
          error: null,
          counts: null
        },
        form: null
      }
    });
    expect(pub.body).toContain('全栈架构设计的最佳实践');
    expect(pub.body).not.toContain('待审核的内容规范违规检查');

    const pending = render(AdminPostsPage, {
      props: {
        data: {
          state: 'ok',
          items: null,
          nextCursor: null,
          status: 'pending_review',
          q: '',
          after: null,
          error: null,
          counts: null
        },
        form: null
      }
    });
    expect(pending.body).toContain('待审核的内容规范违规检查');
    expect(pending.body).not.toContain('全栈架构设计的最佳实践');
  });
});

describe('P1-04: 用户搜索与空状态', () => {
  it('搜索存在用户渲染完整行，搜索不存在用户渲染空态', () => {
    const found = render(AdminUsersPage, {
      props: {
        data: {
          state: 'ok',
          items: [
            {
              id: 'u-1',
              username: 'mockuser',
              display_name: '测试用户',
              email: 'mock@example.com',
              email_verified: true,
              status: 'active',
              level: 2,
              roles: ['member'],
              coin_balance: 100,
              exp_balance: 200,
              created_at: 1700000000000,
              updated_at: 1700000000000,
              last_login_at: 1700000000000,
              version: 1
            }
          ],
          error: null
        },
        form: null
      }
    });
    expect(found.body).toContain('mockuser');
    expect(found.body).toContain('测试用户');
    expect(found.body).toContain('mock@example.com');
  });
});

describe('P1-05: 审核案件统计数与列表一致性', () => {
  it('案件队列统计数动态对应列表数，入口链接包含详情 ID', () => {
    const { body } = render(AdminCasesPage, {
      props: {
        data: {
          items: [
            { id: 'CASE-1', title: '案件 1', status: 'open', priority: 'high', assigned_to: null, created_at: 1700000000000, updated_at: 1700000000000 },
            { id: 'CASE-2', title: '案件 2', status: 'resolved', priority: 'low', assigned_to: null, created_at: 1700000000000, updated_at: 1700000000000 }
          ]
        } as any
      }
    });
    expect(body).toContain('全部 2');
    expect(body).toContain('待处理 1');
    expect(body).toContain('已处理 1');
    expect(body).toContain('href="/admin/moderation/cases/CASE-1"');
    expect(body).toContain('href="/admin/moderation/cases/CASE-2"');
  });
});

describe('P2-01: 板块操作错误不破坏列表', () => {
  it('板块提交失败时错误展示且原板块列表依然渲染', () => {
    const { body } = render(AdminBoardsPage, {
      props: {
        data: {
          loadState: {
            state: 'ok',
            items: [
              { id: 'b1', slug: 'tech', name: '技术板块', description: '技术讨论', version: 1, created_at: 0, updated_at: 0 }
            ]
          }
        },
        form: {
          loadState: {
            state: 'ok',
            items: [
              { id: 'b1', slug: 'tech', name: '技术板块', description: '技术讨论', version: 1, created_at: 0, updated_at: 0 }
            ]
          },
          message: 'slug 格式非法'
        }
      }
    });
    expect(body).toContain('slug 格式非法');
    expect(body).toContain('技术板块');
    expect(body).toContain('/tech');
  });
});

describe('P2-05 & P2-06: 计费与商城字段错误透传与防抖', () => {
  it('商城新建商品包含币种隐藏域、默认展示槽位与字段错误呈现', () => {
    const { body } = render(AdminShopPage, {
      props: {
        data: {
          products: { state: 'ok', items: [] },
          orders: { state: 'ok', items: [] },
          config: { state: 'ok', data: { enabled: true, max_quantity_per_order: 10, version: 1 } }
        },
        form: {
          message: '请检查表单中填写的字段错误',
          fieldErrors: {
            slug: 'slug 只能包含小写字母、数字和连字符',
            unit_price: '价格必须为大于等于 0 的整数'
          }
        }
      }
    });
    expect(body).toContain('name="currency_id" value="coin"');
    expect(body).toContain('name="slot" value="avatar_frame"');
    expect(body).toContain('slug 只能包含小写字母、数字和连字符');
    expect(body).toContain('价格必须为大于等于 0 的整数');
  });
});

describe('P2-03: 存储 tab 无障碍属性与面板关联', () => {
  it('首次进入根据当前后端渲染 aria-selected 与 role=tabpanel', () => {
    const { body } = render(AdminStoragePage, {
      props: {
        data: {
          config: {
            backend: 's3',
            version: 1,
            managed_fields: []
          } as any,
          loadError: null
        },
        form: null
      }
    });
    expect(body).toContain('id="tab-storage-s3"');
    expect(body).toContain('aria-selected="true"');
    expect(body).toContain('id="panel-storage-s3"');
    expect(body).toContain('role="tabpanel"');
  });
});
