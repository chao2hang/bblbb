// M03-UI-02：资料编辑共享常量——服务端 action 与页面表单双端收紧。
//
// 上限与后端校验（M03-PROFILE-04，backend/src/users/profile.rs 文本规则）
// 保持一致；页面 maxlength 与 action clampText 共用同一来源。

/** 资料文本字段上限（字符数）。 */
export const PROFILE_TEXT_LIMITS = {
  display_name: 32,
  bio: 2000,
  signature: 200
} as const;

/** 截断到上限（表单输入与后端均不接受超限值）。 */
export function clampProfileText(value: string, limit: number): string {
  return value.length > limit ? value.slice(0, limit) : value;
}
