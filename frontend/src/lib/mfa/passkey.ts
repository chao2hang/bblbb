// Passkey（WebAuthn）浏览器端工具（M02-MFA-PK）。
//
// 职责：把后端返回的 WebAuthn options JSON（base64url 编码字段）适配为
// `navigator.credentials` 所需的 BufferSource，并把浏览器返回的
// PublicKeyCredential 序列化为后端可反序列化的 JSON（webauthn-rs proto
// 字段名与浏览器原始 JSON 一致：id/rawId/response.clientDataJSON/...）。
//
// 安全说明：challenge 永远来自服务端 options（webauthn-rs state 绑定），
// 本模块不生成、不回传任何 challenge。

/** 后端未配置 Passkey / 浏览器不支持时的判定（secure context 内才有 API）。 */
export function passkeySupported(): boolean {
  return typeof window !== 'undefined' && window.isSecureContext === true && 'PublicKeyCredential' in window;
}

export function base64UrlEncode(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function base64UrlDecode(value: string): Uint8Array {
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/');
  const padding = normalized.length % 4 === 0 ? '' : '='.repeat(4 - (normalized.length % 4));
  const binary = atob(normalized + padding);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** 把 options JSON 中 base64url 字段解码为 BufferSource（递归已知字段）。 */
function decodeCredentialIds(
  list: Array<Record<string, unknown>> | undefined
): PublicKeyCredentialDescriptor[] {
  return (list ?? []).map((cred) => ({
    ...cred,
    id: base64UrlDecode(String(cred.id))
  })) as unknown as PublicKeyCredentialDescriptor[];
}

export interface PasskeyCreationOptionsJson {
  publicKey: {
    rp: Record<string, unknown>;
    user: Record<string, unknown>;
    challenge: string;
    pubKeyCredParams: Array<Record<string, unknown>>;
    timeout?: number;
    excludeCredentials?: Array<Record<string, unknown>>;
    authenticatorSelection?: Record<string, unknown>;
    attestation?: string;
  };
}

export interface PasskeyRequestOptionsJson {
  publicKey: {
    challenge: string;
    rpId?: string;
    timeout?: number;
    allowCredentials?: Array<Record<string, unknown>>;
    userVerification?: string;
  };
}

/** 执行 Passkey 注册（navigator.credentials.create）→ 返回后端 confirm 所需 JSON。 */
export async function registerPasskey(options: PasskeyCreationOptionsJson): Promise<unknown> {
  const pk = options.publicKey;
  const creationOptions = {
    rp: pk.rp,
    user: { ...pk.user, id: base64UrlDecode(String(pk.user.id)) },
    challenge: base64UrlDecode(pk.challenge),
    pubKeyCredParams: pk.pubKeyCredParams,
    ...(pk.timeout !== undefined ? { timeout: pk.timeout } : {}),
    excludeCredentials: decodeCredentialIds(pk.excludeCredentials),
    ...(pk.authenticatorSelection ? { authenticatorSelection: pk.authenticatorSelection } : {}),
    ...(pk.attestation ? { attestation: pk.attestation } : {})
  } as unknown as PublicKeyCredentialCreationOptions;
  const credential = (await navigator.credentials.create({
    publicKey: creationOptions
  })) as PublicKeyCredential | null;
  if (!credential) throw new Error('浏览器未返回凭据');

  const response = credential.response as AuthenticatorAttestationResponse;
  return {
    id: credential.id,
    rawId: base64UrlEncode(new Uint8Array(credential.rawId)),
    type: credential.type,
    ...(credential.authenticatorAttachment ? { authenticatorAttachment: credential.authenticatorAttachment } : {}),
    clientExtensionResults: credential.getClientExtensionResults?.() ?? {},
    response: {
      clientDataJSON: base64UrlEncode(new Uint8Array(response.clientDataJSON)),
      attestationObject: base64UrlEncode(new Uint8Array(response.attestationObject)),
      ...(typeof response.getTransports === 'function' ? { transports: response.getTransports() } : {})
    }
  };
}

/** 执行 Passkey 登录断言（navigator.credentials.get）→ 返回后端校验所需 JSON。 */
export async function assertPasskey(options: PasskeyRequestOptionsJson): Promise<unknown> {
  const pk = options.publicKey;
  const requestOptions = {
    challenge: base64UrlDecode(pk.challenge),
    ...(pk.rpId ? { rpId: pk.rpId } : {}),
    ...(pk.timeout !== undefined ? { timeout: pk.timeout } : {}),
    allowCredentials: decodeCredentialIds(pk.allowCredentials),
    ...(pk.userVerification ? { userVerification: pk.userVerification } : {})
  } as unknown as PublicKeyCredentialRequestOptions;
  const credential = (await navigator.credentials.get({
    publicKey: requestOptions
  })) as PublicKeyCredential | null;
  if (!credential) throw new Error('浏览器未返回凭据');

  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    id: credential.id,
    rawId: base64UrlEncode(new Uint8Array(credential.rawId)),
    type: credential.type,
    ...(credential.authenticatorAttachment ? { authenticatorAttachment: credential.authenticatorAttachment } : {}),
    clientExtensionResults: credential.getClientExtensionResults?.() ?? {},
    response: {
      authenticatorData: base64UrlEncode(new Uint8Array(response.authenticatorData)),
      clientDataJSON: base64UrlEncode(new Uint8Array(response.clientDataJSON)),
      signature: base64UrlEncode(new Uint8Array(response.signature)),
      ...(response.userHandle ? { userHandle: base64UrlEncode(new Uint8Array(response.userHandle)) } : {})
    }
  };
}

/** 浏览器/环境不可用时的统一错误文案（NotSupportedError 等）。 */
export function passkeyErrorMessage(e: unknown): string {
  if (e instanceof DOMException) {
    switch (e.name) {
      case 'NotAllowedError':
        return 'Passkey 验证已取消或超时，请重试';
      case 'InvalidStateError':
        return '该 Passkey 与本设备不匹配，请改用验证码登录';
      case 'NotSupportedError':
        return '当前浏览器不支持 Passkey，请改用验证码登录';
      case 'SecurityError':
        return '当前环境不支持 Passkey（需 HTTPS）';
      default:
        break;
    }
  }
  return e instanceof Error && e.message ? e.message : 'Passkey 验证失败，请改用验证码登录';
}
