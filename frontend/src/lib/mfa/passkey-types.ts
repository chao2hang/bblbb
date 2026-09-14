// Passkey 投影类型（M02-MFA-PK）。
//
// 与后端 `PasskeyInfo`（OpenAPI `PasskeyCredentialInfo`）字段一一对应；
// 仅供服务端 load 与客户端组件共享，不包含任何浏览器 API 依赖。

export interface PasskeyInfo {
  /** 内部记录 ID（撤销接口使用；非 WebAuthn credential id） */
  id: string;
  name: string;
  aaguid: string | null;
  backup_eligible: boolean;
  backed_up: boolean;
  /** 注册时间（Unix 毫秒） */
  created_at: number;
  /** 最近一次断言成功时间（Unix 毫秒；尚未使用为 null） */
  last_used_at: number | null;
}

/** 后端 creation/request options 的最小形状（publicKey 载荷，base64url 字段）。 */
export interface PasskeyOptionsPayload {
  publicKey: Record<string, unknown>;
}
