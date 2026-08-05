// M00-FRONTEND-09：hydration/预取/客户端 store 的类型级隐私断言。
//
// 本文件不参与运行（非 *.test.ts），由 `npm run check`（svelte-check/tsc）
// 在编译期强制：客户端会话 store 与首页公开投影的类型可以携带本人邮箱，
// 但绝不能携带任何凭据、令牌、密钥或正文内容字段。
// 若未来有人给 DTO 追加 `password_hash`、`session_token` 等字段，
// 这里的 `Assert<false>` 会让 svelte-check 直接失败。
import type { User, Board, Tag, PostSummary, PublicProfile } from '$lib/api/types';

type Assert<T extends true> = T;

// ── 客户端会话 store（User = GET /me 投影）─────────────────────────────────
// 允许本人邮箱（登录页/设置页需要展示），但禁止凭据/令牌/密钥字段。

type _UserAllowsOwnEmail = Assert<'email' extends keyof User ? true : false>;
type _UserNoPassword = Assert<'password' extends keyof User ? false : true>;
type _UserNoPasswordHash = Assert<'password_hash' extends keyof User ? false : true>;
type _UserNoSessionToken = Assert<'session_token' extends keyof User ? false : true>;
type _UserNoTotpSecret = Assert<'totp_secret' extends keyof User ? false : true>;
type _UserNoResetToken = Assert<'reset_token' extends keyof User ? false : true>;
type _UserNoSecret = Assert<'secret' extends keyof User ? false : true>;
type _UserNoApiKey = Assert<'api_key' extends keyof User ? false : true>;

// ── 首页 hydration/预取数据源（load 输出的公开投影）────────────────────────
// 板块/标签/帖子行是公开数据：不得携带邮箱、凭据或正文内容（隐藏正文）。
// 断言在「具体实例化处」求值（NoPrivateKeys<Board> 解析为 true/false），
// 因此违反时错误会指向对应的 _*ProjectionClean 行。

type NoPrivateKeys<T> =
  'email' extends keyof T ? false :
  'password' extends keyof T ? false :
  'password_hash' extends keyof T ? false :
  'session_token' extends keyof T ? false :
  'secret' extends keyof T ? false :
  true;

type NoBodyContent<T> =
  'content' extends keyof T ? false :
  'body' extends keyof T ? false :
  'body_html' extends keyof T ? false :
  'hidden_body' extends keyof T ? false :
  true;

type NoStatus<T> = 'status' extends keyof T ? false : true;

type _BoardProjectionClean =
  Assert<NoPrivateKeys<Board>> & Assert<NoBodyContent<Board>>;
type _TagProjectionClean =
  Assert<NoPrivateKeys<Tag>> & Assert<NoBodyContent<Tag>>;
type _PostProjectionClean =
  Assert<NoPrivateKeys<PostSummary>> & Assert<NoBodyContent<PostSummary>>;

// ── 公开用户资料（GET /users/{username} 投影，M03-PROFILE-01）────────────────
// PublicProfile 是严格公开投影：不得携带邮箱、凭据、状态、版本或任何
// Session/内部字段。违反时 svelte-check 直接失败（M03-PROFILE-09）。

type _PublicProfileNoEmail =
  Assert<'email' extends keyof PublicProfile ? false : true>;
type _PublicProfileNoCredentials =
  Assert<
    'password' | 'password_hash' | 'session_token' | 'totp_secret' |
    'reset_token' | 'secret' | 'api_key' extends keyof PublicProfile ? false : true
  >;
type _PublicProfileNoStatus = Assert<NoStatus<PublicProfile>>;
type _PublicProfileNoVersion =
  Assert<'version' extends keyof PublicProfile ? false : true>;
type _PublicProfileProjectionClean =
  Assert<NoPrivateKeys<PublicProfile>> & Assert<NoStatus<PublicProfile>>;