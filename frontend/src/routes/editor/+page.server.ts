// M04-UI-02：发帖编辑器是登录态操作面——匿名访问 303 跳登录
// （?next= 登录后回跳 /editor），不渲染编辑器骨架（发布/草稿自动保存/
// 定时发布等能力对匿名不可见）。真实鉴权仍由后端裁决（会话过期时
// 发布提交 401 → 页面错误处理路径不变）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { SESSION_COOKIE } from '$lib/api/server';

export const load: PageServerLoad = async ({ cookies }) => {
  if (!cookies.get(SESSION_COOKIE)) {
    throw redirect(303, `/login?next=${encodeURIComponent('/editor')}`);
  }
  return {};
};
