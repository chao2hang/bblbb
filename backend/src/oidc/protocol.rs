//! OIDC 协议纯函数：PKCE S256、redirect/post-logout URI 校验与精确匹配、
//! scope 解析、pairwise subject、Discovery 文档、userinfo 投影、授权请求
//! 绑定（request hash）与 RS256 JWT 构造。
//!
//! 本模块不访问数据库、不读取环境变量（可与 `backend/src/domain/` 相同的
//! 纯约束标准）；密钥材料通过参数注入。

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{OidcError, SCOPE_SET};

// ─────────────────────────── base64url / 摘要 ───────────────────────────

fn b64url_engine() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

/// RFC 4648 §5 base64url 无填充编码。
pub fn base64url_encode(data: &[u8]) -> String {
    b64url_engine().encode(data)
}

/// base64url 无填充解码。
pub fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    b64url_engine().decode(s).ok()
}

/// RFC 4648 §4 标准 base64 解码（HTTP Basic 认证用）。
pub fn base64_decode_standard(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

/// SHA-256 hex。
pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

/// SHA-256 base64url（无填充）——PKCE S256 challenge 与 pairwise sub。
fn sha256_base64url(input: &[u8]) -> String {
    base64url_encode(&Sha256::digest(input))
}

/// 常量时间比较（防时序侧信道）。
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ─────────────────────────── PKCE S256 ───────────────────────────

/// 计算 PKCE S256 challenge（RFC 7636 §4.2）。
pub fn pkce_s256_challenge(verifier: &str) -> String {
    sha256_base64url(verifier.as_bytes())
}

/// 校验 code_verifier 是否匹配已存储的 S256 challenge。
pub fn verify_pkce(challenge: &str, verifier: &str) -> bool {
    constant_time_eq(challenge, &pkce_s256_challenge(verifier))
}

fn is_base64url_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// PKCE code_challenge 格式校验（43–128 个 base64url 字符，RFC 7636）。
pub fn is_valid_code_challenge(challenge: &str) -> bool {
    (super::CODE_CHALLENGE_MIN_LEN..=super::CODE_CHALLENGE_MAX_LEN).contains(&challenge.len())
        && challenge.bytes().all(is_base64url_char)
}

/// code_verifier 长度校验（RFC 7636：43–128；OAuth 2.1 建议 43–128）。
pub fn is_valid_code_verifier(verifier: &str) -> bool {
    is_valid_code_challenge(verifier)
}

// ─────────────────────────── scope ───────────────────────────

/// 解析并校验空格分隔的 scope 列表：必须是 `openid` 开头子集、
/// 不重复、无空段。
pub fn parse_scopes(raw: &str) -> Result<Vec<String>, OidcError> {
    let mut seen: Vec<String> = Vec::new();
    for part in raw.split(' ') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if !SCOPE_SET.split(' ').any(|allowed| allowed == part) {
            return Err(OidcError::InvalidRequest(format!(
                "unsupported scope '{part}'"
            )));
        }
        if seen.iter().any(|s| s == part) {
            return Err(OidcError::InvalidRequest(format!(
                "duplicate scope '{part}'"
            )));
        }
        seen.push(part.to_string());
    }
    if !seen.iter().any(|s| s == "openid") {
        return Err(OidcError::InvalidRequest(
            "the 'openid' scope is required".to_string(),
        ));
    }
    Ok(seen)
}

/// scope 子集判定（刷新时只能缩小，不能扩大）。
pub fn scope_is_subset(expanded: &[String], family_scopes: &[String]) -> bool {
    expanded.iter().all(|s| family_scopes.contains(s))
}

/// scope 列表 → 空格分隔字符串（保持顺序）。
pub fn join_scopes(scopes: &[String]) -> String {
    scopes.join(" ")
}

/// 从空格分隔字符串解析出 scope 集合（不校验，仅拆分）。
pub fn split_scopes(raw: &str) -> Vec<String> {
    raw.split(' ')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

// ─────────────────────────── redirect / post-logout URI ───────────────────────────

/// 是否为 loopback 主机（RFC 8252 原生应用例外）。
pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

/// 校验注册/使用的重定向 URI（OAuth/OIDC 注册规则）：
/// - scheme 必须 https；仅 loopback 主机允许 http（本地开发例外）；
/// - 禁止 fragment、userinfo、通配符；
/// - 不做前缀/通配匹配（匹配见 [`redirect_uri_matches`]）。
pub fn validate_redirect_uri(uri: &str) -> Result<(), &'static str> {
    let parsed = url::Url::parse(uri).map_err(|_| "invalid redirect URI")?;
    if parsed.fragment().is_some() {
        return Err("redirect URI must not contain a fragment");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("redirect URI must not contain userinfo");
    }
    match parsed.scheme() {
        "https" => {}
        "http" => {
            let host = parsed.host_str().unwrap_or("");
            if !is_loopback_host(host) {
                return Err("redirect URI must use https outside loopback");
            }
        }
        _ => return Err("redirect URI scheme must be https (loopback http allowed)"),
    }
    if parsed.host_str().is_none() {
        return Err("redirect URI must have a host");
    }
    if uri.contains('*') {
        return Err("redirect URI must not contain wildcards");
    }
    Ok(())
}

/// 精确匹配：预注册 URI 与请求 URI 必须逐字符一致（不做自定义规范化）。
pub fn redirect_uri_matches(registered: &str, presented: &str) -> bool {
    registered == presented
}

/// post-logout redirect URI 校验（与授权 redirect 同样的注册规则）。
pub fn validate_post_logout_uri(uri: &str) -> Result<(), &'static str> {
    validate_redirect_uri(uri)
}

// ─────────────────────────── pairwise subject ───────────────────────────

/// Pairwise Subject（OIDC Core §8.1）：对每个 Client 派生稳定、不可逆的
/// 标识，绝不输出内部 `users.id`。取 SHA-256 前 43 个 base64url 字符。
pub fn pairwise_subject(issuer: &str, user_id: &str, client_id: &str) -> String {
    let digest = sha256_base64url(format!("{issuer}|{user_id}|{client_id}").as_bytes());
    digest[..43.min(digest.len())].to_string()
}

// ─────────────────────────── 授权请求绑定 ───────────────────────────

/// 授权请求（authorize 端点验证通过后的规范化视图）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizeRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    /// 空格分隔的已验证 scope。
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
}

impl AuthorizeRequest {
    /// 规范化序列化（绑定与摘要的单一事实来源）。
    fn canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// 请求摘要：SHA-256 hex（64 字符）。
    pub fn request_hash(&self) -> String {
        sha256_hex(&self.canonical_json())
    }
}

/// 授权请求绑定（存入 `oauth_interactions.request_hash`）。
///
/// 格式 `{sha256_hex}.{AES-256-GCM hex 密文}`：
/// - 前半段是请求摘要（可独立校验 "request hash 绑定"）；
/// - 后半段用 OIDC 主密钥加密的规范化请求，使 interaction decision 时能
///   恢复 nonce/state/PKCE challenge（冻结 schema 无独立参数列）。
pub fn bind_request(req: &AuthorizeRequest, master_key: &[u8]) -> Result<String, OidcError> {
    if master_key.is_empty() {
        return Err(OidcError::ServerError(
            "OIDC key encryption key is not configured".to_string(),
        ));
    }
    let digest = req.request_hash();
    let blob = crate::auth::mfa::encrypt_secret(master_key, req.canonical_json().as_bytes());
    Ok(format!("{digest}.{blob}"))
}

/// 从绑定中恢复授权请求（decision 时使用）。
pub fn unbind_request(binding: &str, master_key: &[u8]) -> Result<AuthorizeRequest, OidcError> {
    let Some((digest, blob)) = binding.split_once('.') else {
        return Err(OidcError::ServerError(
            "malformed request binding".to_string(),
        ));
    };
    let plaintext = crate::auth::mfa::decrypt_secret(master_key, blob)
        .ok_or_else(|| OidcError::ServerError("cannot recover request binding".to_string()))?;
    let plaintext = String::from_utf8(plaintext)
        .map_err(|_| OidcError::ServerError("request binding is not utf-8".to_string()))?;
    let req: AuthorizeRequest = serde_json::from_str(&plaintext)
        .map_err(|_| OidcError::ServerError("request binding payload is invalid".to_string()))?;
    if req.request_hash() != digest {
        return Err(OidcError::ServerError(
            "request binding digest mismatch".to_string(),
        ));
    }
    Ok(req)
}

/// 提取绑定中的摘要部分（供外部比对/展示）。
pub fn binding_digest(binding: &str) -> String {
    binding.split('.').next().unwrap_or(binding).to_string()
}

// ─────────────────────────── Discovery ───────────────────────────

/// OIDC Discovery 文档（/.well-known/openid-configuration）。
///
/// 所有 URL 只从固定、验证过的 `PUBLIC_ORIGIN`（AppConfig.public_origin）派生。
pub fn discovery_document(issuer: &str) -> Value {
    json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/oauth/token"),
        "userinfo_endpoint": format!("{issuer}/oauth/userinfo"),
        "jwks_uri": format!("{issuer}/oauth/jwks.json"),
        "revocation_endpoint": format!("{issuer}/oauth/revoke"),
        "end_session_endpoint": format!("{issuer}/oauth/logout"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "subject_types_supported": ["pairwise"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": ["openid", "profile", "email"],
        "token_endpoint_auth_methods_supported": [
            "client_secret_basic",
            "client_secret_post",
            "none"
        ],
        "code_challenge_methods_supported": ["S256"],
        "claims_supported": [
            "sub", "iss", "aud", "exp", "iat", "auth_time", "nonce",
            "preferred_username", "name", "picture", "updated_at",
            "email", "email_verified"
        ],
        "revocation_endpoint_auth_methods_supported": [
            "client_secret_basic",
            "client_secret_post",
            "none"
        ],
    })
}

// ─────────────────────────── ID Token claims ───────────────────────────

/// ID Token claim 集合（OIDC Core §2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// 多 audience 时按规范提供 azp（v1 恒为单 audience，保留字段）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azp: Option<String>,
    /// JWT ID（防重放审计）。
    pub jti: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,
}

impl IdTokenClaims {
    /// 按 scope 投影身份 claim（profile/email；openid 只给 sub）。
    ///
    /// `picture` 只在有安全公开头像 URL 时输出；论坛不暴露私有附件签名 URL，
    /// 因此 v1 恒不输出 picture（docs/AUTH-OIDC.md §10）。
    pub fn with_user_projection(
        mut self,
        scopes: &[String],
        preferred_username: Option<String>,
        display_name: Option<String>,
        updated_at_secs: Option<i64>,
        email: Option<String>,
        email_verified: bool,
    ) -> Self {
        if scopes.iter().any(|s| s == "profile") {
            self.preferred_username = preferred_username;
            self.name = display_name;
            self.updated_at = updated_at_secs;
        }
        if scopes.iter().any(|s| s == "email") {
            self.email = email;
            self.email_verified = Some(email_verified);
        }
        self
    }
}

// ─────────────────────────── userinfo 投影 ───────────────────────────

/// `/oauth/userinfo` 响应（OIDC Core §5.3）：按 token scope 过滤 claim。
pub fn userinfo_claims(
    sub: &str,
    scopes: &[String],
    preferred_username: Option<&str>,
    display_name: Option<&str>,
    updated_at_secs: Option<i64>,
    email: Option<&str>,
    email_verified: bool,
) -> Value {
    let mut claims = serde_json::Map::new();
    claims.insert("sub".to_string(), Value::String(sub.to_string()));
    if scopes.iter().any(|s| s == "profile") {
        if let Some(name) = preferred_username {
            claims.insert(
                "preferred_username".to_string(),
                Value::String(name.to_string()),
            );
        }
        if let Some(name) = display_name {
            claims.insert("name".to_string(), Value::String(name.to_string()));
        }
        if let Some(at) = updated_at_secs {
            claims.insert("updated_at".to_string(), json!(at));
        }
    }
    if scopes.iter().any(|s| s == "email") {
        if let Some(email) = email {
            claims.insert("email".to_string(), Value::String(email.to_string()));
        }
        claims.insert("email_verified".to_string(), Value::Bool(email_verified));
    }
    Value::Object(claims)
}

// ─────────────────────────── RS256 JWT 构造 ───────────────────────────

/// 构造 JWS 签名输入（`header.payload`）。
pub fn jwt_signing_input(header: &Value, payload: &Value) -> Result<String, OidcError> {
    let header_enc = base64url_encode(
        &serde_json::to_vec(header)
            .map_err(|e| OidcError::ServerError(format!("cannot serialize JWT header: {e}")))?,
    );
    let payload_enc = base64url_encode(
        &serde_json::to_vec(payload)
            .map_err(|e| OidcError::ServerError(format!("cannot serialize JWT payload: {e}")))?,
    );
    Ok(format!("{header_enc}.{payload_enc}"))
}

/// RS256 签名（RSASSA-PKCS1-v1_5 + SHA-256，含 DigestInfo 前缀）。
pub fn rsa256_sign(
    signing_input: &str,
    priv_key: &rsa::RsaPrivateKey,
) -> Result<Vec<u8>, OidcError> {
    let digest = Sha256::digest(signing_input.as_bytes());
    priv_key
        .sign(rsa::Pkcs1v15Sign::new::<Sha256>(), &digest)
        .map_err(|e| OidcError::ServerError(format!("RS256 signing failed: {e}")))
}

/// RS256 签名校验（供 id_token_hint 验证）。
pub fn rsa256_verify(
    signing_input: &str,
    signature: &[u8],
    pub_key: &rsa::RsaPublicKey,
) -> Result<(), OidcError> {
    let digest = Sha256::digest(signing_input.as_bytes());
    pub_key
        .verify(rsa::Pkcs1v15Sign::new::<Sha256>(), &digest, signature)
        .map_err(|_| {
            OidcError::InvalidRequest("id token hint signature verification failed".into())
        })
}

/// 解析 JWT 三段结构。
pub fn split_jwt(token: &str) -> Result<(&str, &str, &str), OidcError> {
    let mut parts = token.split('.');
    let header = parts
        .next()
        .ok_or_else(|| OidcError::InvalidRequest("malformed JWT".into()))?;
    let payload = parts
        .next()
        .ok_or_else(|| OidcError::InvalidRequest("malformed JWT".into()))?;
    let signature = parts
        .next()
        .ok_or_else(|| OidcError::InvalidRequest("malformed JWT".into()))?;
    if parts.next().is_some() {
        return Err(OidcError::InvalidRequest("malformed JWT".into()));
    }
    Ok((header, payload, signature))
}

/// 解码 JWT payload JSON（不校验签名）。
pub fn decode_jwt_payload(token: &str) -> Result<Value, OidcError> {
    let (_, payload, _) = split_jwt(token)?;
    let bytes = base64url_decode(payload)
        .ok_or_else(|| OidcError::InvalidRequest("JWT payload is not base64url".into()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| OidcError::InvalidRequest("JWT payload is not JSON".into()))
}

/// 解码 JWT header JSON（不校验签名）。
pub fn decode_jwt_header(token: &str) -> Result<Value, OidcError> {
    let (header, _, _) = split_jwt(token)?;
    let bytes = base64url_decode(header)
        .ok_or_else(|| OidcError::InvalidRequest("JWT header is not base64url".into()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| OidcError::InvalidRequest("JWT header is not JSON".into()))
}

// ─────────────────────────── 授权请求参数解析 ───────────────────────────

/// 从原始查询串解析出参数对（保留重复项，供歧义检测）。
pub fn parse_params(raw: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(raw.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// 检测重复参数并返回参数表（重复即视为歧义拒绝，M11-PROTOCOL-04）。
pub fn params_map(raw: &str) -> Result<std::collections::HashMap<String, String>, OidcError> {
    let pairs = parse_params(raw);
    let mut map = std::collections::HashMap::with_capacity(pairs.len());
    for (k, v) in pairs {
        if map.contains_key(&k) {
            return Err(OidcError::InvalidRequest(format!(
                "parameter '{k}' was supplied more than once"
            )));
        }
        map.insert(k, v);
    }
    Ok(map)
}

// ─────────────────────────── 单元测试 ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_vectors() {
        // RFC 7636 附录 B 的测试向量
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(pkce_s256_challenge(verifier), expected);
        assert!(verify_pkce(expected, verifier));
        assert!(!verify_pkce(expected, "wrong-verifier"));
    }

    #[test]
    fn code_challenge_format_is_enforced() {
        let ok = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(is_valid_code_challenge(ok));
        assert!(!is_valid_code_challenge("short"));
        assert!(
            !is_valid_code_challenge(&"a".repeat(129)),
            "超长 challenge 必须拒绝"
        );
        assert!(!is_valid_code_challenge("not+base64url+chars!"));
    }

    #[test]
    fn scopes_require_openid_and_whitelist() {
        assert_eq!(
            parse_scopes("openid profile email").unwrap(),
            vec!["openid", "profile", "email"]
        );
        assert_eq!(parse_scopes("openid").unwrap(), vec!["openid"]);
        assert!(parse_scopes("profile").is_err(), "缺少 openid 必须拒绝");
        assert!(parse_scopes("openid money").is_err(), "未知 scope 必须拒绝");
        assert!(
            parse_scopes("openid openid").is_err(),
            "重复 scope 必须拒绝"
        );
        assert!(scope_is_subset(
            &["openid".into()],
            &["openid".into(), "email".into()]
        ));
        assert!(!scope_is_subset(&["email".into()], &["openid".into()]));
    }

    #[test]
    fn redirect_uri_validation_and_exact_match() {
        assert!(validate_redirect_uri("https://client.example/callback").is_ok());
        assert!(validate_redirect_uri("http://localhost:5173/callback").is_ok());
        assert!(validate_redirect_uri("http://127.0.0.1:5173/cb").is_ok());
        assert!(validate_redirect_uri("http://client.example/cb").is_err());
        assert!(validate_redirect_uri("https://client.example/cb#frag").is_err());
        assert!(validate_redirect_uri("ftp://client.example/cb").is_err());
        assert!(validate_redirect_uri("https://user@client.example/cb").is_err());
        assert!(redirect_uri_matches(
            "https://client.example/cb",
            "https://client.example/cb"
        ));
        assert!(!redirect_uri_matches(
            "https://client.example/cb",
            "https://client.example/cb/"
        ));
        assert!(!redirect_uri_matches(
            "https://client.example/cb",
            "https://client.example/cb?x=1"
        ));
    }

    #[test]
    fn pairwise_subject_is_stable_and_per_client() {
        let issuer = "https://bblbb.example";
        let sub_a = pairwise_subject(issuer, "user-1", "client-a");
        let sub_a2 = pairwise_subject(issuer, "user-1", "client-a");
        let sub_b = pairwise_subject(issuer, "user-1", "client-b");
        let sub_c = pairwise_subject(issuer, "user-2", "client-a");
        assert_eq!(sub_a, sub_a2);
        assert_ne!(sub_a, sub_b, "不同 client 必须不同 sub");
        assert_ne!(sub_a, sub_c, "不同用户必须不同 sub");
        assert!(!sub_a.contains("user-1"), "不得包含内部用户 ID");
        assert_eq!(sub_a.len(), 43);
    }

    #[test]
    fn request_binding_roundtrip_with_master_key() {
        let req = AuthorizeRequest {
            response_type: "code".into(),
            client_id: "client-1".into(),
            redirect_uri: "https://client.example/cb".into(),
            scope: "openid profile".into(),
            state: Some("state-abc".into()),
            nonce: Some("nonce-xyz".into()),
            code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into(),
            code_challenge_method: "S256".into(),
        };
        let key = b"master-key-material-32-bytes!";
        let binding = bind_request(&req, key).unwrap();
        assert!(binding.starts_with(&format!("{}.", req.request_hash())));
        let recovered = unbind_request(&binding, key).unwrap();
        assert_eq!(recovered, req);
        assert!(
            unbind_request(&binding, b"wrong-master-key").is_err(),
            "错误主密钥必须无法恢复"
        );
        let req2 = AuthorizeRequest {
            state: Some("tampered".into()),
            ..req.clone()
        };
        // 摘要与密文内容不一致（篡改 digest 前缀）→ 必须拒绝。
        let (_, blob) = binding.split_once('.').unwrap();
        let tampered = format!("{}.{}", req2.request_hash(), blob);
        assert!(
            unbind_request(&tampered, key).is_err(),
            "摘要不匹配必须拒绝"
        );
        assert!(bind_request(&req, b"").is_err(), "无主密钥必须失败");
    }

    #[test]
    fn discovery_derives_all_urls_from_issuer() {
        let doc = discovery_document("https://bblbb.example");
        assert_eq!(doc["issuer"], "https://bblbb.example");
        assert_eq!(
            doc["authorization_endpoint"],
            "https://bblbb.example/oauth/authorize"
        );
        assert_eq!(doc["jwks_uri"], "https://bblbb.example/oauth/jwks.json");
        assert_eq!(doc["code_challenge_methods_supported"][0], "S256");
        assert_eq!(doc["id_token_signing_alg_values_supported"][0], "RS256");
    }

    #[test]
    fn userinfo_projection_filters_by_scope() {
        let sub = "pairwise-sub";
        let openid_only = userinfo_claims(
            sub,
            &["openid".to_string()],
            Some("alice"),
            Some("Alice"),
            Some(1_700_000_000),
            Some("a@example.com"),
            true,
        );
        assert_eq!(openid_only, json!({ "sub": sub }));
        let full = userinfo_claims(
            sub,
            &[
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            Some("alice"),
            Some("Alice"),
            Some(1_700_000_000),
            Some("a@example.com"),
            true,
        );
        assert_eq!(full["preferred_username"], "alice");
        assert_eq!(full["email"], "a@example.com");
        assert_eq!(full["email_verified"], true);
        assert_eq!(full["updated_at"], 1_700_000_000);
        assert!(
            full.get("picture").is_none(),
            "无安全公开头像 URL 不输出 picture"
        );
    }

    #[test]
    fn duplicate_params_are_rejected() {
        assert!(params_map("a=1&a=2&b=3").is_err());
        assert!(params_map("a=1&b=2").is_ok());
    }
}
