// 通知页是纯个人数据页（列表/已读/偏好均为会话操作面）——匿名访问
// 303 跳登录（?next= 登录后回跳 /notifications），不渲染页面骨架。
// 原实现无服务端 load：匿名也渲染完整通知 UI，客户端 onMount 拉取
// 401 后才表现为空态。真实鉴权仍由后端裁决。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { SESSION_COOKIE } from '$lib/api/server';

export const load: PageServerLoad = async ({ cookies }) => {
  if (!cookies.get(SESSION_COOKIE)) {
    throw redirect(303, `/login?next=${encodeURIComponent('/notifications')}`);
  }
  return {};
};
