// M07-SHOP-STUDIO：装扮工作台已重构精简。
// 直接重定向到 /admin/shop，后台统一进行选品定价上架与昵称颜色快捷创建。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
  throw redirect(302, '/admin/shop');
};
