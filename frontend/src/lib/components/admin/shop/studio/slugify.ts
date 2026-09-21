// M07-SHOP-STUDIO：商品 slug 工具，镜像 backend/src/shop/studio.rs 的
// slugify/valid_slug（前端只做预填与预检，最终派生与冲突解决在服务端事务内）。

/** 由商品标题派生 slug：ASCII 小写、非字母数字折叠为单个连字符、
 *  截断 48 字符、去首尾连字符；无法派生时回退 "cosmetic"。 */
export function slugify(title: string): string {
  let out = '';
  let pendingDash = false;
  for (const ch of title.toLowerCase()) {
    if (/[a-z0-9]/.test(ch)) {
      if (pendingDash && out.length > 0) out += '-';
      pendingDash = false;
      out += ch;
    } else {
      pendingDash = true;
    }
  }
  // 与后端一致：先完整构建，再截断 48 字符、去尾部连字符。
  return out.slice(0, 48).replace(/-+$/, '') || 'cosmetic';
}

/** 显式 slug 预检：与 OpenAPI pattern `^[a-z0-9]+(-[a-z0-9]+)*$` 一致，≤64。 */
export function validSlug(slug: string): boolean {
  return slug.length > 0 && slug.length <= 64 && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(slug);
}
