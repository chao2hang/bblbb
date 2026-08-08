// M13-UI-01：后台设置 概览页——真实权限门（role.manage）+ 导航链接。
import type { PageServerLoad } from './$types';
import { overviewLoad } from '../_overview';

export const load: PageServerLoad = ({ cookies, request }) =>
  overviewLoad(cookies, request.headers.get('x-request-id'), '后台设置', [
    { href: '/admin/activity', label: '活跃任务配置', desc: '签到/任务奖励（活动域）' },
    { href: '/admin/shop', label: '商城配置', desc: '商品/订单/退款' },
    { href: '/admin/users', label: '用户管理', desc: '用户状态与角色' }
  ]);
