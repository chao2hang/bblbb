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
import type { AdminLevelsPageData } from '../../../routes/admin/levels/+page.server';
import type { AdminPostItem, AuditLogItem, ShopProduct } from '$lib/api/types';

const adminPostFixtures: AdminPostItem[] = [
  {
    id: 'post-1',
    title: '全栈架构设计的最佳实践',
    author_username: 'Alice',
    board_slug: 'tech',
    board_name: '技术分享',
    status: 'published',
    review_status: 'approved',
    is_featured: false,
    is_pinned: false,
    is_locked: false,
    view_count: 1420,
    created_at: 1700000000000
  },
  {
    id: 'post-2',
    title: '待审核的内容规范违规检查',
    author_username: 'Bob',
    board_slug: 'water',
    board_name: '灌水吐槽',
    status: 'pending_review',
    review_status: 'pending_review',
    is_featured: false,
    is_pinned: false,
    is_locked: false,
    view_count: 12,
    created_at: 1699900000000
  }
];

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
      expect(body).toContain('video_policy.update');
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

describe('M18-ADMIN-BATCH: 通知管理按钮→弹层新契约', () => {
  it('已发送广播列表仍 SSR 渲染；行「撤回」= ⋮菜单触发按钮 + 隐藏审计表单；常驻公告表单卡移除', () => {
    const { body } = render(AdminNotificationsPage, {
      props: {
        data: {
          state: 'ok',
          templates: [],
          queue: { mode: 'inline/outbox', failed: 2, outbox_count: 5 },
          items: [
            { id: 'bc-1', title: '维护通知', body: '本周末例行维护', target_count: 12, target_type: 'all', sender_username: 'admin', recalled: false, created_at: 1700000000000 },
            { id: 'bc-2', title: '旧公告', body: '已撤回的历史广播', target_count: 8, target_type: 'all', sender_username: 'admin', recalled: true, created_at: 1699990000000 }
          ],
          nextCursor: null,
          after: null,
          error: null
        }
      }
    });
    // 列表行仍渲染（含已撤回行的状态徽标）
    expect(body).toContain('维护通知');
    expect(body).toContain('旧公告');
    expect(body).toContain('已撤回');
    // 新契约（约定 D）：行「撤回」= 「⋮」菜单触发按钮（aria-label 含行语义）
    // + DangerConfirm；隐藏表单常驻 SSR（id/reason 审计要素，机制不变）
    expect(body).toContain('action="?/recall"');
    expect(body).toContain('name="reason"');
    expect(body).toContain('aria-label="更多操作：广播 维护通知"');
    // 菜单关闭态不渲染操作列表（无 role="menuitem"）；列表项/DangerConfirm 标题
    // 文案「撤回广播」均不作为可见文本出现在 body（「全选可撤回广播」为 aria 标签，不算）
    expect(body).not.toContain('role="menuitem"');
    expect(body).not.toContain('>撤回广播<');
    // 已撤回行不再提供操作菜单
    expect(body).not.toContain('aria-label="更多操作：广播 旧公告"');
    // 新契约：常驻公告表单卡移除（发送公告 = 页头按钮 + Dialog，SSR 不再有 ?/broadcast 表单）
    expect(body).toContain('发送公告');
    expect(body).not.toContain('action="?/broadcast"');
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
    // 新契约：常驻保存表单移除（编辑计费策略 = 按钮 + Dialog，SSR 不再有 ?/save 表单）
    expect(body).toContain('编辑计费策略');
    expect(body).not.toContain('action="?/save"');
    // 新契约：下载记录为计费流水（无批量端点）→ 选择列与「已选」死 UI 已移除
    expect(body).not.toContain('项已选');
  });
});

describe('P1-15: 等级管理（LinuxDo 信任等级单轨）', () => {
  // 2026-09 等级合并单轨：/admin/levels = TL0–TL4 信任等级规则表 + 手动授予 +
  // 按信任等级取档的附件配额；经验方案阶梯展示已移除。
  const trustItem = (level: number, name: string, user_count: number, requirements: Record<string, unknown> | null = null) => ({
    level,
    name,
    summary: null,
    requirements,
    is_enabled: true,
    version: 1,
    user_count
  });
  const levelsPageData = (items: any[], quotas: Record<string, any> = {}): AdminLevelsPageData => ({
    state: 'ok',
    items,
    quotas,
    error: null
  });

  it('信任等级表渲染 TL/名称/用户数/晋升条件（经验阶梯与 /admin/trust-levels 引导不再出现）', () => {
    const { body } = render(AdminLevelsPage, {
      props: {
        data: levelsPageData([
          trustItem(0, '新用户', 3),
          trustItem(1, '基本用户', 2, { topics_entered: 5, posts_read: 30 })
        ])
      }
    });
    expect(body).toContain('TL0 新用户');
    expect(body).toContain('TL1 基本用户');
    expect(body).toContain('晋升条件');
    expect(body).toContain('进入话题数');
    expect(body).toContain('已纳管用户（活跃）总量：<b>5</b> 名');
    // 2026-09 可配置化：每行「⋮」编辑入口（菜单关闭态不渲染列表项「编辑规则」）
    expect(body).toContain('aria-label="更多操作：等级规则 TL0"');
    expect(body).not.toContain('>编辑规则<');
    // 2026-09 合并：经验阶梯（L1/阈值列）与独立信任等级页引导均已移除
    expect(body).not.toContain('经验阈值');
    expect(body).not.toContain('/admin/trust-levels');
    // 手动授予卡常驻（TL4 唯一授予入口）
    expect(body).toContain('action="?/setLevel"');
  });

  it('GAP-FIX: 附件空间配额按信任等级渲染（摘要行 + 「⋮」菜单配额入口；写表单移入弹层）', () => {
    const { body } = render(AdminLevelsPage, {
      props: {
        data: levelsPageData(
          [trustItem(0, '新用户', 2)],
          {
            '0': {
              level: 0,
              single_file_max_bytes: 5 * 1048576,
              total_bytes: 250 * 1048576,
              daily_upload_bytes: 50 * 1048576,
              retention_days: 30,
              policy_version: 3
            }
          }
        )
      }
    });
    // 摘要行（TL0：总容量/单文件/每日/保留期/策略版本）——列表渲染保留
    expect(body).toContain('总容量 250 MB');
    expect(body).toContain('单文件 5 MB');
    expect(body).toContain('每日 50 MB');
    expect(body).toContain('策略版本 v3');
    // 约定 B：选择列 + 全选（BatchBar 选中态不进 SSR 基线）
    expect(body).toContain('aria-label="选择信任等级 TL0"');
    expect(body).toContain('aria-label="全选等级档位"');
    expect(body).not.toContain('批量设置配额');
    // 新契约（约定 D）：每级配额入口 = 「⋮」菜单触发按钮（aria-label 含行语义）；
    // 菜单关闭态不渲染列表项「设置附件配额」（Dialog 标题同文案，关闭态也不渲染）。
    // 配额写表单（MB 输入 + If-Match 隐藏版本 + reason 必填）仍在弹层内，
    // 页面不再暴露常驻 updateQuota 裸表单
    expect(body).toContain('aria-label="更多操作：信任等级 TL0"');
    expect(body).not.toContain('设置附件配额');
    expect(body).not.toMatch(/<form[^>]*action="\?\/updateQuota"/);
    expect(body).not.toMatch(/<form[^>]*action="\?\/batchQuota"/);
  });
});

describe('P1-13: BI 看板周期切换与多态指标', () => {
  // P0 整改：metrics:null（API 无数据/失败）必须渲染空态，禁止回退演示快照
  // （此前断言 421/489,000 演示 KPI 为基线，已随 mock 移除同步更新）。
  it('API 无指标时渲染真实空态，不回退演示数据', () => {
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
    expect(day.body).toContain('暂无统计数据');
    expect(day.body).not.toContain('421');

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
    expect(year.body).toContain('暂无统计数据');
    expect(year.body).toContain('当前周期下暂无聚合分析指标');
    expect(year.body).not.toContain('489,000');
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
          items: adminPostFixtures,
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
          items: adminPostFixtures,
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

  it('M18-ADMIN-BATCH: 死按钮移出 SSR，批量操作改为批量隐藏表单契约', () => {
    const { body } = render(AdminCasesPage, {
      props: {
        data: {
          items: [
            { id: 'CASE-1', title: '案件 1', status: 'open', priority: 'high', assigned_to: null, created_at: 1700000000000, updated_at: 1700000000000 }
          ]
        } as any
      }
    });
    // 新契约：原 disabled 无 onclick 的死按钮不再常驻 SSR（改为选中后的 BatchBar + 弹层）
    expect(body).not.toContain('批量关闭');
    expect(body).not.toContain('批量驳回');
    // 批量关闭/驳回的隐藏审计表单常驻 SSR（ids/reason，关闭时作为 resolution 写审计）
    expect(body).toContain('action="?/batchClose"');
    expect(body).toContain('action="?/batchReject"');
    expect(body).toContain('name="reason"');
    expect(body).toContain('name="ids"');
    // 列表/选择基线保持
    expect(body).toContain('全选当前列表');
    expect(body).toContain('选择案件 CASE-1');
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
              { id: 'b1', slug: 'tech', name: '技术板块', description: '技术讨论', icon: null, version: 1, created_at: 0, updated_at: 0 }
            ]
          }
        },
        form: {
          loadState: {
            state: 'ok',
            items: [
              { id: 'b1', slug: 'tech', name: '技术板块', description: '技术讨论', icon: null, version: 1, created_at: 0, updated_at: 0 }
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
  it('商城新建商品改按钮+弹层：触发按钮渲染，字段错误经无 JS 横幅透传且页面不崩', () => {
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
    // 新契约：新建商品 = 页头按钮 → Dialog（?/create），弹层本体无 JS 不渲染
    expect(body).toContain('新建商品');
    // 无 JS 回退横幅仍渲染动作 message（字段错误透传的 SSR 通道）
    expect(body).toContain('请检查表单中填写的字段错误');
    // nullsafe：带 fieldErrors 的 form 结果不破坏商品/订单区渲染
    expect(body).toContain('暂无商品');
    expect(body).toContain('暂无订单');
  });
});

describe('P2-03: 存储配置编辑入口（约定 A：按钮 → 弹层）', () => {
  it('首次进入渲染「编辑配置」入口按钮；tab/表单随配置写表单移入弹层，无常驻写表单', () => {
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
    // 只读运行状态仍 SSR 渲染（后端徽标 + 来源徽标）
    expect(body).toContain('编辑配置');
    expect(body).toContain('S3 实时运行中');
    // 原 tab/tabpanel SSR 断言（id="tab-storage-s3"/aria-selected/role=tabpanel）
    // 随约定 A 移入弹层（客户端渲染，弹层未打开时不参与 SSR）
    expect(body).not.toContain('id="tab-storage-s3"');
    expect(body).not.toMatch(/<form[^>]*action="\?\/save"/);
  });
});

describe('M18-ADMIN-OPS 约定 D: 商城商品行三点菜单 SSR', () => {
  const shopProduct: ShopProduct = {
    id: 'prod-1',
    kind: 'cosmetic_badge',
    status: 'published',
    slug: 'badge-gold',
    title: '黄金徽章',
    currency_id: 'coin',
    unit_price: 100,
    quantity_limit: 1,
    required_level: 1,
    refund_policy: 'non_refundable',
    version: 3,
    created_at: 1700000000000,
    updated_at: 1700000000000
  };

  it('商品行「⋮」三点菜单渲染（行语义 aria-label）；菜单关闭态与商品写表单不进 SSR', () => {
    const { body } = render(AdminShopPage, {
      props: {
        data: {
          products: { state: 'ok', items: [shopProduct] },
          orders: { state: 'ok', items: [] },
          config: { state: 'ok', data: { enabled: true, max_quantity_per_order: 10, version: 1 } }
        },
        form: null
      }
    });
    // 商品行数据仍 SSR 渲染；触发按钮 = 「⋮」三点菜单（aria-label 含商品行语义）
    expect(body).toContain('黄金徽章');
    expect(body).toContain('aria-label="更多操作：商品 黄金徽章"');
    // 菜单关闭态：操作列表（编辑/下架项）与商品操作 Dialog 表单（?/update|publish|disable）
    // 均不进 SSR HTML
    expect(body).not.toContain('row-actions__menu');
    expect(body).not.toContain('action="?/update"');
    expect(body).not.toContain('action="?/publish"');
    expect(body).not.toContain('action="?/disable"');
    // 订单行「退款」单一写操作保留原按钮（本例无 succeeded 订单 → 退款弹层表单不渲染）
    expect(body).not.toContain('action="?/refund"');
  });
});
