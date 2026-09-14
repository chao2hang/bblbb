#!/usr/bin/env python3
"""
BBLBB 社区数据库伪数据注入脚本（用于上线前全功能测试）

功能说明：
1. 将数据库中所有已有用户的密码哈希统一更新为针对 `zx123456..` 的 Argon2id 哈希。
2. 批量创建覆盖多角色、多层级、多状态的模拟测试用户：
   - 超级管理员 (admin)
   - 全局审核员/版主 (moderator)
   - 板块版主 (board_mod)
   - 资深技术大牛 (tech_expert)
   - 创意设计师 (creative_artist)
   - 资讯速递员 (news_reporter)
   - 活跃提问会员 (active_alice)
   - 休闲日常用户 (casual_bob)
   - 商城消费达人 (rich_buyer)
   - 新注册未验证用户 (newbie_unverified)
   - 被临时禁言用户 (muted_troublemaker)
   - 被永久封禁用户 (banned_violator)
3. 为各类测试用户注入全方位的业务伪数据：
   - 用户基础资料（昵称、个人简介、签名档、等级、信任级别）
   - 用户偏好与隐私设置（外观主题、隐私可见性、通知偏好）
   - 积分体系（经验值 exp、金币 coin 账户及充值/奖励交易流水）
   - 社区标签（rust, sveltekit, architecture, design, frontend, backend 等）
   - 全板块主题帖（技术深度长文、综合讨论、创意作品展示、求助问答、官方资讯）
   - 帖子完整富文本与格式（Markdown 标题、代码高亮、引用、列表、标签关联）
   - 多楼层评论与嵌套讨论回复（含楼层号、引用回复、二级回复）
   - 互动反应与收藏（帖子点赞、评论点赞、精华帖收藏）
   - 社交关注网络（多对多关注、双向互关）
   - 私信会话系统（多个会话双向长对话、未读标记）
   - 站内通知系统（系统通知、回复提醒、点赞提醒、@提及通知）
   - 个人草稿箱（未发布的讨论和文章草稿）
   - 社区成就系统（首发帖、被赞达人、百粉成就等解锁与进度）
   - 商城系统（商品购买记录、装扮资产、已装备头像框/昵称流光特效）
   - 审核与管理工单（举报处理单、审核案件、封禁/禁言处罚记录）
   - 全文检索数据（search_documents 与 search_fts 自动同步）
   - 安全验证与 MFA（高权限管理员配置预置 TOTP 密钥与应急恢复码，并签发有效 Session）
"""

import os
import sys
import time
import json
import base64
import hashlib
import sqlite3
import re
import html
from typing import Dict, List, Tuple

import argon2
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

# 统一测试密码
UNIFIED_PASSWORD = "zx123456.."

# 数据库文件路径
DB_PATH = os.environ.get("BBLBB_DATABASE_URL", "data/bblbb.sqlite")
if DB_PATH.startswith("sqlite://"):
    DB_PATH = DB_PATH[len("sqlite://"):]

# TOTP 加密密钥材料（从 backend/.env 提取或默认）
MFA_ENCRYPTION_KEY_HEX = "e85126ee9e2fb48fb9ba86791a48da0070ada646b2d26cee3ba5df78a3e7b718"
FIXED_TOTP_SECRET_B32 = "JBSWY3DPEHPK3PXP"  # 测试固定 Base32 密钥

# 固定应急恢复码
FIXED_RECOVERY_CODES = [
    "RECOVERY01-AAAA",
    "RECOVERY02-BBBB",
    "RECOVERY03-CCCC",
    "RECOVERY04-DDDD",
    "RECOVERY05-EEEE",
]


def uuidv7() -> str:
    """生成标准 RFC 9562 UUIDv7"""
    t = int(time.time() * 1000)
    rand_bytes = os.urandom(10)
    b0 = (t >> 40) & 0xFF
    b1 = (t >> 32) & 0xFF
    b2 = (t >> 24) & 0xFF
    b3 = (t >> 16) & 0xFF
    b4 = (t >> 8) & 0xFF
    b5 = t & 0xFF
    b6 = 0x70 | (rand_bytes[0] & 0x0F)
    b7 = rand_bytes[1]
    b8 = 0x80 | (rand_bytes[2] & 0x3F)
    b9 = rand_bytes[3]
    raw = bytes([b0, b1, b2, b3, b4, b5, b6, b7, b8, b9]) + rand_bytes[4:]
    h = raw.hex()
    return f"{h[:8]}-{h[8:12]}-{h[12:16]}-{h[16:20]}-{h[20:]}"


def base32_decode(s: str) -> bytes:
    alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
    s = s.upper().rstrip("=")
    buffer = 0
    bits = 0
    out = bytearray()
    for char in s:
        val = alphabet.find(char)
        if val < 0:
            continue
        buffer = (buffer << 5) | val
        bits += 5
        if bits >= 8:
            bits -= 8
            out.append((buffer >> bits) & 0xFF)
    return bytes(out)


def encrypt_totp_secret(secret_bytes: bytes, key_hex: str) -> str:
    """与 backend/src/auth/mfa.rs 对齐的 AES-256-GCM 加密"""
    key_material = key_hex.encode("utf-8")
    aes_key = hashlib.sha256(key_material).digest()
    aesgcm = AESGCM(aes_key)
    nonce = os.urandom(12)
    ciphertext = aesgcm.encrypt(nonce, secret_bytes, None)
    return (nonce + ciphertext).hex()


def hash_argon2(password: str) -> str:
    """与 backend/src/auth/password.rs 参数一致的 Argon2id 哈希"""
    ph = argon2.PasswordHasher(
        time_cost=2,
        memory_cost=19456,
        parallelism=1,
        type=argon2.Type.ID
    )
    return ph.hash(password)


def render_markdown_to_html(md: str) -> str:
    """将 Markdown 转换为排版规范的 HTML（避免占位截断与标题重复）"""
    lines = md.strip().split("\n")
    out = []
    in_code_block = False
    code_lines = []
    list_stack = []

    def inline_format(text: str) -> str:
        text = re.sub(r"`([^`]+)`", lambda m: f"<code>{html.escape(m.group(1))}</code>", text)
        text = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", text)
        text = re.sub(r"\*([^*]+)\*", r"<em>\1</em>", text)
        text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', text)
        return text

    def close_lists(target_depth=0):
        while len(list_stack) > target_depth:
            tag, _ = list_stack.pop()
            out.append(f"</li></{tag}>")

    for line in lines:
        stripped = line.strip()

        if stripped.startswith("```"):
            if in_code_block:
                out.append(f"<pre><code>{html.escape(chr(10).join(code_lines))}</code></pre>")
                code_lines = []
                in_code_block = False
            else:
                close_lists(0)
                in_code_block = True
            continue

        if in_code_block:
            code_lines.append(line)
            continue

        if not stripped:
            close_lists(0)
            continue

        if stripped in ("---", "***", "___"):
            close_lists(0)
            out.append("<hr />")
            continue

        m_h = re.match(r"^(#{1,6})\s+(.*)$", stripped)
        if m_h:
            close_lists(0)
            lvl = len(m_h.group(1))
            out.append(f"<h{lvl}>{inline_format(m_h.group(2))}</h{lvl}>")
            continue

        indent = len(line) - len(line.lstrip(" "))
        depth = 1 if indent < 2 else 2

        m_ul = re.match(r"^[-*+]\s+(.*)$", stripped)
        m_ol = re.match(r"^\d+\.\s+(.*)$", stripped)

        if m_ul or m_ol:
            tag = "ul" if m_ul else "ol"
            item_text = inline_format(m_ul.group(1) if m_ul else m_ol.group(1))
            if len(list_stack) < depth:
                out.append(f"<{tag}><li>{item_text}")
                list_stack.append((tag, depth))
            elif len(list_stack) > depth:
                close_lists(depth)
                out.append(f"</li><li>{item_text}")
            else:
                out.append(f"</li><li>{item_text}")
            continue

        close_lists(0)

        if stripped.startswith("> "):
            out.append(f"<blockquote><p>{inline_format(stripped[2:])}</p></blockquote>")
            continue

        out.append(f"<p>{inline_format(stripped)}</p>")

    if in_code_block:
        out.append(f"<pre><code>{html.escape(chr(10).join(code_lines))}</code></pre>")
    close_lists(0)
    return "\n".join(out)


def main():
    if not os.path.exists(DB_PATH):
        print(f"[-] 数据库文件未找到: {DB_PATH}")
        sys.exit(1)

    print(f"[*] 连接数据库: {DB_PATH}")
    con = sqlite3.connect(DB_PATH)
    con.row_factory = sqlite3.Row
    cur = con.cursor()

    now_ms = int(time.time() * 1000)
    day_ms = 86400 * 1000

    # 1. 生成统一密码哈希
    print(f"[*] 生成密码 '{UNIFIED_PASSWORD}' 的 Argon2id 哈希 (m=19456, t=2, p=1)...")
    unified_hash = hash_argon2(UNIFIED_PASSWORD)
    print(f"[+] 哈希完成: {unified_hash[:30]}...")

    # 2. 更新已有全部用户的密码
    cur.execute("SELECT count(*) FROM users")
    existing_count = cur.fetchone()[0]
    print(f"[*] 发现 {existing_count} 位已有用户，统一更新其密码哈希为 '{UNIFIED_PASSWORD}'...")
    cur.execute("UPDATE users SET password_hash = ?", (unified_hash,))
    print(f"[+] 已更新 {cur.rowcount} 个已有账号的密码")

    # 获取系统角色 ID
    cur.execute("SELECT id, name FROM roles")
    role_map = {row["name"]: row["id"] for row in cur.fetchall()}
    print(f"[*] 系统已有角色: {list(role_map.keys())}")

    # 获取板块 ID
    cur.execute("SELECT id, slug, name FROM boards")
    board_map = {row["slug"]: row["id"] for row in cur.fetchall()}
    print(f"[*] 系统已有板块: {list(board_map.keys())}")

    # 获取货币 ID
    cur.execute("SELECT id, code FROM currencies")
    currency_map = {row["code"]: row["id"] for row in cur.fetchall()}
    exp_cur_id = currency_map.get("exp")
    coin_cur_id = currency_map.get("coin")

    # 3. 准备要注入的新用户角色画像列表
    user_definitions = [
        {
            "username": "admin",
            "email": "admin@bblbb.test",
            "display_name": "系统超级管理员",
            "bio": "BBLBB 社区平台超级管理员，负责全站治理、功能上线前验收与运维支撑。",
            "signature": "守护社区底线，构建优质交流空间",
            "level": 10,
            "trust_level": 4,
            "status": "active",
            "email_verified": 1,
            "global_role": "administrator",
            "exp": 12000,
            "coin": 8888,
            "totp": True,
            "theme": "default",
        },
        {
            "username": "moderator",
            "email": "moderator@bblbb.test",
            "display_name": "全局合规审查员",
            "bio": "全站内容合规与社区风纪委员会成员。及时响应违规举报与争议处理。",
            "signature": "理性交流，友善发帖，违规必纠",
            "level": 8,
            "trust_level": 3,
            "status": "active",
            "email_verified": 1,
            "global_role": "global_moderator",
            "exp": 6500,
            "coin": 3200,
            "totp": True,
            "theme": "default",
        },
        {
            "username": "board_mod",
            "email": "board_mod@bblbb.test",
            "display_name": "技术版块专职版主",
            "bio": "负责『技术分享』与『综合讨论』板块的内容置顶、精华评选及日常维护。",
            "signature": "专注高质量代码与硬核技术交流",
            "level": 6,
            "trust_level": 3,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "board_roles": ["tech", "general"],
            "exp": 4200,
            "coin": 2100,
            "totp": True,
            "theme": "default",
        },
        {
            "username": "tech_expert",
            "email": "tech_expert@bblbb.test",
            "display_name": "老张聊架构",
            "bio": "10年+ 高并发与底层系统研发经验，热衷于 Rust, Axum, SvelteKit, SQLite 内核剖析。",
            "signature": "Make It Work, Make It Right, Make It Fast.",
            "level": 7,
            "trust_level": 3,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "exp": 5800,
            "coin": 2600,
            "totp": False,
            "theme": "cyberpunk",
            "deco": {
                "color": "prod-nick-fx-aurora",
                "frame": "prod-frame-gold",
                "title": "prod-title-night-owl",
                "effect": "prod-effect-sparkle"
            }
        },
        {
            "username": "creative_artist",
            "email": "artist@bblbb.test",
            "display_name": "星尘手绘小筑",
            "bio": "UI/UX 设计师、Blender 爱好者。分享社区装扮、主题图标与极光动效设计稿。",
            "signature": "用设计点亮代码世界的每一像素",
            "level": 5,
            "trust_level": 2,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "exp": 3100,
            "coin": 1500,
            "totp": False,
            "theme": "sunset",
            "deco": {
                "color": "prod-nick-fx-sunset",
                "frame": "prod-frame-gold",
                "effect": "prod-effect-sparkle"
            }
        },
        {
            "username": "news_reporter",
            "email": "news@bblbb.test",
            "display_name": "前沿资讯播报员",
            "bio": "聚合开源热点、Rust 生态、前端全栈演进与业界技术前沿资讯。",
            "signature": "每周二、四发布行业前沿技术雷达",
            "level": 4,
            "trust_level": 2,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "exp": 1800,
            "coin": 900,
            "totp": False,
            "theme": "default",
        },
        {
            "username": "active_alice",
            "email": "alice_test@bblbb.test",
            "display_name": "全栈萌新爱丽丝",
            "bio": "计算机大三学生，正在探索 SvelteKit + TailwindCSS 与 Rust Web 开发！求指导~",
            "signature": "每天写点代码，今天也要元气满满！",
            "level": 3,
            "trust_level": 2,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "exp": 850,
            "coin": 480,
            "totp": False,
            "theme": "default",
        },
        {
            "username": "casual_bob",
            "email": "bob_test@bblbb.test",
            "display_name": "下班就摸鱼的鲍勃",
            "bio": "键盘发烧友，综合讨论区水贴选手，热心解答新人和工位外设推荐。",
            "signature": "代码写得好，不如按时下班早",
            "level": 2,
            "trust_level": 1,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "exp": 360,
            "coin": 220,
            "totp": False,
            "theme": "default",
        },
        {
            "username": "rich_buyer",
            "email": "rich_buyer@bblbb.test",
            "display_name": "氪金狂魔王总",
            "bio": "支持社区创作者，收集全套商城昵称流光、头像框与主页特效。",
            "signature": "能用金币解决的问题，都不是问题",
            "level": 5,
            "trust_level": 2,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "exp": 9999,
            "coin": 50000,
            "totp": False,
            "theme": "dark",
            "deco": {
                "color": "prod-nick-fx-rainbow",
                "frame": "prod-frame-gold",
                "title": "prod-title-night-owl",
                "effect": "prod-effect-sparkle"
            }
        },
        {
            "username": "newbie_unverified",
            "email": "unverified@bblbb.test",
            "display_name": "刚注册未验证邮箱",
            "bio": "账号处于待激活状态，用于测试未验证邮箱拦截提示与功能准入门槛。",
            "signature": "还没有完成邮箱激活",
            "level": 1,
            "trust_level": 0,
            "status": "pending",
            "email_verified": 0,
            "global_role": None,
            "exp": 0,
            "coin": 0,
            "totp": False,
            "theme": "default",
        },
        {
            "username": "muted_troublemaker",
            "email": "muted@bblbb.test",
            "display_name": "禁言测试账号",
            "bio": "用于测试社区发帖、评论拦截与处罚提示的账号。",
            "signature": "当前被禁言中...",
            "level": 1,
            "trust_level": 0,
            "status": "active",
            "email_verified": 1,
            "global_role": None,
            "exp": 10,
            "coin": 5,
            "totp": False,
            "theme": "default",
            "sanction": "mute"
        },
        {
            "username": "banned_violator",
            "email": "banned@bblbb.test",
            "display_name": "封禁测试账号",
            "bio": "严重违反社区用户协议，处于永久封停状态。",
            "signature": "账号已被停用",
            "level": 1,
            "trust_level": 0,
            "status": "banned",
            "email_verified": 1,
            "global_role": None,
            "exp": 0,
            "coin": 0,
            "totp": False,
            "theme": "default",
            "sanction": "ban"
        }
    ]

    user_id_map: Dict[str, str] = {}
    session_tokens: Dict[str, str] = {}

    print(f"\n[*] 开始插入/更新测试用户（密码统一为 {UNIFIED_PASSWORD}）...")
    for u in user_definitions:
        username = u["username"]
        email = u["email"]
        
        # 检查是否已有
        cur.execute("SELECT id FROM users WHERE username_normalized = ?", (username.lower(),))
        row = cur.fetchone()
        if row:
            uid = row["id"]
            # 更新已有信息
            cur.execute("""
                UPDATE users SET
                    display_name = ?, bio = ?, signature = ?,
                    level = ?, trust_level = ?, status = ?,
                    email_verified = ?, email_verified_at = ?,
                    password_hash = ?, updated_at = ?
                WHERE id = ?
            """, (
                u["display_name"], u["bio"], u["signature"],
                u["level"], u["trust_level"], u["status"],
                u["email_verified"], now_ms - 10 * day_ms if u["email_verified"] else None,
                unified_hash, now_ms, uid
            ))
        else:
            uid = uuidv7()
            cur.execute("""
                INSERT INTO users (
                    id, username_normalized, email_normalized, password_hash,
                    status, email_verified, email_verified_at, display_name,
                    bio, signature, timezone, level, level_updated_at,
                    trust_level, trust_level_updated_at, version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?,
                    ?, ?, ?, ?,
                    ?, ?, 'Asia/Shanghai', ?, ?,
                    ?, ?, 1, ?, ?
                )
            """, (
                uid, username.lower(), email.lower(), unified_hash,
                u["status"], u["email_verified"], now_ms - 10 * day_ms if u["email_verified"] else None,
                u["display_name"], u["bio"], u["signature"],
                u["level"], now_ms - 5 * day_ms,
                u["trust_level"], now_ms - 5 * day_ms,
                now_ms - 30 * day_ms, now_ms
            ))
        user_id_map[username] = uid

        # 偏好设置 user_preferences
        cur.execute("""
            INSERT INTO user_preferences (user_id, timezone, locale, theme_name, notification_json, updated_at, reaction_notifications)
            VALUES (?, 'Asia/Shanghai', 'zh-CN', ?, '{"email": true, "browser": true}', ?, 1)
            ON CONFLICT(user_id) DO UPDATE SET
                timezone = 'Asia/Shanghai',
                locale = 'zh-CN',
                theme_name = excluded.theme_name,
                updated_at = excluded.updated_at
        """, (uid, u.get("theme", "default"), now_ms))

        # 隐私设置 user_privacy
        cur.execute("""
            INSERT INTO user_privacy (user_id, email_visible_to, profile_visible_to, updated_at)
            VALUES (?, 'registered', 'everyone', ?)
            ON CONFLICT(user_id) DO UPDATE SET updated_at = excluded.updated_at
        """, (uid, now_ms))

        # 积分账户 point_accounts (exp / coin)
        if exp_cur_id:
            cur.execute("""
                INSERT INTO point_accounts (user_id, currency_id, balance, frozen_balance, version, updated_at)
                VALUES (?, ?, ?, 0, 1, ?)
                ON CONFLICT(user_id, currency_id) DO UPDATE SET
                    balance = excluded.balance,
                    updated_at = excluded.updated_at
            """, (uid, exp_cur_id, u.get("exp", 0), now_ms))

        if coin_cur_id:
            cur.execute("""
                INSERT INTO point_accounts (user_id, currency_id, balance, frozen_balance, version, updated_at)
                VALUES (?, ?, ?, 0, 1, ?)
                ON CONFLICT(user_id, currency_id) DO UPDATE SET
                    balance = excluded.balance,
                    updated_at = excluded.updated_at
            """, (uid, coin_cur_id, u.get("coin", 0), now_ms))

        # 角色分配
        if u.get("global_role") and u["global_role"] in role_map:
            role_id = role_map[u["global_role"]]
            cur.execute("""
                INSERT OR IGNORE INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                VALUES (?, ?, NULL, ?, NULL)
            """, (uid, role_id, now_ms - 20 * day_ms))

        if u.get("board_roles"):
            for b_slug in u["board_roles"]:
                if b_slug in board_map and "board_moderator" in role_map:
                    cur.execute("""
                        INSERT OR IGNORE INTO board_role_assignments (id, board_id, user_id, role_id, granted_by, granted_at, expires_at)
                        VALUES (?, ?, ?, ?, NULL, ?, NULL)
                    """, (uuidv7(), board_map[b_slug], uid, role_map["board_moderator"], now_ms - 20 * day_ms))

        # TOTP 2FA 凭据（针对 elevated 角色）
        if u.get("totp"):
            totp_raw = base32_decode(FIXED_TOTP_SECRET_B32)
            enc_secret = encrypt_totp_secret(totp_raw, MFA_ENCRYPTION_KEY_HEX)
            # 插入或更新已确认的 TOTP 凭据
            cur.execute("SELECT id FROM totp_credentials WHERE user_id = ? AND revoked_at IS NULL", (uid,))
            existing_totp = cur.fetchone()
            if existing_totp:
                cur.execute("""
                    UPDATE totp_credentials SET encrypted_secret = ?, last_accepted_step = 0, confirmed_at = ?
                    WHERE id = ?
                """, (enc_secret, now_ms - 10 * day_ms, existing_totp["id"]))
            else:
                cur.execute("""
                    INSERT INTO totp_credentials (
                        id, user_id, encrypted_secret, last_accepted_step, created_at, confirmed_at, revoked_at
                    ) VALUES (?, ?, ?, 0, ?, ?, NULL)
                """, (uuidv7(), uid, enc_secret, now_ms - 10 * day_ms, now_ms - 10 * day_ms))
            
            # 插入用户专属固定应急恢复码（code_hash 具有全局 UNIQUE 约束）
            cur.execute("DELETE FROM mfa_recovery_codes WHERE user_id = ?", (uid,))
            prefix = username.upper().replace("_", "")
            for i in range(1, 6):
                code = f"{prefix}-RC{i:02d}-SAFE"
                code_hash = hashlib.sha256(code.encode()).hexdigest()
                cur.execute("""
                    INSERT INTO mfa_recovery_codes (id, user_id, code_hash, created_at, consumed_at)
                    VALUES (?, ?, ?, ?, NULL)
                """, (uuidv7(), uid, code_hash, now_ms - 10 * day_ms))

        # 为测试方便预先生成免登录长效 Session（30天有效，auth_verified_at=now 满足 step-up）
        session_id = uuidv7()
        session_token = base64.urlsafe_b64encode(os.urandom(24)).decode().rstrip("=")
        token_hash = hashlib.sha256(session_token.encode()).hexdigest()
        cur.execute("""
            INSERT INTO user_sessions (
                id, user_id, token_hash, csrf_secret_hash, user_agent,
                created_at, last_seen_at, idle_expires_at, absolute_expires_at,
                version, ip_prefix_hash, auth_verified_at
            ) VALUES (
                ?, ?, ?, ?, 'BBLBB-TestHarness/1.0',
                ?, ?, ?, ?,
                0, '127.0.0.1', ?
            )
        """, (
            session_id, uid, token_hash, token_hash,
            now_ms, now_ms, now_ms + 30 * day_ms, now_ms + 90 * day_ms,
            now_ms
        ))
        session_tokens[username] = session_token

        # 装扮装备（M07-SHOP-SCHEMA-06 语义）：user_presentations 槽位必须存
        # 「已装备权益」的 entitlement id（equip/rebuild_presentation 同构）。
        # 公开投影按权益有效性编译 Token——直接写商品 id/任意字符串会被服务端
        # 视为孤立 ID 丢弃（历史 seed 的『seed-frame-gold』即因此从不生效）。
        if u.get("deco"):
            deco = u["deco"]
            slot_products = [
                ("nickname_color_id", deco.get("color")),
                ("avatar_frame_id", deco.get("frame")),
                ("title_prefix_id", deco.get("title")),
                ("profile_effect_id", deco.get("effect")),
            ]
            slot_entitlements: dict = {}
            for slot_col, product_id in slot_products:
                if not product_id:
                    slot_entitlements[slot_col] = None
                    continue
                product = cur.execute(
                    "SELECT unit_price, currency_id FROM shop_products WHERE id = ? AND status = 'published'",
                    (product_id,),
                ).fetchone()
                if product is None:
                    print(f"  [!] 商品 {product_id} 不存在/未发布，跳过 {username} 的 {slot_col}")
                    slot_entitlements[slot_col] = None
                    continue
                order_id = uuidv7()
                ent_id = uuidv7()
                cur.execute("""
                    INSERT OR IGNORE INTO shop_orders (
                        id, user_id, product_id, product_version, quantity,
                        currency_id, unit_price, total_amount, point_operation_id,
                        status, idempotency_key, request_hash, created_at, updated_at
                    ) VALUES (
                        ?, ?, ?, 1, 1,
                        ?, ?, ?, ?,
                        'succeeded', ?, 'seed-mock-deco', ?, ?
                    )
                """, (
                    order_id, uid, product_id,
                    product["currency_id"], product["unit_price"], product["unit_price"],
                    uuidv7(), f"seed-deco-{uid}-{product_id}",
                    now_ms - 5 * day_ms, now_ms - 5 * day_ms
                ))
                cur.execute("""
                    INSERT INTO user_entitlements (
                        id, user_id, product_id, order_id, status,
                        quantity, remaining_quantity, valid_from, expires_at,
                        equipped_at, revoked_at, created_at, updated_at
                    ) VALUES (
                        ?, ?, ?, ?, 'equipped',
                        1, 1, ?, NULL,
                        ?, NULL, ?, ?
                    )
                """, (
                    ent_id, uid, product_id, order_id,
                    now_ms - 5 * day_ms, now_ms - 5 * day_ms,
                    now_ms - 5 * day_ms, now_ms - 5 * day_ms
                ))
                slot_entitlements[slot_col] = ent_id
            cur.execute("""
                INSERT INTO user_presentations (
                    user_id, nickname_color_id, avatar_frame_id, title_prefix_id,
                    profile_effect_id, version, updated_at, created_at
                ) VALUES (?, ?, ?, ?, ?, 1, ?, ?)
                ON CONFLICT(user_id) DO UPDATE SET
                    nickname_color_id = excluded.nickname_color_id,
                    avatar_frame_id = excluded.avatar_frame_id,
                    title_prefix_id = excluded.title_prefix_id,
                    profile_effect_id = excluded.profile_effect_id,
                    updated_at = excluded.updated_at
            """, (
                uid,
                slot_entitlements.get("nickname_color_id"),
                slot_entitlements.get("avatar_frame_id"),
                slot_entitlements.get("title_prefix_id"),
                slot_entitlements.get("profile_effect_id"),
                now_ms,
                now_ms - 5 * day_ms
            ))

        print(f"  [+] 用户 {username:18} | ID: {uid} | 状态: {u['status']:7} | 等级: Lv.{u['level']}")

    # 4. 设置处罚记录（muted 与 banned）
    admin_id = user_id_map["admin"]
    muted_id = user_id_map["muted_troublemaker"]
    banned_id = user_id_map["banned_violator"]

    cur.execute("DELETE FROM sanctions WHERE user_id IN (?, ?)", (muted_id, banned_id))
    # 7 天禁言
    cur.execute("""
        INSERT INTO sanctions (
            id, user_id, board_id, kind, status, reason,
            starts_at, ends_at, created_by, created_at
        ) VALUES (
            ?, ?, NULL, 'mute', 'active', '违反社区发言守则，频繁无意义灌水及情绪化发言',
            ?, ?, ?, ?
        )
    """, (uuidv7(), muted_id, now_ms - 3600000, now_ms + 7 * day_ms, admin_id, now_ms - 3600000))

    # 永久封禁
    cur.execute("""
        INSERT INTO sanctions (
            id, user_id, board_id, kind, status, reason,
            starts_at, ends_at, created_by, created_at
        ) VALUES (
            ?, ?, NULL, 'ban', 'active', '严重违规：发布恶意攻击与违禁广告内容，依法封禁',
            ?, NULL, ?, ?
        )
    """, (uuidv7(), banned_id, now_ms - 86400000, admin_id, now_ms - 86400000))
    print("[+] 已设置禁言与封禁处罚记录")

    # 5. 准备标签 Tags
    tag_data = [
        ("rust", "Rust 编程语言与生态系统实践"),
        ("sveltekit", "SvelteKit 全栈与现代化前端框架"),
        ("architecture", "高并发架构、系统设计与可扩展性"),
        ("database", "SQLite, PostgreSQL 与分布式存储优化"),
        ("design", "UI/UX 视觉设计、动效与三维建模"),
        ("frontend", "现代 Web 前端技术栈与性能调优"),
        ("backend", "高性能 Web 服务端与微服务开发"),
        ("tutorial", "进阶教程与手把手实战指南"),
        ("qa", "社区问答与难题求助"),
        ("news", "开源动态与前沿技术快讯"),
        ("announcement", "BBLBB 社区官方公告与更新日志"),
    ]
    tag_id_map: Dict[str, str] = {}
    for t_name, t_desc in tag_data:
        cur.execute("SELECT id FROM tags WHERE name = ?", (t_name,))
        row = cur.fetchone()
        if row:
            tid = row["id"]
        else:
            tid = uuidv7()
            cur.execute("""
                INSERT INTO tags (id, name, slug, description, usage_count, is_active, created_at, updated_at)
                VALUES (?, ?, ?, ?, 0, 1, ?, ?)
            """, (tid, t_name, t_name, t_desc, now_ms - 30 * day_ms, now_ms))
        tag_id_map[t_name] = tid
    print(f"[+] 标签准备完毕，共 {len(tag_id_map)} 个核心标签")

    # 6. 生成高质量丰富的主题贴（Posts）
    posts_data = [
        {
            "author": "admin",
            "board": "general",
            "post_type": "article",
            "pinned": 1,
            "title": "【社区公告】欢迎加入 BBLBB 现代化开发者社区！全站守则与功能速览",
            "summary": "欢迎来到 BBLBB！本文介绍了社区的初衷、核心板块划分、积分装扮机制与讨论守则。",
            "tags": ["announcement"],
            "markdown": """# 欢迎来到 BBLBB 现代化开发者社区

亲爱的开发者伙伴们：

经过持续打磨与迭代，**BBLBB 社区平台**迎来了全新版本！本平台旨在为开发者、设计师与创作者提供一个**纯粹、高效、高审美**的技术交流与作品展示空间。

---

## 核心功能速览

1. **五大板块**：
   - **综合讨论**：自由分享工作流、效率利器与日常闲聊。
   - **技术分享**：深度技术干货、Rust 与前端实战、架构设计。
   - **创意工坊**：UI/UX 设计稿、3D 渲染动效、独立项目 Showcase。
   - **求助问答**：遇到技术阻碍？在这里向社区同行提问与互助。
   - **资讯动态**：追踪开源前沿、官方版本发布与技术热点。
2. **安全与认证**：
   - 支持强大的 **TOTP 两步验证 (2FA)** 与应急恢复码保障账号资产安全。
   - 多层级 RBAC 权限与实时审计机制。
3. **经济与装扮系统**：
   - 积极参与讨论与发帖即可获得 **经验 (EXP)** 与 **金币 (COIN)**。
   - 商城提供个性化昵称流光、专属头像框与称号。

---

## 社区基本守则

- **友善交流**：请保持对同行者的尊重，鼓励建设性建议，杜绝人身攻击与恶意挑衅。
- **排版优美**：发布代码时请务必使用 Markdown 代码块语法标记语言。
- **严禁垃圾广告**：违规发布黑产引流或垃圾广告将直接面临永久封禁。

祝大家在 BBLBB 交流愉快，灵感迸发！
""",
            "views": 388,
            "replies": [
                ("casual_bob", "沙发！前排支持官方，界面和流畅度真的做得太赞了！", None),
                ("tech_expert", "恭喜新版上线，技术栈 Axum + SvelteKit 的手感非常优秀，期待后续的高并发表现！", None),
                ("creative_artist", "暗黑模式下的渐变设计非常舒适，期待大家在创意工坊多发作品！", None),
                ("active_alice", "萌新报到！希望能在社区认识更多一起写代码的小伙伴~", None),
            ]
        },
        {
            "author": "tech_expert",
            "board": "tech",
            "post_type": "article",
            "pinned": 0,
            "title": "深入剖析 Rust 异步并发模型与 Tokio 运行时调度原理",
            "summary": "详细拆解 Rust 零成本抽象下的 Future 状态机、Pin 投影以及 Tokio 多线程 Work-stealing 调度机制。",
            "tags": ["rust", "backend", "architecture"],
            "markdown": """## 1. 为什么 Rust 异步与众不同？

在大多数带有虚拟机的语言中（如 Go、Node.js），异步运行时是由 VM 深度绑定的，开发者通常无需关心协程切换细节。而在 Rust 中，标准库**仅提供 Future Trait**，没有内置运行时。

```rust
pub trait Future {
    type Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}
```

### 核心特性：
- **Pull-based 轮询模型**：Future 不会自动向前推进，只有当外部 `poll` 被调用时才执行一段逻辑。
- **状态机代码生成**：每个 `async fn` 都被编译器编译为一个匿名的 `enum` 状态机，无需额外的堆分配。
- **Pin 机制**：确保自引用结构在内存中不会发生意外位移。

---

## 2. Tokio 的 Work-stealing（工作窃取）调度器

Tokio 的多线程运行时（`rt-multi-thread`）采用经典的分布式任务队列架构：

```
[Main Thread]
      |
+-----+-----+-----+-----+
| W1  | W2  | W3  | W4  |  <-- 每个 Worker 拥有独立本地队列（环形缓冲区）
+-----+-----+-----+-----+
   ^           |
   +-- Steal --+           <-- 本地空闲时从相邻 Worker 队列尾部窃取一半任务
```

### 关键优化技巧：
1. **避免在异步上下文中阻塞**：任何可能耗时超过毫秒级的同步调用（如慢磁盘读写、CPU 密集型压缩），都必须使用 `tokio::task::spawn_blocking` 移入专用线程池。
2. **合理使用 `tokio::select!`**：警惕 Future cancel-safety（取消安全性）问题，避免重入状态丢失。
""",
            "views": 420,
            "replies": [
                ("active_alice", "老张老师讲得太透彻了！以前一直被 Pin 搞晕，终于搞懂自引用为什么必须 Pin 了！", None),
                ("casual_bob", "mark！异步 Rust 踩坑无数，尤其是 select! 取消丢失数据那次排查了一下午...", None),
                ("board_mod", "干货满满！已加入技术板块精选合集，感谢老张的高质量产出。", None),
            ]
        },
        {
            "author": "tech_expert",
            "board": "tech",
            "post_type": "article",
            "pinned": 0,
            "title": "SvelteKit 2 + Vite 现代化全栈实战：SSR 状态流转与前端体验极致优化",
            "summary": "探讨 Svelte 5 Runes 反应式语法、服务端加载器（PageServerLoad）及 CSRF 交互下的全流程状态流转最佳实践。",
            "tags": ["sveltekit", "frontend", "architecture"],
            "markdown": """## 现代化全栈：为什么我们选择 SvelteKit？

随着现代前端单页应用（SPA）复杂度越来越高，包体积与运行时开销成为了不可忽视的痛点。SvelteKit 凭借其无虚拟 DOM（Virtual DOM）设计和精准的编译器优化，为开发者带来了前所未有的流畅手感。

### 1. 服务端数据加载模型

在 `+page.server.ts` 中，我们通过类型安全的 `PageServerLoad` 契约直接拉取后端接口：

```typescript
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ fetch, cookies }) => {
  const res = await fetch('/api/v1/posts?limit=20');
  if (!res.ok) {
    return { posts: [] };
  }
  const data = await res.json();
  return { posts: data.items };
};
```

### 2. 双向流转与 CSRF 安全防护

由于 SvelteKit 在表单提交与动作处理器（Form Actions）中广泛使用流式增强（`use:enhance`），在生产级环境中必须严格把控 Cookie 与 CSRF 校验：
- **`__Host-` 前缀 Cookie**：锁定 Path=/ 且必须使用 Secure 传输，防止子域跨站覆盖。
- **确定性 CSRF Token**：与 Session ID 及 Secret 关联，防止重放与跨站伪造。

### 3. Svelte 5 Runes 拥抱细粒度响应

借助 `$state`、`$derived` 和 `$effect`，组件内部的状态依赖由编译器自动收集，代码更接近纯粹的 JavaScript 原生心智模型！
""",
            "views": 315,
            "replies": [
                ("creative_artist", "SvelteKit 配合 Tailwind 的开发速度确实爽飞，尤其在移动端首屏加载特别快！", None),
                ("active_alice", "提个问题：在流式渲染加载慢的外部 API 时，推荐用嵌套路由还是 Suspense 式的异步流？", None),
                ("tech_expert", "回复 @active_alice：通常建议首屏核心框架同步 SSR，非关键模块使用 Promise 结合 Svelte await 块流式传输到客户端渲染。", 2),
            ]
        },
        {
            "author": "creative_artist",
            "board": "creative",
            "post_type": "article",
            "pinned": 0,
            "title": "【设计分享】BBLBB 社区流光微光装扮与暗黑主题规范",
            "summary": "解密社区商城中『极光流光』、『鎏金之环』等头像挂件与昵称色彩的 CSS 渐变动效实现原理。",
            "tags": ["design", "frontend"],
            "markdown": """## 设计理念：克制且富有表现力的赛博暗色系

我们希望社区的视觉风格既不显得冷冰冰，又不会因过度装饰导致视觉疲劳。因此，整体 UI 采用了**深空石墨灰**作为基底，同时使用**柔和极光渐变**来强化活跃用户的互动反馈。

### 1. 极光流光文本动效实现

使用纯现代 CSS 实现高性能文字渐变移动，完全不占用 CPU 运算：

```css
.nickname-aurora {
  background: linear-gradient(
    135deg,
    #06b6d4 0%,
    #8b5cf6 50%,
    #ec4899 100%
  );
  background-size: 200% 200%;
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  animation: aurora-flow 4s ease infinite;
}

@keyframes aurora-flow {
  0% { background-position: 0% 50%; }
  50% { background-position: 100% 50%; }
  100% { background-position: 0% 50%; }
}
```

### 2. 头像框图层适配规范

所有的头像挂件（SVG/WebP）均严格基于 `128x128` 像素参考网格，外圈预留 `8px` 发光缓冲区，确保在不同尺寸头像上缩放均保持清晰抗锯齿。

大家可以在个人中心商城尝试装备这些特效，欢迎在评论区提出你的装扮创意想法！
""",
            "views": 280,
            "replies": [
                ("rich_buyer", "已经把极光流光和鎏金头像框全部买齐了，佩戴效果帅呆！希望能出更多专属定制徽章！", None),
                ("casual_bob", "学到了，background-clip: text 的渐变流动真的很丝滑！", None),
            ]
        },
        {
            "author": "active_alice",
            "board": "help",
            "post_type": "discussion",
            "pinned": 0,
            "title": "【求助】SvelteKit 中页面切换时的客户端 CSRF Token 刷新机制如何处理？",
            "summary": "在单页客户端导航时，后端的预认证 CSRF Token 会不会过期？前端应该如何优雅地重试与静默换票？",
            "tags": ["sveltekit", "frontend", "qa"],
            "markdown": """大家好！我在本地写前台交互测试时遇到了一个关于 CSRF 的疑问：

### 场景描述：
1. 用户在未登录状态下进入登录页，后端签发了一个 `__Host-bblbb_csrf` cookie 和对应 10 分钟有效期的 preauth token。
2. 用户在页面停留了 15 分钟（超过了 Token 有效期）然后再点击登录表单提交。
3. 此时后端返回了 `403 csrf_failed`。

### 遇到的问题：
在客户端使用 `fetch` 拦截器时，如何优雅地捕获这个 `csrf_failed` 并自动请求 `/api/v1/auth/csrf` 换取新 Token 后重试刚才的 POST 请求？

有大佬写过类似的重试中间件吗？求教最佳实践方案！
""",
            "views": 190,
            "replies": [
                ("tech_expert", "可以使用类似 Axios 拦截器或封装统一的 `authedFetch`。当状态码为 403 且 code === 'csrf_failed' 时，挂起当前队列，调用一次 GET /api/v1/auth/csrf，刷新内存中的 token 和 header，然后再重放原始请求。", None),
                ("active_alice", "谢谢张老师！我已经按照这个思路在 fetch wrapper 中加入了一个带互斥锁的 refreshPromise，测试完美重试成功！", 1),
                ("moderator", "好问题！这也是上线前必须覆盖到的安全边界测试场景之一。", None),
            ]
        },
        {
            "author": "news_reporter",
            "board": "news",
            "post_type": "article",
            "pinned": 0,
            "title": "【业界动态】Rust 2026 Edition 路线图与重要语言特性预览",
            "summary": "Rust 官方核心团队公布了最新的编译器提速计划、异步闭包（Async Closure）的稳定进展以及生命周期报错诊断的进一步提升。",
            "tags": ["rust", "news"],
            "markdown": """## Rust 官方团队公布 2026 年度技术路线

在最新的官方博客中，Rust 核心团队与各 WG 工作组梳理了近期重点推进的基础设施升级：

### 1. 编译器并行前端与增量缓存提速
- 默认启用更细粒度的并行化编译阶段，大型项目的 debug 增量构建耗时预计下降 **30%~45%**。
- 更好地支持分布式缓存（如 sccache）在宏展开阶段的命中率。

### 2. 异步闭包（Async Closures）趋于稳定
长时间以来，在闭包中返回 Future 一直需要复杂的 Trait 签名转换。全新的 `async || {}` 原生支持将彻底消除诸多类型推导痛点：

```rust
let process_items = async |item: &str| {
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    println!("Processed: {item}");
};
```

### 3. 类型系统报错可读性改进
编译器将能更精准地给出借用生命周期冲突时的“建议修复方案”（fixit diff），极大降低新手的学习曲线。
""",
            "views": 480,
            "replies": [
                ("casual_bob", "async closure 终于要稳了！写迭代器 map 异步再也不用手工包 Pin<Box<dyn Future>> 了！", None),
                ("tech_expert", "编译器提速是大家期盼已久的，尤其是大型微服务单测全量编译能省不少宝贵时间。", None),
            ]
        },
        {
            "author": "casual_bob",
            "board": "general",
            "post_type": "discussion",
            "pinned": 0,
            "title": "【闲聊讨论】2026 年你最推荐的开发工作流、终端神器与效率工具是什么？",
            "summary": "抛砖引玉分享一下自己的日常开发装备，大家都有什么相见恨晚的开发利器？",
            "tags": ["architecture"],
            "markdown": """又到了愉快的周五摸鱼讨论时间！

作为一名每天有 8 小时以上都在和终端打交道的开发者，一套顺手的工具链能大幅提升幸福感。先分享我的常驻清单：

- **终端仿真器**：Ghostty / WezTerm（GPU 加速，多 Tab 分屏丝滑）
- **Shell**：Fish + Starship 提示符（开箱即用的自动补全和 Git 状态）
- **命令行利器**：
  - `ripgrep (rg)`：搜索代码速度快如闪电
  - `eza`：现代版 ls，树状目录一目了然
  - `bat`：带语法高亮和 Git 修改提示的 cat
  - `zellij`：现代化的终端复用器，布局管理极度舒适

大家平时都在用什么私藏的生产力神器？求安利！
""",
            "views": 260,
            "replies": [
                ("active_alice", "安利 Lazygit！命令行下做 Git 交互式 rebase 和暂存部分代码段简直无敌好用！", None),
                ("creative_artist", "设计稿管理推荐 Raycast + Figma 快捷插件，效率起飞。", None),
                ("tech_expert", "推荐 `mold` 链接器，Linux 下链接超大 Rust 项目的耗时从 10 秒直接压缩到 1 秒内。", None),
            ]
        }
    ]

    print("\n[*] 开始注入主题帖、正文与多楼层评论...")
    post_id_map: Dict[str, str] = {}

    for p in posts_data:
        post_id = uuidv7()
        author_id = user_id_map[p["author"]]
        board_id = board_map[p["board"]]
        created_time = now_ms - (len(posts_data) - len(post_id_map)) * 12 * 3600 * 1000
        slug = f"{p['title'][:20].replace(' ', '-').lower()}-{post_id[:8]}"

        # 插入 post 主表
        cur.execute("""
            INSERT INTO posts (
                id, board_id, author_id, title, content, content_format,
                status, visibility, reply_count, view_count, last_reply_at,
                pinned, created_at, updated_at, post_type, slug, summary,
                published_at, version
            ) VALUES (
                ?, ?, ?, ?, '', 'markdown',
                'published', 'public', ?, ?, ?,
                ?, ?, ?, ?, ?, ?,
                ?, 1
            )
        """, (
            post_id, board_id, author_id, p["title"],
            len(p["replies"]), p["views"], created_time + 7200000 if p["replies"] else None,
            p["pinned"], created_time, created_time, p["post_type"], slug, p["summary"],
            created_time
        ))

        # 插入 post_contents 富文本表
        body_html = render_markdown_to_html(p["markdown"])
        cur.execute("""
            INSERT INTO post_contents (
                post_id, body_markdown, body_html, renderer_version, excerpt, updated_at
            ) VALUES (?, ?, ?, 'markdown-v2+ammonia-v2', ?, ?)
        """, (post_id, p["markdown"], body_html, p["summary"], created_time))

        # 关联标签 post_tags
        for tag_name in p.get("tags", []):
            if tag_name in tag_id_map:
                cur.execute("""
                    INSERT OR IGNORE INTO post_tags (post_id, tag_id, created_at)
                    VALUES (?, ?, ?)
                """, (post_id, tag_id_map[tag_name], created_time))
                cur.execute("UPDATE tags SET usage_count = usage_count + 1 WHERE id = ?", (tag_id_map[tag_name],))

        # 写入 search_documents 触发 FTS 索引
        cur.execute("""
            INSERT OR REPLACE INTO search_documents (
                doc_id, entity_type, title, body, excerpt, slug,
                author_id, tags_json, source_revision, policy_revision, indexed_at
            ) VALUES (?, 'post', ?, ?, ?, ?, ?, ?, 1, 1, ?)
        """, (
            post_id, p["title"], p["markdown"], p["summary"], slug,
            author_id, json.dumps(p.get("tags", [])), created_time
        ))

        post_id_map[p["title"]] = post_id

        # 插入评论
        floor = 1
        last_comment_id = None
        comment_ids = []
        for replier_name, reply_text, parent_idx in p["replies"]:
            comment_id = uuidv7()
            replier_id = user_id_map[replier_name]
            reply_time = created_time + floor * 1800000
            parent_id = comment_ids[parent_idx - 1] if parent_idx and parent_idx <= len(comment_ids) else None

            cur.execute("""
                INSERT INTO comments (
                    id, post_id, author_id, parent_id, content, content_format,
                    status, floor, created_at, updated_at, version
                ) VALUES (?, ?, ?, ?, ?, 'markdown', 'published', ?, ?, ?, 1)
            """, (comment_id, post_id, replier_id, parent_id, reply_text, floor, reply_time, reply_time))

            comment_ids.append(comment_id)
            last_comment_id = comment_id
            floor += 1

            # 随机给热门评论点赞
            if floor % 2 == 0:
                cur.execute("""
                    INSERT OR IGNORE INTO comment_reactions (comment_id, user_id, reaction, created_at)
                    VALUES (?, ?, 'like', ?)
                """, (comment_id, admin_id, reply_time + 60000))

        if last_comment_id:
            cur.execute("UPDATE posts SET last_reply_id = ?, last_reply_at = ? WHERE id = ?", (
                last_comment_id, created_time + (floor - 1) * 1800000, post_id
            ))

        print(f"  [+] 帖子成功发布: 《{p['title'][:25]}...》 (回复数: {len(p['replies'])})")

    # 7. 帖子点赞与收藏 (post_reactions & favorites)
    print("\n[*] 注入帖子点赞与收藏行为...")
    all_post_ids = list(post_id_map.values())
    active_user_ids = [user_id_map[u["username"]] for u in user_definitions if u["status"] == "active"]

    for i, pid in enumerate(all_post_ids):
        # 每个帖子有 2~5 个用户点赞
        for uid in active_user_ids[i % 3 : i % 3 + 4]:
            cur.execute("""
                INSERT OR IGNORE INTO post_reactions (post_id, user_id, reaction, created_at)
                VALUES (?, ?, 'like', ?)
            """, (pid, uid, now_ms - (i + 1) * 3600000))

        # 精华贴收藏
        if i < 4:
            cur.execute("""
                INSERT OR IGNORE INTO favorites (user_id, post_id, created_at)
                VALUES (?, ?, ?)
            """, (user_id_map["active_alice"], pid, now_ms - 7200000))
            cur.execute("""
                INSERT OR IGNORE INTO favorites (user_id, post_id, created_at)
                VALUES (?, ?, ?)
            """, (user_id_map["casual_bob"], pid, now_ms - 3600000))

    # 8. 社交关注关系 (user_follows)
    print("[*] 建立用户社交关注网络...")
    follow_relations = [
        ("active_alice", "tech_expert"),
        ("active_alice", "creative_artist"),
        ("active_alice", "admin"),
        ("casual_bob", "tech_expert"),
        ("casual_bob", "news_reporter"),
        ("casual_bob", "active_alice"),
        ("rich_buyer", "creative_artist"),
        ("rich_buyer", "tech_expert"),
        ("tech_expert", "creative_artist"),
        ("creative_artist", "tech_expert"),  # 互关
        ("board_mod", "tech_expert"),
        ("moderator", "admin"),
    ]
    for follower, followee in follow_relations:
        cur.execute("""
            INSERT OR IGNORE INTO user_follows (follower_id, followee_id, created_at)
            VALUES (?, ?, ?)
        """, (user_id_map[follower], user_id_map[followee], now_ms - 15 * day_ms))
    print(f"[+] 已建立 {len(follow_relations)} 条关注关系")

    # 9. 私信与双向对话 (conversations & messages)
    print("\n[*] 模拟用户私信对话...")
    dialogues = [
        (
            "active_alice", "tech_expert",
            [
                ("active_alice", "老张老师您好！打扰您了，在拜读您的 Tokio 调度文章时想请教一个 Pin 的问题。"),
                ("tech_expert", "你好 Alice！客气了，具体是哪个部分有疑问？"),
                ("active_alice", "就是自引用结构中，为什么单纯放在堆上的 Box 还需要调用 Pin::new_unchecked 才能保证安全呢？"),
                ("tech_expert", "因为普通的 Box 是允许通过 std::mem::swap 换出内部值的！换出的一瞬间内存地址变了，指针就悬垂了。Pin 保证了即便在堆上也不会被移动。"),
                ("active_alice", "原来如此！swap 这一层我之前一直没想到，太感谢老张老师了！"),
            ]
        ),
        (
            "rich_buyer", "creative_artist",
            [
                ("rich_buyer", "画师大大你好！商城里的极光流光特效太帅了，我想定制一套专属公会头像框，请问接商单吗？"),
                ("creative_artist", "王总你好！感谢支持，目前接小规模定制，可以把您的色系偏好和元素需求发我看看~"),
                ("rich_buyer", "太好了，想要黑金龙鳞风格的，预算充足！下周我把需求文档发你邮箱！"),
            ]
        ),
        (
            "moderator", "casual_bob",
            [
                ("moderator", "鲍勃你好，注意到你在讨论区分享的终端工具贴反响很热烈！"),
                ("casual_bob", "哇版主大大，我还以为我水贴要被警告了哈哈（擦汗）"),
                ("moderator", "不会不会，内容非常实用！只是提醒一下下次可以把常用标签打齐，方便新人检索，感谢优质分享！"),
                ("casual_bob", "收到收到，下次一定注意规范排版！"),
            ]
        )
    ]

    for user_a_name, user_b_name, msg_list in dialogues:
        conv_id = uuidv7()
        user_a_id = user_id_map[user_a_name]
        user_b_id = user_id_map[user_b_name]
        conv_created = now_ms - 2 * day_ms

        cur.execute("""
            INSERT INTO conversations (id, created_at, last_message_at)
            VALUES (?, ?, ?)
        """, (conv_id, conv_created, now_ms - 1800000))

        # 参与者
        cur.execute("""
            INSERT INTO conversation_participants (conversation_id, user_id, last_read_at)
            VALUES (?, ?, ?), (?, ?, ?)
        """, (conv_id, user_a_id, now_ms - 60000, conv_id, user_b_id, now_ms - 60000))

        # 消息条目
        for sender_name, body in msg_list:
            msg_id = uuidv7()
            sender_id = user_id_map[sender_name]
            cur.execute("""
                INSERT INTO messages (id, conversation_id, sender_id, body, created_at)
                VALUES (?, ?, ?, ?, ?)
            """, (msg_id, conv_id, sender_id, body, conv_created))
            conv_created += 300000

    print("[+] 私信会话生成完成")

    # 10. 站内通知系统 (notifications)
    print("\n[*] 注入丰富通知（系统、回复、点赞、@提醒）...")
    notifications_data = [
        # active_alice
        (
            "active_alice", "reply", "收到新的讨论回复",
            "老张聊架构 回复了你在《深入剖析 Rust 异步并发模型与 Tokio 运行时调度原理》中的评论。",
            "/posts/" + all_post_ids[1], 0, "activity"
        ),
        (
            "active_alice", "reaction", "获得新的点赞",
            "摸鱼高手鲍勃 赞同了你的评论。",
            "/posts/" + all_post_ids[1], 0, "activity"
        ),
        (
            "active_alice", "system", "欢迎加入 BBLBB 开发者社区！",
            "账号注册完成，新手任务与签到奖励已发放，祝你在社区探索愉快！",
            "/announcements", 1, "system"
        ),
        # tech_expert
        (
            "tech_expert", "mention", "有人在讨论中提及了你",
            "全栈萌新爱丽丝 在帖子《SvelteKit 2 + Vite 现代化全栈实战》中 @ 了你。",
            "/posts/" + all_post_ids[2], 0, "activity"
        ),
        (
            "tech_expert", "badge", "解锁新成就：被赞达人",
            "你在社区的内容已累计收获超过 100 次点赞与好评！",
            "/me", 0, "system"
        ),
        # casual_bob
        (
            "casual_bob", "system", "每日签到奖励",
            "连续签到 7 天打卡成功！获得 30 经验值与 15 金币奖励。",
            "/me", 1, "system"
        ),
        # admin
        (
            "admin", "moderation", "收到待处理违规举报",
            "用户 active_alice 提交了一条关于灌水评论的举报工单，请前往管理后台审查。",
            "/admin/reports", 0, "moderation"
        )
    ]

    for u_name, n_type, title, body, link, is_read, category in notifications_data:
        cur.execute("""
            INSERT INTO notifications (
                id, user_id, type, title, body, link,
                is_read, created_at, read_at, category
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            uuidv7(), user_id_map[u_name], n_type, title, body, link,
            is_read, now_ms - 3600000, now_ms - 1800000 if is_read else None, category
        ))
    print(f"[+] 已插入 {len(notifications_data)} 条多类别测试通知")

    # 11. 草稿箱测试数据 (drafts)
    print("\n[*] 注入草稿箱数据...")
    drafts_data = [
        (
            "tech_expert", "tech", "article",
            "【草稿】Rust 与 WebAssembly 零拷贝内存传递优化方案",
            "## 痛点分析\n\n传统的 wasm-bindgen 在宿主与 Wasm 线性内存之间往往存在额外的内存复制..."
        ),
        (
            "active_alice", "help", "discussion",
            "【草稿】关于 TailwindCSS v4 的全新主题变量配置问题",
            "更新到 Tailwind 4 之后，以前的 tailwind.config.js 变成了 CSS 原生 @theme 规则..."
        )
    ]
    for d_author, d_board, d_type, d_title, d_md in drafts_data:
        cur.execute("""
            INSERT INTO drafts (
                id, owner_id, board_id, post_type, title, markdown,
                created_at, updated_at, version
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1)
        """, (
            uuidv7(), user_id_map[d_author], board_map[d_board],
            d_type, d_title, d_md, now_ms - 86400000, now_ms - 3600000
        ))
    print("[+] 草稿箱数据注入完毕")

    # 12. 成就系统数据 (user_achievements)
    print("\n[*] 注入用户成就系统解锁数据...")
    achievements_mapping = [
        ("tech_expert", "first_post", 1, 1),
        ("tech_expert", "like_magnet", 100, 1),
        ("tech_expert", "hundred_followers", 100, 1),
        ("tech_expert", "streak_7", 7, 0),
        ("active_alice", "first_post", 1, 1),
        ("active_alice", "streak_7", 5, 0),
        ("creative_artist", "first_post", 1, 1),
        ("creative_artist", "like_magnet", 45, 0),
        ("rich_buyer", "streak_7", 7, 1),
    ]
    for u_name, ach_code, progress, equipped in achievements_mapping:
        cur.execute("SELECT id FROM achievements WHERE code = ?", (ach_code,))
        row = cur.fetchone()
        if row:
            cur.execute("""
                INSERT OR REPLACE INTO user_achievements (user_id, achievement_id, unlocked_at, progress, equipped)
                VALUES (?, ?, ?, ?, ?)
            """, (user_id_map[u_name], row["id"], now_ms - 5 * day_ms, progress, equipped))
    print("[+] 用户成就解锁注入完毕")

    # 13. 商城商品购买资产 (user_entitlements)
    print("\n[*] 注入商城订单与用户资产...")
    # 为 rich_buyer 分配多件装扮资产（owned；装备态由上方装扮装备块写入，
    # 避免同一商品出现两份 equipped 权益导致装备槽指向歧义）。
    rich_uid = user_id_map["rich_buyer"]
    items_to_own = [
        "prod-title-night-owl",
        "prod-nick-fx-rainbow",
        "prod-frame-gold",
        "prod-effect-sparkle"
    ]
    for prod_id in items_to_own:
        ent_id = uuidv7()
        order_id = uuidv7()
        cur.execute("""
            INSERT OR IGNORE INTO shop_orders (
                id, user_id, product_id, product_version, quantity,
                currency_id, unit_price, total_amount, point_operation_id,
                status, idempotency_key, request_hash, created_at, updated_at
            ) VALUES (
                ?, ?, ?, 1, 1,
                ?, 500, 500, ?,
                'succeeded', ?, 'dummy_hash', ?, ?
            )
        """, (
            order_id, rich_uid, prod_id, coin_cur_id,
            uuidv7(), uuidv7(), now_ms - 10 * day_ms, now_ms - 10 * day_ms
        ))

        cur.execute("""
            INSERT OR IGNORE INTO user_entitlements (
                id, user_id, product_id, order_id, status,
                quantity, remaining_quantity, valid_from, expires_at,
                equipped_at, revoked_at, created_at, updated_at
            ) VALUES (
                ?, ?, ?, ?, 'owned',
                1, 1, ?, NULL,
                NULL, NULL, ?, ?
            )
        """, (
            ent_id, rich_uid, prod_id, order_id,
            now_ms - 10 * day_ms,
            now_ms - 10 * day_ms, now_ms - 10 * day_ms
        ))

    # 14. 社区治理与工单 (reports & moderation_cases)
    print("\n[*] 注入内容审核案件与举报工单...")
    report_id = uuidv7()
    cur.execute("""
        INSERT OR IGNORE INTO reports (
            id, reporter_id, target_type, target_id, reason_code,
            details, status, report_dedup_key, dedup_until, assigned_to,
            created_at, updated_at
        ) VALUES (
            ?, ?, 'comment', ?, 'spam',
            '此评论涉及外部违规推销链接，请版主核实清理。',
            'triaged', ?, ?, ?, ?, ?
        )
    """, (
        report_id, user_id_map["active_alice"], all_post_ids[0],
        f"spam-report-{report_id[:8]}", now_ms + day_ms,
        user_id_map["moderator"], now_ms - 7200000, now_ms - 3600000
    ))

    case_id = uuidv7()
    cur.execute("""
        INSERT OR IGNORE INTO moderation_cases (
            id, title, status, priority, assigned_to, created_by,
            created_at, updated_at
        ) VALUES (
            ?, '【工单 #101】关于综合讨论区广告机器人灌水治理与批量处置',
            'investigating', 'normal', ?, ?, ?, ?
        )
    """, (
        case_id, user_id_map["moderator"], user_id_map["admin"],
        now_ms - 86400000, now_ms - 3600000
    ))

    # 提交事务
    con.commit()
    con.close()

    print("\n" + "=" * 80)
    print(" 数据库伪数据注入已全部顺利完成！上线前测试账号与凭据汇总如下：")
    print("=" * 80)
    print(f"统一密码: {UNIFIED_PASSWORD}\n")
    print(f"{'用户名':<18} | {'身份/角色':<18} | {'邮箱':<24} | {'两步验证 (2FA)'}")
    print("-" * 80)
    for u in user_definitions:
        uname = u["username"]
        role_desc = u["display_name"]
        email = u["email"]
        mfa_desc = "TOTP已启用 (见下方说明)" if u.get("totp") else "免两步验证 (直接登录)"
        print(f"{uname:<18} | {role_desc:<18} | {email:<24} | {mfa_desc}")

    print("-" * 80)
    print("\n【管理员 / 版主 (TOTP 两步验证) 测试说明】：")
    print(f" 1. 账号列表: admin, moderator, board_mod")
    print(f" 2. 密码: {UNIFIED_PASSWORD}")
    print(f" 3. 若前端提示输入 6 位两步验证动态码，可使用以下两种方式之一：")
    print(f"    - 方式 A (推荐)：直接使用应急恢复码登录，对应账号的任一恢复码：")
    print(f"      * admin:      ADMIN-RC01-SAFE 到 ADMIN-RC05-SAFE")
    print(f"      * moderator:  MODERATOR-RC01-SAFE 到 MODERATOR-RC05-SAFE")
    print(f"      * board_mod:  BOARDMOD-RC01-SAFE 到 BOARDMOD-RC05-SAFE")
    print(f"    - 方式 B：在身份验证器 App (Google Authenticator / 1Password) 添加 Base32 密钥：")
    print(f"      * 密钥: {FIXED_TOTP_SECRET_B32}")
    print(f" 4. 此外，脚本已为各管理员与普通账号在 `user_sessions` 签发免登录 Session Cookie，")
    print(f"    可随时通过注入 Cookie `__Host-bblbb_session` 实现免密直接进入！")
    print("=" * 80)


if __name__ == "__main__":
    main()
