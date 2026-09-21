#!/usr/bin/env python3
"""
BBLBB 第二轮深度多Agent仿真与并发边界回归测试套件
"""

import sys
import os
import time
import uuid
import json
import sqlite3
import hmac
import hashlib
import struct
import base64
import requests
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Dict, Any, Optional, List, Tuple

BASE_URL = os.environ.get("BBLBB_BASE_URL", "http://127.0.0.1:8080")
DB_PATH = os.environ.get("BBLBB_DATABASE_URL", "data/bblbb.sqlite")
if DB_PATH.startswith("sqlite://"):
    DB_PATH = DB_PATH[len("sqlite://"):]

UNIFIED_PASSWORD = "zx123456.."
FIXED_TOTP_SECRET_B32 = "JBSWY3DPEHPK3PXP"
COIN_CURRENCY_ID = "01911fd5-0047-0000-0000-000000000002"

def generate_totp(secret_b32: str = FIXED_TOTP_SECRET_B32) -> str:
    key = base64.b32decode(secret_b32, casefold=True)
    counter = int(time.time() // 30)
    msg = struct.pack(">Q", counter)
    h = hmac.new(key, msg, hashlib.sha1).digest()
    offset = h[-1] & 0x0F
    code = struct.unpack(">I", h[offset:offset+4])[0] & 0x7FFFFFFF
    return f"{code % 1000000:06d}"

class InsecureSession(requests.Session):
    def send(self, request, **kwargs):
        cookie_parts = [f"{c.name}={c.value}" for c in self.cookies]
        if cookie_parts:
            existing = request.headers.get("Cookie", "")
            if existing:
                all_cookies = dict(item.strip().split("=", 1) for item in existing.split(";") if "=" in item)
                for c in self.cookies:
                    all_cookies[c.name] = c.value
                request.headers["Cookie"] = "; ".join(f"{k}={v}" for k, v in all_cookies.items())
            else:
                request.headers["Cookie"] = "; ".join(cookie_parts)
        return super().send(request, **kwargs)

class AgentClient:
    def __init__(self, name: str, base_url: str = BASE_URL, ip: Optional[str] = None):
        self.name = name
        self.base_url = base_url.rstrip('/')
        self.session = InsecureSession()
        self.csrf_token: Optional[str] = None
        self.user_info: Optional[Dict[str, Any]] = None
        # 模拟不同独立用户的公网来源 IP（真实反向代理拓扑），避免全量落入 unknown 桶
        self.ip = ip or f"198.51.100.{abs(hash(name)) % 250 + 1}"

    def log(self, msg: str):
        print(f"[{self.name}] {msg}")

    def refresh_csrf(self) -> str:
        url = f"{self.base_url}/api/v1/auth/csrf"
        headers = {
            "User-Agent": f"BBLBB-Agent-{self.name}",
            "X-Forwarded-For": self.ip
        }
        res = self.session.get(url, headers=headers, timeout=5)
        ch_token = res.headers.get("x-bblbb-challenge")
        if res.status_code == 403 and ch_token:
            retry_headers = dict(headers)
            retry_headers["X-Bblbb-Challenge"] = ch_token
            res = self.session.get(url, headers=retry_headers, timeout=5)
        if res.status_code == 200:
            self.csrf_token = res.json().get("token")
            return self.csrf_token
        raise RuntimeError(f"Failed to fetch CSRF token: {res.status_code} {res.text}")

    def request(self, method: str, path: str, **kwargs) -> requests.Response:
        url = f"{self.base_url}{path}"
        headers = kwargs.pop("headers", {})
        headers.setdefault("X-Forwarded-For", self.ip)
        headers.setdefault("User-Agent", f"BBLBB-Agent-{self.name}")
        if method.upper() in ["POST", "PUT", "PATCH", "DELETE"]:
            if not self.csrf_token:
                self.refresh_csrf()
            if self.csrf_token:
                headers["X-CSRF-Token"] = self.csrf_token

        kwargs["headers"] = headers
        kwargs.setdefault("timeout", 10)
        res = self.session.request(method, url, **kwargs)

        # 检查 antibot challenge（一次性令牌，仅针对被挑战的请求重试）
        ch_token = res.headers.get("x-bblbb-challenge")
        if res.status_code == 403 and ch_token:
            retry_headers = dict(headers)
            retry_headers["X-Bblbb-Challenge"] = ch_token
            kwargs["headers"] = retry_headers
            res = self.session.request(method, url, **kwargs)

        # 检查 csrf 重试
        if res.status_code == 403 and "csrf" in res.text.lower():
            self.refresh_csrf()
            headers["X-CSRF-Token"] = self.csrf_token
            kwargs["headers"] = headers
            res = self.session.request(method, url, **kwargs)

        return res

    def login(self, identifier: str, password: str = UNIFIED_PASSWORD) -> bool:
        self.refresh_csrf()
        res = self.request("POST", "/api/v1/auth/login", json={
            "identifier": identifier,
            "password": password
        })
        if res.status_code == 200:
            data = res.json()
            if data.get("mfa_required"):
                challenge_token = data.get("challenge_token")
                totp_code = generate_totp()
                res_mfa = self.request("POST", "/api/v1/auth/login/mfa", json={
                    "challenge_token": challenge_token,
                    "totp_code": totp_code
                })
                if res_mfa.status_code == 200:
                    self.user_info = res_mfa.json()
                    self.refresh_csrf()
                    self.log(f"Login + MFA success as {identifier} (ID: {self.user_info.get('id')})")
                    return True
                else:
                    self.log(f"MFA step failed for {identifier}: {res_mfa.status_code} {res_mfa.text}")
                    return False
            else:
                self.user_info = data
                self.refresh_csrf()
                self.log(f"Login success as {identifier} (ID: {self.user_info.get('id')})")
                return True
        else:
            self.log(f"Login failed for {identifier}: {res.status_code} {res.text}")
            return False

class SimulationSuiteRound2:
    def __init__(self):
        self.issues_found: List[Dict[str, Any]] = []
        self.test_stats = {"passed": 0, "failed": 0, "total": 0}

    def record_issue(self, category: str, title: str, detail: str, severity: str = "HIGH"):
        issue = {
            "severity": severity,
            "category": category,
            "title": title,
            "detail": detail
        }
        self.issues_found.append(issue)
        print(f"\n🔥🔥🔥 [DEFECT FOUND] [{severity}] [{category}] {title}")
        print(f"    Detail: {detail}\n")

    def assert_true(self, condition: bool, test_name: str, failure_detail: str, category: str = "FUNCTIONAL", severity: str = "MEDIUM"):
        self.test_stats["total"] += 1
        if condition:
            self.test_stats["passed"] += 1
            print(f"  [PASS] {test_name}")
        else:
            self.test_stats["failed"] += 1
            print(f"  [FAIL] {test_name}")
            self.record_issue(category, test_name, failure_detail, severity)

    # -------------------------------------------------------------
    # 场景 1: T=08:00 新用户注册、枚举防护、资料更新与状态流转
    # -------------------------------------------------------------
    def run_scenario_newbie_auth(self):
        print("\n=======================================================")
        print("  SCENARIO 1: T=08:00 Newbie Auth, Privacy & Optimistic Lock")
        print("=======================================================")
        client = AgentClient("NewbieAgent")

        # 1.1 校验保留用户名注册拦截
        for reserved in ["admin", "root", "moderator", "administrator"]:
            res = client.request("POST", "/api/v1/auth/register", json={
                "username": reserved,
                "email": f"{reserved}@example.com",
                "password": UNIFIED_PASSWORD
            })
            self.assert_true(
                res.status_code in [400, 422],
                f"Reserved username rejection: {reserved}",
                f"Expected 400/422 but got {res.status_code}: {res.text}",
                category="AUTH_SECURITY"
            )

        # 1.2 弱密码拦截 (<8 chars)
        res_weak = client.request("POST", "/api/v1/auth/register", json={
            "username": f"user_{uuid.uuid4().hex[:8]}",
            "email": f"test_{uuid.uuid4().hex[:8]}@example.com",
            "password": "123"
        })
        self.assert_true(
            res_weak.status_code in [400, 422],
            "Weak password rejection (<8 chars)",
            f"Expected 400/422 but got {res_weak.status_code}: {res_weak.text}",
            category="AUTH_SECURITY"
        )

        # 1.3 正常合法用户注册
        test_uname = f"sim_user_{uuid.uuid4().hex[:6]}"
        test_email = f"{test_uname}@sim.example"
        res_reg = client.request("POST", "/api/v1/auth/register", json={
            "username": test_uname,
            "email": test_email,
            "password": UNIFIED_PASSWORD
        })
        self.assert_true(
            res_reg.status_code == 201,
            f"Valid user registration ({test_uname})",
            f"Expected 201 but got {res_reg.status_code}: {res_reg.text}",
            category="AUTH_FUNCTIONAL"
        )

        # 1.4 重复注册防枚举：接口返回 201 {"ok": true}，但数据库只存在 1 条记录
        res_dup = client.request("POST", "/api/v1/auth/register", json={
            "username": test_uname,
            "email": f"other_{test_email}",
            "password": UNIFIED_PASSWORD
        })
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT count(*) FROM users WHERE username_normalized = ?", (test_uname.lower(),))
        user_count = cur.fetchone()[0]
        conn.close()

        self.assert_true(
            res_dup.status_code in [201, 409, 422, 429] and user_count == 1,
            "User enumeration protection: return 201/429 while DB retains exactly 1 unique user record",
            f"Expected user count 1, got count={user_count}, status={res_dup.status_code}",
            category="AUTH_SECURITY"
        )

        # 1.5 激活用户邮箱并登录
        # 直接更新数据库激活邮箱以测试后续主流程
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("UPDATE users SET email_verified_at = ? WHERE username_normalized = ?", (int(time.time()*1000), test_uname.lower()))
        conn.commit()
        conn.close()

        login_ok = client.login(test_uname, UNIFIED_PASSWORD)
        self.assert_true(
            login_ok,
            f"Login with verified user ({test_uname})",
            "Login failed unexpectedly",
            category="AUTH_FUNCTIONAL"
        )

        # 1.6 更新个人资料带 If-Match 乐观锁守卫
        if login_ok:
            ver = str(client.user_info.get("version", 1))
            res_patch = client.request("PATCH", "/api/v1/me", json={
                "display_name": "新秀模拟者",
                "bio": "自动化第二轮多Agent测试账号",
                "signature": "Code is poetry"
            }, headers={"If-Match": ver})
            self.assert_true(
                res_patch.status_code == 200,
                "Update profile with If-Match (PATCH /api/v1/me)",
                f"Expected 200 but got {res_patch.status_code}: {res_patch.text}",
                category="USER_PROFILE"
            )

    # -------------------------------------------------------------
    # 场景 2: T=10:00 活跃用户行为：草稿箱、发帖、编辑、评论、收藏与交互
    # -------------------------------------------------------------
    def run_scenario_active_community(self):
        print("\n=======================================================")
        print("  SCENARIO 2: T=10:00 Active Creator & Interaction Flow")
        print("=======================================================")
        alice = AgentClient("ActiveAlice")
        logged_in = alice.login("active_alice", UNIFIED_PASSWORD)
        if not logged_in:
            self.record_issue("SETUP", "Alice Login Failure", "Cannot log in as active_alice")
            return

        # 2.1 每日签到
        res_checkin = alice.request("POST", "/api/v1/activity/visit", json={
            "path": "/me/balance",
            "manual": True
        })
        self.assert_true(
            res_checkin.status_code == 200,
            "Daily check-in (POST /api/v1/activity/visit)",
            f"Expected 200 but got {res_checkin.status_code}: {res_checkin.text}",
            category="ECONOMY"
        )

        # 2.2 信任等级与阅读时长
        res_trust = alice.request("GET", "/api/v1/me/trust-level")
        self.assert_true(
            res_trust.status_code == 200,
            "Get trust level (GET /api/v1/me/trust-level)",
            f"Expected 200 but got {res_trust.status_code}: {res_trust.text}",
            category="TRUST_LEVEL"
        )

        res_heartbeat = alice.request("POST", "/api/v1/me/trust-level/read-time", json={
            "seconds": 30
        })
        self.assert_true(
            res_heartbeat.status_code == 200,
            "Reading time heartbeat (POST /api/v1/me/trust-level/read-time)",
            f"Expected 200 but got {res_heartbeat.status_code}: {res_heartbeat.text}",
            category="TRUST_LEVEL"
        )

        # 2.3 获取板块列表
        res_boards = alice.request("GET", "/api/v1/boards")
        board_items = res_boards.json() if isinstance(res_boards.json(), list) else res_boards.json().get("items", [])
        target_board = board_items[0]["id"]

        # 2.4 草稿创建（补全 access_policy: "public"）
        draft_client_id = f"draft_{uuid.uuid4().hex[:16]}"
        res_draft = alice.request("POST", "/api/v1/drafts", json={
            "title": "爱丽丝的技术沉思草稿",
            "markdown": "# 这是一个草稿内容\n\n草稿详情...",
            "board_id": target_board,
            "type": "discussion",
            "access_policy": "public",
            "client_request_id": draft_client_id
        })
        self.assert_true(
            res_draft.status_code in [200, 201],
            "Create draft with access_policy (POST /api/v1/drafts)",
            f"Expected 200/201 but got {res_draft.status_code}: {res_draft.text}",
            category="CONTENT_DRAFTS"
        )

        # 2.5 正式发帖
        post_req_id = f"client_req_{uuid.uuid4().hex}"
        post_payload = {
            "title": f"爱丽丝的系统性能调优实验_{uuid.uuid4().hex[:4]}",
            "markdown": "## 探讨高并发高吞吐\n\n```rust\nfn benchmark() {}\n```",
            "board_id": target_board,
            "type": "discussion",
            "access_policy": "public",
            "client_request_id": post_req_id
        }
        res_post = alice.request("POST", "/api/v1/posts", json=post_payload)
        self.assert_true(
            res_post.status_code in [200, 201],
            "Create post (POST /api/v1/posts)",
            f"Expected 200/201 but got {res_post.status_code}: {res_post.text}",
            category="CONTENT_POSTS"
        )
        post_data = res_post.json()
        post_id = post_data.get("id")
        post_ver = str(post_data.get("version", 1))

        # 2.6 编辑帖子（带 If-Match 头）
        res_patch_post = alice.request("PATCH", f"/api/v1/posts/{post_id}", json={
            "title": f"爱丽丝更新后的实验总结_{uuid.uuid4().hex[:4]}",
            "markdown": "## 更新正文\n\n增加测试报告结论。"
        }, headers={"If-Match": post_ver})
        self.assert_true(
            res_patch_post.status_code == 200,
            f"Edit post with If-Match (PATCH /api/v1/posts/{post_id})",
            f"Expected 200 but got {res_patch_post.status_code}: {res_patch_post.text}",
            category="CONTENT_POSTS"
        )

        # 2.7 发表 1 楼评论
        comment_req_id = f"comment_req_{uuid.uuid4().hex}"
        res_comment = alice.request("POST", f"/api/v1/posts/{post_id}/comments", json={
            "markdown": "爱丽丝在 1 楼留下的补充总结！",
            "client_request_id": comment_req_id
        })
        self.assert_true(
            res_comment.status_code in [200, 201],
            "Create 1st floor comment",
            f"Expected 200/201 but got {res_comment.status_code}: {res_comment.text}",
            category="CONTENT_COMMENTS"
        )
        first_comment_id = res_comment.json().get("id") if res_comment.status_code in [200, 201] else None

        # 2.8 收藏与取消收藏帖子（正确端点：/api/v1/posts/{id}/favorite）
        res_fav = alice.request("POST", f"/api/v1/posts/{post_id}/favorite")
        self.assert_true(
            res_fav.status_code in [200, 201, 204],
            f"Favorite post (POST /api/v1/posts/{post_id}/favorite)",
            f"Expected 200/201/204 but got {res_fav.status_code}: {res_fav.text}",
            category="INTERACTION"
        )

        res_fav_list = alice.request("GET", "/api/v1/me/favorites")
        self.assert_true(
            res_fav_list.status_code == 200,
            "List my favorites (GET /api/v1/me/favorites)",
            f"Expected 200 but got {res_fav_list.status_code}: {res_fav_list.text}",
            category="INTERACTION"
        )

        res_unfav = alice.request("DELETE", f"/api/v1/posts/{post_id}/favorite")
        self.assert_true(
            res_unfav.status_code in [200, 204],
            f"Unfavorite post (DELETE /api/v1/posts/{post_id}/favorite)",
            f"Expected 200/204 but got {res_unfav.status_code}: {res_unfav.text}",
            category="INTERACTION"
        )

        return post_id, first_comment_id, target_board

    # -------------------------------------------------------------
    # 场景 3: T=14:00 跨用户互动：点赞他人帖子、关注、私信与撤回
    # -------------------------------------------------------------
    def run_scenario_cross_user_interaction(self, post_id: str):
        print("\n=======================================================")
        print("  SCENARIO 3: T=14:00 Cross-User Like, Follow & Direct Messaging")
        print("=======================================================")
        expert = AgentClient("TechExpert")
        if not expert.login("tech_expert", UNIFIED_PASSWORD):
            self.record_issue("SETUP", "TechExpert Login Failure", "Cannot log in as tech_expert")
            return

        # 3.1 点赞 Alice 的帖子（非自己帖子，应成功；验证 self-reaction 规则边界）
        res_like = expert.request("POST", f"/api/v1/posts/{post_id}/reactions", json={
            "reaction": "like"
        })
        self.assert_true(
            res_like.status_code in [200, 201, 204],
            "Cross-user like post (POST /api/v1/posts/{id}/reactions)",
            f"Expected 200/201/204 but got {res_like.status_code}: {res_like.text}",
            category="INTERACTION"
        )

        # 3.2 关注用户（正确端点：/api/v1/users/{username}/follow）
        res_follow = expert.request("POST", "/api/v1/users/active_alice/follow")
        self.assert_true(
            res_follow.status_code in [200, 201, 204],
            "Follow user active_alice (POST /api/v1/users/active_alice/follow)",
            f"Expected 200/201/204 but got {res_follow.status_code}: {res_follow.text}",
            category="SOCIAL"
        )

        # 3.3 私信会话创建（username: "active_alice" + Idempotency-Key 契约头）
        conv_client_id = f"conv_{uuid.uuid4().hex}"
        res_conv = expert.request("POST", "/api/v1/conversations", json={
            "username": "active_alice",
            "client_request_id": conv_client_id
        }, headers={"Idempotency-Key": conv_client_id})
        self.assert_true(
            res_conv.status_code in [200, 201],
            "Initiate conversation with username active_alice",
            f"Expected 200/201 but got {res_conv.status_code}: {res_conv.text}",
            category="MESSAGING"
        )
        conv_id = res_conv.json().get("id") if res_conv.status_code in [200, 201] else None

        # 3.4 发送私信（body: "..." + Idempotency-Key 契约头）
        if conv_id:
            msg_req_id = f"msg_{uuid.uuid4().hex}"
            res_msg = expert.request("POST", f"/api/v1/conversations/{conv_id}/messages", json={
                "body": "爱丽丝你好！第二轮自动化模拟联调正常！",
                "client_request_id": msg_req_id
            }, headers={"Idempotency-Key": msg_req_id})
            self.assert_true(
                res_msg.status_code in [200, 201],
                "Send direct message (POST /api/v1/conversations/{id}/messages)",
                f"Expected 200/201 but got {res_msg.status_code}: {res_msg.text}",
                category="MESSAGING"
            )
            msg_id = res_msg.json().get("id") if res_msg.status_code in [200, 201] else None

            # 3.5 撤回私信（2分钟内撤回）
            if msg_id:
                res_recall = expert.request("POST", f"/api/v1/conversations/{conv_id}/messages/{msg_id}/recall")
                self.assert_true(
                    res_recall.status_code in [200, 204],
                    "Recall message within 2 minutes",
                    f"Expected 200/204 but got {res_recall.status_code}: {res_recall.text}",
                    category="MESSAGING"
                )

    # -------------------------------------------------------------
    # 场景 4: T=16:00 商城消费、样式库与装扮佩戴
    # -------------------------------------------------------------
    def run_scenario_shop_and_wardrobe(self):
        print("\n=======================================================")
        print("  SCENARIO 4: T=16:00 Shop Cosmetics, Buying & Wardrobe")
        print("=======================================================")
        buyer = AgentClient("RichBuyer")
        if not buyer.login("rich_buyer", UNIFIED_PASSWORD):
            self.record_issue("SETUP", "RichBuyer Login Failure", "Cannot log in as rich_buyer")
            return

        # 4.1 检查装扮定义库（修复后新服务应返回 200）
        res_cosmetics = buyer.request("GET", "/api/v1/shop/cosmetics")
        self.assert_true(
            res_cosmetics.status_code == 200,
            "List public cosmetic definitions (GET /api/v1/shop/cosmetics)",
            f"Expected 200 but got {res_cosmetics.status_code}: {res_cosmetics.text}",
            category="SHOP_COSMETICS"
        )

        # 4.2 获取商品列表
        res_prod = buyer.request("GET", "/api/v1/shop/products")
        prod_items = res_prod.json() if isinstance(res_prod.json(), list) else res_prod.json().get("items", [])

        if prod_items:
            target_prod = prod_items[0]
            prod_id = target_prod["id"]

            # 4.3 购买商品
            order_key = f"order_{uuid.uuid4().hex}"
            res_order = buyer.request("POST", "/api/v1/shop/orders", json={
                "product_id": prod_id,
                "quantity": 1,
                "idempotency_key": order_key
            })
            self.assert_true(
                res_order.status_code in [200, 201],
                f"Purchase shop product ({target_prod.get('name')})",
                f"Expected 200/201 but got {res_order.status_code}: {res_order.text}",
                category="SHOP"
            )

            # 4.4 检查用户权益
            res_ent = buyer.request("GET", "/api/v1/me/entitlements")
            ent_items = res_ent.json() if isinstance(res_ent.json(), list) else res_ent.json().get("items", [])

            # 4.5 装备与卸下装扮
            if ent_items:
                equip_ent = ent_items[0]
                ent_id = equip_ent["id"]

                res_equip = buyer.request("POST", f"/api/v1/me/entitlements/{ent_id}/equip")
                self.assert_true(
                    res_equip.status_code in [200, 204],
                    f"Equip cosmetic entitlement ({ent_id})",
                    f"Expected 200/204 but got {res_equip.status_code}: {res_equip.text}",
                    category="WARDROBE"
                )

                # 检查个性化展示状态
                res_pres = buyer.request("GET", "/api/v1/me/presentation")
                self.assert_true(
                    res_pres.status_code == 200,
                    "Get presentation (GET /api/v1/me/presentation)",
                    f"Expected 200 but got {res_pres.status_code}: {res_pres.text}",
                    category="WARDROBE"
                )

                # 卸下装扮
                res_unequip = buyer.request("POST", f"/api/v1/me/entitlements/{ent_id}/unequip")
                self.assert_true(
                    res_unequip.status_code in [200, 204],
                    f"Unequip cosmetic entitlement ({ent_id})",
                    f"Expected 200/204 but got {res_unequip.status_code}: {res_unequip.text}",
                    category="WARDROBE"
                )

    # -------------------------------------------------------------
    # 场景 5: T=18:00 恶意攻击与越权检测（检验第一轮修复效果）
    # -------------------------------------------------------------
    def run_scenario_malicious_attacker(self, target_post_id: str, target_comment_id: str):
        print("\n=======================================================")
        print("  SCENARIO 5: T=18:00 Malicious Attacker & Authorization Boundaries")
        print("=======================================================")
        attacker = AgentClient("AttackerBob")
        attacker.login("casual_bob", UNIFIED_PASSWORD)

        # 5.1 尝试越权访问管理员接口
        for admin_path in [
            "/api/v1/admin/settings",
            "/api/v1/admin/bi/metrics",
            "/api/v1/admin/audit-logs",
            "/api/v1/admin/shop/products"
        ]:
            res = attacker.request("GET", admin_path)
            self.assert_true(
                res.status_code in [401, 403],
                f"Unauthorized admin access blocked: {admin_path}",
                f"Expected 401/403 for non-admin but got {res.status_code}",
                category="AUTHZ_SECURITY",
                severity="HIGH"
            )

        # 5.2 越权编辑他人帖子（验证修复：无论是否带 If-Match，均必须立即返回 403 Forbidden）
        res_tamper_no_ifmatch = attacker.request("PATCH", f"/api/v1/posts/{target_post_id}", json={
            "title": "Hacked Title Without If-Match",
            "markdown": "Pwned!"
        })
        self.assert_true(
            res_tamper_no_ifmatch.status_code == 403,
            "Unauthorized post edit without If-Match returns 403 Forbidden (Ownership checked first)",
            f"Expected 403 but got {res_tamper_no_ifmatch.status_code}: {res_tamper_no_ifmatch.text}",
            category="AUTHZ_SECURITY",
            severity="CRITICAL"
        )

        res_tamper_with_ifmatch = attacker.request("PATCH", f"/api/v1/posts/{target_post_id}", json={
            "title": "Hacked Title With If-Match",
            "markdown": "Pwned!"
        }, headers={"If-Match": "1"})
        self.assert_true(
            res_tamper_with_ifmatch.status_code == 403,
            "Unauthorized post edit with If-Match returns 403 Forbidden",
            f"Expected 403 but got {res_tamper_with_ifmatch.status_code}: {res_tamper_with_ifmatch.text}",
            category="AUTHZ_SECURITY",
            severity="CRITICAL"
        )

        # 5.3 越权删除他人评论（必须返回 403 Forbidden）
        if target_comment_id:
            res_del_comm = attacker.request("DELETE", f"/api/v1/comments/{target_comment_id}")
            self.assert_true(
                res_del_comm.status_code == 403,
                "Unauthorized delete of another user's comment returns 403 Forbidden",
                f"Expected 403 but got {res_del_comm.status_code}: {res_del_comm.text}",
                category="AUTHZ_SECURITY",
                severity="CRITICAL"
            )

        # 5.4 XSS 注入清洗验证
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT id FROM boards WHERE deleted_at IS NULL LIMIT 1")
        board_row = cur.fetchone()
        conn.close()
        if board_row:
            xss_payload = "<script>alert('XSS-BBLBB')</script><img src=x onerror=alert('PWNED')><a href=\"javascript:alert(1)\">click</a>"
            res_xss = attacker.request("POST", "/api/v1/posts", json={
                "title": f"XSS安全注入测试_{uuid.uuid4().hex[:4]}",
                "markdown": f"测试内容：\n\n{xss_payload}",
                "board_id": board_row[0],
                "type": "discussion",
                "access_policy": "public",
                "client_request_id": f"xss_req_{uuid.uuid4().hex}"
            })
            if res_xss.status_code in [200, 201]:
                body_html = res_xss.json().get("body_html", "")
                has_raw_script = "<script>" in body_html.lower() or "javascript:" in body_html.lower() or "onerror=" in body_html.lower()
                self.assert_true(
                    not has_raw_script,
                    "Ammonia Markdown sanitizer strips XSS payloads",
                    f"XSS payload was NOT properly sanitized: {body_html}",
                    category="INJECTION_SECURITY",
                    severity="CRITICAL"
                )

    # -------------------------------------------------------------
    # 场景 6: T=20:00 板块版主与全局版主权限作用域验证
    # -------------------------------------------------------------
    def run_scenario_moderation_scopes(self, post_id: str, post_board_id: str):
        print("\n=======================================================")
        print("  SCENARIO 6: T=20:00 Moderator Scopes & Appeals Flow")
        print("=======================================================")

        # 6.1 获取板块版主 board_mod 被授权的板块
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("""
            SELECT bra.board_id FROM board_role_assignments bra 
            JOIN users u ON bra.user_id = u.id 
            WHERE u.username_normalized = 'board_mod'
        """)
        assigned_boards = set(row[0] for row in cur.fetchall())
        conn.close()

        # 6.2 板块版主登录
        board_mod = AgentClient("BoardMod")
        board_mod.login("board_mod", UNIFIED_PASSWORD)

        # 6.3 验证板块作用域：若 post_board_id 属于 assigned_boards，代改应通过（或在缺少 reason 时返回 400）；
        # 若 post_board_id 不属于 assigned_boards，代改必须被拦截为 403 Forbidden！
        is_assigned = post_board_id in assigned_boards
        res_mod_edit = board_mod.request("PATCH", f"/api/v1/posts/{post_id}", json={
            "title": "版主合规修订",
            "markdown": "内容合规性修订说明",
            "reason": "合规性调整"
        }, headers={"If-Match": "1"})

        if is_assigned:
            self.assert_true(
                res_mod_edit.status_code in [200, 409],  # 200 or 409 version mismatch
                f"Board mod authorized on assigned board ({post_board_id})",
                f"Expected 200/409 on assigned board but got {res_mod_edit.status_code}: {res_mod_edit.text}",
                category="AUTHZ_MODERATION"
            )
        else:
            self.assert_true(
                res_mod_edit.status_code == 403,
                f"Board mod rejected on unassigned board ({post_board_id})",
                f"Expected 403 Forbidden on unassigned board but got {res_mod_edit.status_code}: {res_mod_edit.text}",
                category="AUTHZ_MODERATION"
            )

        # 6.4 全局版主通过 TOTP MFA 登录并审核申诉
        global_mod = AgentClient("GlobalMod")
        mod_login_ok = global_mod.login("moderator", UNIFIED_PASSWORD)
        self.assert_true(
            mod_login_ok,
            "Global moderator login with dynamic TOTP code",
            "Global moderator login failed",
            category="AUTH_MFA"
        )
        if mod_login_ok:
            res_appeals = global_mod.request("GET", "/api/v1/admin/moderation/appeals")
            self.assert_true(
                res_appeals.status_code == 200,
                "Global moderator view appeals queue (GET /api/v1/admin/moderation/appeals)",
                f"Expected 200 but got {res_appeals.status_code}: {res_appeals.text}",
                category="MODERATION"
            )

        return global_mod

    # -------------------------------------------------------------
    # 场景 7: T=24:00 并发竞态与原子性回归
    # -------------------------------------------------------------
    def run_scenario_concurrency_race_conditions(self):
        print("\n=======================================================")
        print("  SCENARIO 7: T=24:00 Concurrency, Race Conditions & Double-Spend")
        print("=======================================================")

        # 7.1 并发签到测试 (防并发 double-reward / 双重入账)
        print("  [Concurrency 1] Testing concurrent daily check-in race conditions...")
        bob = AgentClient("BobConcurrency")
        bob.login("casual_bob", UNIFIED_PASSWORD)

        def make_checkin_request():
            s = InsecureSession()
            for c in bob.session.cookies:
                s.cookies.set(c.name, c.value)
            headers = {
                "X-CSRF-Token": bob.csrf_token,
                "User-Agent": "BBLBB-Concurrent-Checkin"
            }
            cookie_parts = [f"{c.name}={c.value}" for c in bob.session.cookies]
            headers["Cookie"] = "; ".join(cookie_parts)
            return s.post(f"{BASE_URL}/api/v1/activity/visit", json={"path": "/me/balance", "manual": True}, headers=headers, timeout=5)

        # 记录执行前账户金币与 transactions 数量
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT id FROM users WHERE username_normalized = 'casual_bob'")
        bob_id = cur.fetchone()[0]
        cur.execute("SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?", (bob_id, COIN_CURRENCY_ID))
        bal_row = cur.fetchone()
        initial_balance = bal_row[0] if bal_row else 0
        conn.close()

        # 10 个线程并发打卡
        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [executor.submit(make_checkin_request) for _ in range(10)]
            results = [f.result() for f in as_completed(futures)]

        success_count = sum(1 for r in results if r.status_code == 200)
        print(f"    Check-in concurrency: {len(results)} requests sent, {success_count} returned 200 OK")

        # 校验数据库：今天针对 check_in 的 transactions 记录数不得大于 1
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("""
            SELECT count(*) FROM point_transactions pt
            JOIN point_operations po ON pt.operation_id = po.id
            WHERE pt.user_id = ? AND po.memo LIKE '%签到%' AND pt.created_at > ?
        """, (bob_id, int(time.time() - 120) * 1000))
        tx_count = cur.fetchone()[0]
        conn.close()

        self.assert_true(
            tx_count <= 1,
            "Check-in race condition: strictly at most 1 check-in reward granted",
            f"Double reward detected! Found {tx_count} transactions created in parallel burst!",
            category="CONCURRENCY_DOUBLE_SPEND",
            severity="CRITICAL"
        )

        # 7.2 并发点赞测试 (同一帖并发点赞，校验计数与幂等性)
        print("  [Concurrency 2] Testing concurrent like toggle race conditions...")
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT id FROM posts WHERE deleted_at IS NULL LIMIT 1")
        target_p = cur.fetchone()[0]
        conn.close()

        def make_like_request():
            s = InsecureSession()
            for c in bob.session.cookies:
                s.cookies.set(c.name, c.value)
            headers = {
                "X-CSRF-Token": bob.csrf_token,
                "Cookie": "; ".join(f"{c.name}={c.value}" for c in bob.session.cookies)
            }
            return s.post(f"{BASE_URL}/api/v1/posts/{target_p}/reactions", json={"reaction": "like"}, headers=headers, timeout=5)

        with ThreadPoolExecutor(max_workers=8) as executor:
            futures = [executor.submit(make_like_request) for _ in range(8)]
            like_results = [f.result() for f in as_completed(futures)]

        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT count(*) FROM post_reactions WHERE post_id = ? AND user_id = ?", (target_p, bob_id))
        reaction_count = cur.fetchone()[0]
        conn.close()

        self.assert_true(
            reaction_count <= 1,
            "Concurrent like idempotency: exactly <=1 reaction entry in DB",
            f"Duplicate reactions found: {reaction_count} entries for same user and post",
            category="CONCURRENCY",
            severity="HIGH"
        )

        # 7.3 并发注册测试 (5个线程同时注册完全相同的用户名，必须只创建1条记录)
        print("  [Concurrency 3] Testing concurrent registration atomicity...")
        shared_username = f"race_user_{uuid.uuid4().hex[:6]}"
        shared_email = f"{shared_username}@race.example"

        def make_reg_request():
            client = AgentClient(f"RegThread-{uuid.uuid4().hex[:4]}")
            client.refresh_csrf()
            return client.request("POST", "/api/v1/auth/register", json={
                "username": shared_username,
                "email": shared_email,
                "password": UNIFIED_PASSWORD
            })

        with ThreadPoolExecutor(max_workers=5) as executor:
            futures = [executor.submit(make_reg_request) for _ in range(5)]
            reg_results = [f.result() for f in as_completed(futures)]

        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT count(*) FROM users WHERE username_normalized = ?", (shared_username.lower(),))
        actual_db_users = cur.fetchone()[0]
        conn.close()

        self.assert_true(
            actual_db_users == 1,
            "Concurrent registration atomicity: exactly 1 user persisted in DB",
            f"Expected exactly 1 user in DB, found {actual_db_users}!",
            category="CONCURRENCY_RACE_CONDITION",
            severity="CRITICAL"
        )

    # -------------------------------------------------------------
    # 场景 8: [ROUND 3] 受限内容与付费解锁权限体系（付费帖、回复可见、等级限制）
    # -------------------------------------------------------------
    def run_scenario_content_access_control(self, board_id: str):
        print("\n=======================================================")
        print("  SCENARIO 8: [ROUND 3] Gated Content, Paywall & Unlocking Flow")
        print("=======================================================")
        author = AgentClient("AuthorWang")
        author.login("author_wang", UNIFIED_PASSWORD)

        # 8.1 付费帖创建与价格边界校验
        # 价格为 0 -> 422 invalid_price_coin
        res_zero = author.request("POST", "/api/v1/posts", json={
            "title": "零元付费帖测试",
            "markdown": "内容...",
            "board_id": board_id,
            "type": "article",
            "access_policy": "paid",
            "price_coin": 0,
            "client_request_id": f"crl_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_zero.status_code == 422,
            "Paid post rejection on price_coin = 0 (422 invalid_price_coin)",
            f"Expected 422 but got {res_zero.status_code}: {res_zero.text}",
            category="CONTENT_PAYWALL"
        )

        # 价格超限 > 1000 -> 422 invalid_price_coin
        res_high = author.request("POST", "/api/v1/posts", json={
            "title": "高价付费帖测试",
            "markdown": "内容...",
            "board_id": board_id,
            "type": "article",
            "access_policy": "paid",
            "price_coin": 2000,
            "client_request_id": f"crl_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_high.status_code == 422,
            "Paid post rejection on price_coin > 1000 (422 invalid_price_coin)",
            f"Expected 422 but got {res_high.status_code}: {res_high.text}",
            category="CONTENT_PAYWALL"
        )

        # 非 paid 策略带 price_coin -> 422
        res_pub_price = author.request("POST", "/api/v1/posts", json={
            "title": "公开帖带价格测试",
            "markdown": "内容...",
            "board_id": board_id,
            "type": "article",
            "access_policy": "public",
            "price_coin": 100,
            "client_request_id": f"crl_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_pub_price.status_code == 422,
            "Public post rejection with price_coin (422 invalid_price_coin)",
            f"Expected 422 but got {res_pub_price.status_code}: {res_pub_price.text}",
            category="CONTENT_PAYWALL"
        )

        # 8.2 正常创建 50 金币付费帖
        paid_crl = f"paid_crl_{uuid.uuid4().hex}"
        paid_token = uuid.uuid4().hex
        res_paid_post = author.request("POST", "/api/v1/posts", json={
            "title": f"王作者的付费独家架构秘籍_{paid_token[:4]}",
            "markdown": f"## 核心绝密架构图_{paid_token}\n\n这是付费后才能看到的绝密内容：TOKEN_SECRET_{paid_token}！",
            "summary": "这是一篇高质量付费技术深度长文",
            "board_id": board_id,
            "type": "article",
            "access_policy": "paid",
            "price_coin": 50,
            "client_request_id": paid_crl
        })
        self.assert_true(
            res_paid_post.status_code in [200, 201],
            "Create paid post (access_policy='paid', price_coin=50)",
            f"Expected 200/201 but got {res_paid_post.status_code}: {res_paid_post.text}",
            category="CONTENT_PAYWALL"
        )
        if res_paid_post.status_code not in [200, 201]:
            return
        paid_post_id = res_paid_post.json().get("id")

        # 8.3 未购买状态下的遮蔽校验 (Bob 查看)
        bob = AgentClient("BobViewer")
        bob.login("casual_bob", UNIFIED_PASSWORD)
        res_view_locked = bob.request("GET", f"/api/v1/posts/{paid_post_id}")
        self.assert_true(
            res_view_locked.status_code == 200,
            "View paid post metadata (GET /api/v1/posts/{id})",
            f"Expected 200 but got {res_view_locked.status_code}",
            category="CONTENT_PAYWALL"
        )
        locked_data = res_view_locked.json()
        self.assert_true(
            "body_html" not in locked_data or locked_data.get("body_html") is None,
            "Paid content body_html completely masked before unlock",
            f"Sensitive content leaked before payment! body_html={locked_data.get('body_html')}",
            category="SECURITY_MASKING",
            severity="CRITICAL"
        )
        access_sum = locked_data.get("access_summary", {})
        self.assert_true(
            access_sum.get("unlocked") is False,
            "Access summary marks unlocked: false",
            f"Expected unlocked=false, got {access_sum}",
            category="CONTENT_PAYWALL"
        )

        # 8.4 付费解锁流程 (POST /api/v1/posts/{id}/unlock)
        # 获取 Bob 解锁前金币余额并确保充足
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT id FROM users WHERE username_normalized = 'casual_bob'")
        bob_id = cur.fetchone()[0]
        cur.execute("UPDATE point_accounts SET balance = MAX(balance, 500) WHERE user_id = ? AND currency_id = ?", (bob_id, COIN_CURRENCY_ID))
        conn.commit()
        cur.execute("SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?", (bob_id, COIN_CURRENCY_ID))
        bob_before_bal = cur.fetchone()[0]
        conn.close()

        unlock_crl = f"unlock_{uuid.uuid4().hex}"
        res_unlock = bob.request("POST", f"/api/v1/posts/{paid_post_id}/unlock", json={
            "client_request_id": unlock_crl
        })
        self.assert_true(
            res_unlock.status_code == 200,
            "Unlock paid post (POST /api/v1/posts/{id}/unlock)",
            f"Expected 200 but got {res_unlock.status_code}: {res_unlock.text}",
            category="CONTENT_PAYWALL"
        )
        unlock_data = res_unlock.json()
        self.assert_true(
            unlock_data.get("unlocked") is True,
            "Unlock response confirms unlocked: true",
            f"Expected unlocked=true, got {unlock_data}",
            category="CONTENT_PAYWALL"
        )

        # 校验余额扣减（扣除 50 金币）
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?", (bob_id, COIN_CURRENCY_ID))
        bob_after_bal = cur.fetchone()[0]
        conn.close()
        self.assert_true(
            bob_before_bal - bob_after_bal == 50,
            "Coin balance strictly deducted by 50 coins in ledger",
            f"Expected balance deduction 50, before={bob_before_bal}, after={bob_after_bal}",
            category="ECONOMY_LEDGER",
            severity="CRITICAL"
        )

        # 8.5 解锁幂等性验证 (重放同一 client_request_id 或再次解锁同一帖子)
        res_unlock_idemp = bob.request("POST", f"/api/v1/posts/{paid_post_id}/unlock", json={
            "client_request_id": unlock_crl
        })
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?", (bob_id, COIN_CURRENCY_ID))
        bob_idemp_bal = cur.fetchone()[0]
        conn.close()

        self.assert_true(
            res_unlock_idemp.status_code == 200 and bob_idemp_bal == bob_after_bal,
            "Unlock idempotency: replay does NOT double-charge user balance",
            f"Double charge detected! Balance changed from {bob_after_bal} to {bob_idemp_bal}",
            category="ECONOMY_DOUBLE_SPEND",
            severity="CRITICAL"
        )

        # 8.6 解锁后查看内容 (body_html 现已完整可见)
        res_view_unlocked = bob.request("GET", f"/api/v1/posts/{paid_post_id}")
        unlocked_data = res_view_unlocked.json()
        has_secret = f"TOKEN_SECRET_{paid_token}" in unlocked_data.get("body_html", "")
        self.assert_true(
            has_secret and unlocked_data.get("access_summary", {}).get("unlocked") is True,
            "Paid content body_html visible after payment grant",
            f"Content still missing after successful unlock! body={unlocked_data.get('body_html')}",
            category="CONTENT_PAYWALL"
        )

        # 8.7 余额不足解锁拦截 (创建另一个高价帖子，使用0金币用户尝试购买)
        exp_token = uuid.uuid4().hex
        res_expensive = author.request("POST", "/api/v1/posts", json={
            "title": f"超高价帖子_{exp_token[:4]}",
            "markdown": f"超高价值内容_{exp_token}",
            "board_id": board_id,
            "type": "article",
            "access_policy": "paid",
            "price_coin": 500,
            "client_request_id": f"exp_crl_{uuid.uuid4().hex}"
        })
        expensive_id = res_expensive.json().get("id")

        pauper = AgentClient("PauperUser")
        pauper.login("newbie_unverified", UNIFIED_PASSWORD)
        res_insufficient = pauper.request("POST", f"/api/v1/posts/{expensive_id}/unlock", json={
            "client_request_id": f"pauper_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_insufficient.status_code == 409 and "insufficient_funds" in res_insufficient.text,
            "Insufficient funds rejection on unlock (409 insufficient_funds)",
            f"Expected 409 insufficient_funds but got {res_insufficient.status_code}: {res_insufficient.text}",
            category="ECONOMY_SECURITY",
            severity="HIGH"
        )

        # 8.8 回复可见（after_reply）策略生命周期验证
        reply_post_crl = f"rep_crl_{uuid.uuid4().hex}"
        rep_token = uuid.uuid4().hex
        res_reply_post = author.request("POST", "/api/v1/posts", json={
            "title": f"王作者的回复可见技术帖_{rep_token[:4]}",
            "markdown": f"## 隐藏内容_{rep_token}\n\n回复后才能阅读的提取码：SECRET_REPLY_{rep_token}！",
            "board_id": board_id,
            "type": "discussion",
            "access_policy": "after_reply",
            "client_request_id": reply_post_crl
        })
        reply_post_id = res_reply_post.json().get("id")

        # Bob 回复前查看 -> 锁定
        res_reply_view_pre = bob.request("GET", f"/api/v1/posts/{reply_post_id}")
        pre_data = res_reply_view_pre.json()
        self.assert_true(
            "body_html" not in pre_data or pre_data.get("body_html") is None,
            "after_reply post body_html masked before replying",
            f"Content leaked before reply: {pre_data.get('body_html')}",
            category="CONTENT_ACCESS_POLICY"
        )

        # Bob 回复帖子
        res_bob_comment = bob.request("POST", f"/api/v1/posts/{reply_post_id}/comments", json={
            "markdown": "感谢楼主分享，回复支持一下！",
            "client_request_id": f"bob_rep_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_bob_comment.status_code in [200, 201],
            "Reply to after_reply post",
            f"Expected 200/201 but got {res_bob_comment.status_code}: {res_bob_comment.text}",
            category="CONTENT_ACCESS_POLICY"
        )

        # Bob 回复后查看 -> 现已解锁可见
        res_reply_view_post = bob.request("GET", f"/api/v1/posts/{reply_post_id}")
        post_reply_data = res_reply_view_post.json()
        has_reply_secret = f"SECRET_REPLY_{rep_token}" in post_reply_data.get("body_html", "")
        self.assert_true(
            has_reply_secret and post_reply_data.get("access_summary", {}).get("unlocked") is True,
            "after_reply post unlocked and body_html visible after replying",
            f"Content still masked after reply! body={post_reply_data.get('body_html')}",
            category="CONTENT_ACCESS_POLICY"
        )

        # 8.9 等级可见限制（level）校验
        # 尝试设置高于作者自身等级的 visibility_level -> 400
        res_level_exceed = author.request("POST", "/api/v1/posts", json={
            "title": "超越作者等级帖子",
            "markdown": "无法创建的内容",
            "board_id": board_id,
            "type": "discussion",
            "access_policy": "level",
            "visibility_level": 5,  # author_wang level is 1
            "client_request_id": f"lv_ex_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_level_exceed.status_code == 400,
            "Post creation rejected when visibility_level > author level",
            f"Expected 400 but got {res_level_exceed.status_code}: {res_level_exceed.text}",
            category="CONTENT_ACCESS_POLICY"
        )

    # -------------------------------------------------------------
    # 场景 9: [ROUND 4] 审核处罚完整生命周期与禁言封禁生效边界（处罚-禁言拦截-申诉-复核隔离-解禁恢复）
    # -------------------------------------------------------------
    def run_scenario_sanction_appeal_lifecycle(self, board_id: str, post_id: str, mod: AgentClient):
        print("\n=======================================================")
        print("  SCENARIO 9: [ROUND 4] Sanctions, Mute Enforcement & Appeals Lifecycle")
        print("=======================================================")

        # 9.1 全局版主对测试用户 (creative_artist) 下发临时禁言处罚
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT id FROM users WHERE username_normalized = 'creative_artist'")
        target_uid = cur.fetchone()[0]
        conn.close()

        now_ms = int(time.time() * 1000)
        res_sanction = mod.request("POST", "/api/v1/admin/moderation/sanctions", json={
            "target_user_id": target_uid,
            "kind": "mute",
            "reason": "发布违规广告刷屏",
            "starts_at": now_ms,
            "ends_at": now_ms + 3600_000  # 禁言 1 小时
        })
        self.assert_true(
            res_sanction.status_code in [200, 201],
            "Moderator issues temporary mute sanction (POST /api/v1/admin/moderation/sanctions)",
            f"Expected 200/201 but got {res_sanction.status_code}: {res_sanction.text}",
            category="MODERATION_SANCTION"
        )
        if res_sanction.status_code not in [200, 201]:
            return
        sanction_data = res_sanction.json()
        sanction_id = sanction_data.get("id")

        # 9.2 验证禁言生效边界：被禁言用户发表帖子与评论必须被权限层拦截 (403 Forbidden)
        artist = AgentClient("MutedArtist")
        artist.login("creative_artist", UNIFIED_PASSWORD)

        res_muted_post = artist.request("POST", "/api/v1/posts", json={
            "title": "被禁言用户的尝试发帖",
            "markdown": "测试内容...",
            "board_id": board_id,
            "type": "discussion",
            "access_policy": "public",
            "client_request_id": f"mut_p_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_muted_post.status_code == 403,
            "Muted user post creation rejected (403 Forbidden)",
            f"Expected 403 when muted, but got {res_muted_post.status_code}: {res_muted_post.text}",
            category="AUTHZ_SECURITY",
            severity="CRITICAL"
        )

        res_muted_comment = artist.request("POST", f"/api/v1/posts/{post_id}/comments", json={
            "markdown": "被禁言用户的尝试评论...",
            "client_request_id": f"mut_c_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_muted_comment.status_code == 403,
            "Muted user comment creation rejected (403 Forbidden)",
            f"Expected 403 when muted, but got {res_muted_comment.status_code}: {res_muted_comment.text}",
            category="AUTHZ_SECURITY",
            severity="CRITICAL"
        )

        # 9.3 被处罚用户查看我的处罚列表并提交申诉 (POST /api/v1/appeals)
        res_my_sanc = artist.request("GET", "/api/v1/me/sanctions")
        self.assert_true(
            res_my_sanc.status_code == 200,
            "List my sanctions (GET /api/v1/me/sanctions)",
            f"Expected 200 but got {res_my_sanc.status_code}",
            category="MODERATION_APPEALS"
        )

        res_appeal = artist.request("POST", "/api/v1/appeals", json={
            "sanction_id": sanction_id,
            "content": "我已深切认识到不当言论问题，申请解除禁言处罚"
        })
        self.assert_true(
            res_appeal.status_code == 201,
            "Submit appeal for sanction (POST /api/v1/appeals)",
            f"Expected 201 but got {res_appeal.status_code}: {res_appeal.text}",
            category="MODERATION_APPEALS"
        )
        if res_appeal.status_code != 201:
            return
        appeal_data = res_appeal.json()
        appeal_id = appeal_data.get("id")

        # 重复申诉拦截（同一处罚在处理中禁止重复提交）
        res_dup_appeal = artist.request("POST", "/api/v1/appeals", json={
            "sanction_id": sanction_id,
            "content": "重复催促申诉"
        })
        self.assert_true(
            res_dup_appeal.status_code in [400, 409],
            "Duplicate appeal rejection while pending",
            f"Expected 400/409 but got {res_dup_appeal.status_code}: {res_dup_appeal.text}",
            category="MODERATION_APPEALS"
        )

        # 9.4 利益冲突防线：原处罚做出者 (moderator) 不得作为复核人裁决自身案件
        res_self_review = mod.request("PATCH", f"/api/v1/admin/moderation/appeals/{appeal_id}", json={
            "decision": "upheld",
            "reason": "原版主自我撤销",
            "expected_version": 1
        })
        self.assert_true(
            res_self_review.status_code in [400, 403, 409],
            "Conflict of Interest defense: original sanctioner CANNOT decide own appeal",
            f"Expected rejection (400/403/409) for conflict of interest, got {res_self_review.status_code}",
            category="MODERATION_GOVERNANCE",
            severity="HIGH"
        )

        # 9.5 超级管理员 (admin) 作为独立复核人裁决申诉并撤销处罚 (uphold)
        admin = AgentClient("SuperAdmin")
        admin.login("admin", UNIFIED_PASSWORD)

        # 获取申诉当前版本
        res_adm_detail = admin.request("GET", f"/api/v1/admin/moderation/appeals/{appeal_id}")
        adm_appeal_obj = res_adm_detail.json() if res_adm_detail.status_code == 200 else {}
        appeal_ver = adm_appeal_obj.get("version", adm_appeal_obj.get("updated_at", 1))
        if isinstance(appeal_ver, str):
            appeal_ver = 1

        res_uphold = admin.request("PATCH", f"/api/v1/admin/moderation/appeals/{appeal_id}", json={
            "decision": "upheld",
            "reason": "初犯认错态度良好，独立复核决定予以提前解禁",
            "expected_version": appeal_ver
        })
        self.assert_true(
            res_uphold.status_code == 200,
            "Independent admin upholds appeal and revokes sanction (PATCH ...)",
            f"Expected 200 but got {res_uphold.status_code}: {res_uphold.text}",
            category="MODERATION_APPEALS"
        )

        # 9.6 验证处罚撤销（Sanction Reversal）与发帖权限恢复
        res_restored_post = artist.request("POST", "/api/v1/posts", json={
            "title": f"解禁后的重新出发创作_{uuid.uuid4().hex[:4]}",
            "markdown": "重新回归社区，感谢管理层公正复核！",
            "board_id": board_id,
            "type": "discussion",
            "access_policy": "public",
            "client_request_id": f"res_p_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_restored_post.status_code in [200, 201],
            "Post creation restored after appeal upheld (201 Created)",
            f"Permissions not restored after appeal! Got {res_restored_post.status_code}: {res_restored_post.text}",
            category="MODERATION_SANCTION",
            severity="CRITICAL"
        )

    # -------------------------------------------------------------
    # 场景 10: [ROUND 5] 经济系统强一致性与商城并发秒杀超卖（库存扣减、双重消费防线、流水账目校验）
    # -------------------------------------------------------------
    def run_scenario_economy_concurrency_ledger(self):
        print("\n=======================================================")
        print("  SCENARIO 10: [ROUND 5] Economy Concurrency, Flash-Sale & Ledger Invariants")
        print("=======================================================")

        # 10.1 准备限量 2 件的秒杀商品
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("""
            INSERT OR REPLACE INTO shop_products (
                id, kind, status, slug, title, description_safe, icon_token,
                presentation_tokens_json, slot, currency_id, unit_price, quantity_limit,
                stock_remaining, required_level, validity_seconds, refund_policy,
                version, created_by, created_at, updated_at
            ) VALUES (
                'prod-flash-sale', 'cosmetic_badge', 'published', 'flash-sale-badge',
                '限量绝版秒杀徽章', '并发秒杀测试专属', 'award', '["badge.flash"]',
                'profile_badge', '01911fd5-0047-0000-0000-000000000002', 10, 1,
                2, 1, NULL, 'non_refundable', 1, '01a09867-dbdd-7f08-8c0d-90ba2f149adf',
                1789400000000, 1789400000000
            )
        """)
        # 清理该商品的既有订单与权益
        cur.execute("DELETE FROM shop_orders WHERE product_id = 'prod-flash-sale'")
        cur.execute("DELETE FROM user_entitlements WHERE product_id = 'prod-flash-sale'")
        conn.commit()
        conn.close()

        # 10.2 5 个不同 Agent 客户端并发抢购该限量商品
        users_to_race = ["active_alice", "tech_expert", "rich_buyer", "casual_bob", "creative_artist"]
        buyers = []
        for u in users_to_race:
            client = AgentClient(f"FlashBuyer-{u}")
            client.login(u, UNIFIED_PASSWORD)
            buyers.append(client)

        def make_purchase(client: AgentClient):
            order_key = f"fl_{uuid.uuid4().hex}"
            return client.request("POST", "/api/v1/shop/orders", json={
                "product_id": "prod-flash-sale",
                "quantity": 1,
                "idempotency_key": order_key
            })

        with ThreadPoolExecutor(max_workers=5) as executor:
            futures = [executor.submit(make_purchase, b) for b in buyers]
            results = [f.result() for f in as_completed(futures)]

        success_orders = [r for r in results if r.status_code in [200, 201]]
        exhausted_orders = [r for r in results if r.status_code == 409]

        self.assert_true(
            len(success_orders) == 2,
            "Flash-sale concurrency: exactly 2 purchases succeed when stock=2",
            f"Expected exactly 2 successes, got {len(success_orders)} successes (status codes: {[r.status_code for r in results]})",
            category="CONCURRENCY_OVERSELL",
            severity="CRITICAL"
        )
        self.assert_true(
            len(exhausted_orders) == 3,
            "Flash-sale concurrency: exactly 3 purchases fail with 409 OutOfStock",
            f"Expected 3 rejected with 409, got {len(exhausted_orders)} (statuses: {[r.status_code for r in results]}, texts: {[r.text[:80] for r in results]})",
            category="CONCURRENCY_OVERSELL",
            severity="HIGH"
        )

        # 检查数据库库存与权益数量
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("SELECT stock_remaining FROM shop_products WHERE id = 'prod-flash-sale'")
        stock_in_db = cur.fetchone()[0]
        cur.execute("SELECT count(*) FROM user_entitlements WHERE product_id = 'prod-flash-sale'")
        ent_count = cur.fetchone()[0]
        cur.execute("SELECT count(*) FROM shop_orders WHERE product_id = 'prod-flash-sale' AND status = 'succeeded'")
        order_count = cur.fetchone()[0]
        conn.close()

        self.assert_true(
            stock_in_db == 0 and ent_count == 2 and order_count == 2,
            "Inventory invariant: stock_remaining strictly 0, entitlements strictly 2, orders strictly 2",
            f"Oversell detected! stock={stock_in_db}, entitlements={ent_count}, orders={order_count}",
            category="CONCURRENCY_OVERSELL",
            severity="CRITICAL"
        )

        # 10.3 限购拦截校验：已成功购买的用户再次购买必须被拒绝（409 purchase_limit_exceeded）
        winner_client = buyers[0]
        res_rep_buy = winner_client.request("POST", "/api/v1/shop/orders", json={
            "product_id": "prod-flash-sale",
            "quantity": 1,
            "idempotency_key": f"dup_buy_{uuid.uuid4().hex}"
        })
        self.assert_true(
            res_rep_buy.status_code == 409,
            "Purchase limit exceeded rejection (409 Conflict)",
            f"Expected 409 on second purchase, got {res_rep_buy.status_code}: {res_rep_buy.text}",
            category="SHOP_PURCHASE_LIMIT"
        )

        # 10.4 全局账务不变量审计 (Ledger Invariant Audit)
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("""
            SELECT pa.user_id, pa.currency_id, pa.balance,
                   COALESCE(SUM(pt.delta_balance), 0) as total_delta
            FROM point_accounts pa
            LEFT JOIN point_transactions pt ON pa.user_id = pt.user_id AND pa.currency_id = pt.currency_id
            GROUP BY pa.user_id, pa.currency_id
        """)
        mismatches = []
        for uid, cid, bal, total_delta in cur.fetchall():
            if bal < 0:
                mismatches.append(f"Negative balance for user {uid}: {bal}")
        conn.close()

        self.assert_true(
            len(mismatches) == 0,
            "Global ledger non-negative invariant: zero accounts with negative balance",
            f"Negative balance detected in ledger: {mismatches}",
            category="ECONOMY_LEDGER",
            severity="CRITICAL"
        )

    # -------------------------------------------------------------
    # 场景 11: [ROUND 6] 全站通知网络、偏好免打扰与已读状态一致性
    # -------------------------------------------------------------
    def run_scenario_notifications_network(self, board_id: str):
        print("\n=======================================================")
        print("  SCENARIO 11: [ROUND 6] Notifications Network, Preferences & Read State")
        print("=======================================================")
        alice = AgentClient("AliceNotify")
        alice.login("active_alice", UNIFIED_PASSWORD)
        bob = AgentClient("BobNotifier")
        bob.login("casual_bob", UNIFIED_PASSWORD)

        # 11.1 通知偏好设置读取与边界负向测试
        res_prefs = alice.request("GET", "/api/v1/notifications/preferences")
        self.assert_true(
            res_prefs.status_code == 200,
            "Get notification preferences (GET /api/v1/notifications/preferences)",
            f"Expected 200 but got {res_prefs.status_code}",
            category="NOTIFICATIONS"
        )

        # 尝试将 security 类通知所有通道全关 -> 400 拦截 (安全偏好强制保障)
        res_sec_disable = alice.request("PUT", "/api/v1/notifications/preferences", json={
            "category": "security",
            "email_enabled": False,
            "in_app_enabled": False,
            "push_enabled": False
        })
        self.assert_true(
            res_sec_disable.status_code == 400,
            "Security notification cannot be fully disabled (400 Bad Request)",
            f"Expected 400 when disabling security alerts, got {res_sec_disable.status_code}: {res_sec_disable.text}",
            category="NOTIFICATIONS_SECURITY",
            severity="HIGH"
        )

        # 正常更新 activity 偏好
        res_act_pref = alice.request("PUT", "/api/v1/notifications/preferences", json={
            "category": "activity",
            "email_enabled": False,
            "in_app_enabled": True,
            "push_enabled": False
        })
        self.assert_true(
            res_act_pref.status_code == 200,
            "Update activity notification preference (PUT ...)",
            f"Expected 200 but got {res_act_pref.status_code}: {res_act_pref.text}",
            category="NOTIFICATIONS"
        )

        # 11.2 互动触发站内通知（Bob 在 Alice 帖子下评论并 @active_alice）
        notify_post_crl = f"notif_{uuid.uuid4().hex}"
        res_post = alice.request("POST", "/api/v1/posts", json={
            "title": f"爱丽丝的通知触发测试帖子_{uuid.uuid4().hex[:4]}",
            "markdown": "测试站内通知流转通道...",
            "board_id": board_id,
            "type": "discussion",
            "access_policy": "public",
            "client_request_id": notify_post_crl
        })
        notify_post_id = res_post.json().get("id")

        if notify_post_id:
            res_comment = bob.request("POST", f"/api/v1/posts/{notify_post_id}/comments", json={
                "markdown": "你好 @active_alice ，测试站内通知及时送达！",
                "client_request_id": f"c_notif_{uuid.uuid4().hex}"
            })
            self.assert_true(
                res_comment.status_code in [200, 201],
                "Bob comments and mentions Alice to trigger notification",
                f"Expected 200/201 but got {res_comment.status_code}",
                category="NOTIFICATIONS"
            )

            # 11.3 Alice 接收并检查通知列表
            res_notifs = alice.request("GET", "/api/v1/notifications?unread_only=true")
            self.assert_true(
                res_notifs.status_code == 200,
                "List unread notifications (GET /api/v1/notifications?unread_only=true)",
                f"Expected 200 but got {res_notifs.status_code}",
                category="NOTIFICATIONS"
            )
            notif_data = res_notifs.json()
            items = notif_data.get("items", [])
            unread_cnt = notif_data.get("unread_count", 0)

            self.assert_true(
                len(items) > 0 and unread_cnt > 0,
                "Unread notification received and unread_count incremented",
                f"Expected items > 0 and unread_count > 0, got items={len(items)}, unread_cnt={unread_cnt}",
                category="NOTIFICATIONS",
                severity="HIGH"
            )

            # 11.4 单条标记已读与重复标记测试
            if items:
                target_notif = items[0]
                target_nid = target_notif["id"]

                res_read = alice.request("POST", f"/api/v1/notifications/{target_nid}/read")
                self.assert_true(
                    res_read.status_code in [200, 204],
                    "Mark single notification as read (POST .../{id}/read)",
                    f"Expected 200/204 but got {res_read.status_code}",
                    category="NOTIFICATIONS"
                )

                # 重复标记已读 -> 404 (already read)
                res_read_repeat = alice.request("POST", f"/api/v1/notifications/{target_nid}/read")
                self.assert_true(
                    res_read_repeat.status_code == 404,
                    "Repeat mark-read rejection (404 already read)",
                    f"Expected 404 on already-read notification, got {res_read_repeat.status_code}",
                    category="NOTIFICATIONS"
                )

            # 11.5 全量一键标记已读 (POST /api/v1/notifications/read-all)
            res_read_all = alice.request("POST", "/api/v1/notifications/read-all")
            self.assert_true(
                res_read_all.status_code == 200,
                "Mark all notifications as read (POST /api/v1/notifications/read-all)",
                f"Expected 200 but got {res_read_all.status_code}",
                category="NOTIFICATIONS"
            )

            # 检查已读后状态一致性 (unread_count 应为 0)
            res_after_read = alice.request("GET", "/api/v1/notifications?unread_only=true")
            after_data = res_after_read.json()
            after_unread = after_data.get("unread_count", 0)
            self.assert_true(
                after_unread == 0,
                "Unread count resets strictly to 0 after read-all",
                f"Expected unread_count 0, got {after_unread}",
                category="NOTIFICATIONS",
                severity="HIGH"
            )

    # -------------------------------------------------------------
    # 场景 12: [ROUND 7] 附件存储授权、下载计费与配额超限防线
    # -------------------------------------------------------------
    def run_scenario_attachments_storage_quota(self):
        print("\n=======================================================")
        print("  SCENARIO 12: [ROUND 7] Attachments Storage, Quota & Protection")
        print("=======================================================")
        alice = AgentClient("AliceUploader")
        alice.login("active_alice", UNIFIED_PASSWORD)
        bob = AgentClient("BobAttacker")
        bob.login("casual_bob", UNIFIED_PASSWORD)

        # 12.1 查询当前容量与附件列表
        res_list = alice.request("GET", "/api/v1/attachments")
        self.assert_true(
            res_list.status_code == 200,
            "List my attachments and quota summary (GET /api/v1/attachments)",
            f"Expected 200 but got {res_list.status_code}",
            category="STORAGE_ATTACHMENTS"
        )

        # 12.2 创建合法图片附件（两阶段上传第 1 步）
        att_key = f"att_{uuid.uuid4().hex}"
        res_create = alice.request("POST", "/api/v1/attachments", json={
            "filename": "demo_test_image.png",
            "size": 1024,
            "declared_media_type": "image/png"
        }, headers={"Idempotency-Key": att_key})
        self.assert_true(
            res_create.status_code in [200, 201],
            "Create attachment entry (POST /api/v1/attachments)",
            f"Expected 200/201 but got {res_create.status_code}: {res_create.text}",
            category="STORAGE_ATTACHMENTS"
        )
        if res_create.status_code not in [200, 201]:
            return
        att_obj = res_create.json()
        att_id = att_obj.get("attachment", {}).get("id") or att_obj.get("id")

        # 12.3 幂等性校验：同 Idempotency-Key 重放返回同一附件记录
        res_idemp = alice.request("POST", "/api/v1/attachments", json={
            "filename": "demo_test_image.png",
            "size": 1024,
            "declared_media_type": "image/png"
        }, headers={"Idempotency-Key": att_key})
        idemp_att_id = res_idemp.json().get("attachment", {}).get("id") or res_idemp.json().get("id")
        self.assert_true(
            res_idemp.status_code in [200, 201] and idemp_att_id == att_id,
            "Attachment creation idempotency: replay returns same attachment ID",
            f"Expected same attachment {att_id}, got {idemp_att_id}",
            category="STORAGE_ATTACHMENTS"
        )

        # 12.4 配额超限边界负向测试（声明超大体积 100GB）
        huge_key = f"huge_{uuid.uuid4().hex}"
        res_huge = alice.request("POST", "/api/v1/attachments", json={
            "filename": "huge_file.png",
            "size": 100 * 1024 * 1024 * 1024,  # 100GB
            "declared_media_type": "image/png"
        }, headers={"Idempotency-Key": huge_key})
        self.assert_true(
            res_huge.status_code == 409 and "quota_exceeded" in res_huge.text,
            "Oversized upload rejected by quota policy (409 quota_exceeded)",
            f"Expected 409 quota_exceeded, got {res_huge.status_code}: {res_huge.text}",
            category="STORAGE_QUOTA",
            severity="HIGH"
        )

        # 12.5 越权删除拦截：非本人用户尝试删除附件必须被拒绝 (403/404)
        if att_id:
            res_unauth_del = bob.request("DELETE", f"/api/v1/attachments/{att_id}")
            self.assert_true(
                res_unauth_del.status_code in [403, 404],
                "Unauthorized delete of another user's attachment rejected",
                f"Expected 403/404 when deleting other's attachment, got {res_unauth_del.status_code}",
                category="STORAGE_SECURITY",
                severity="CRITICAL"
            )

            # 12.6 作者本人删除附件
            res_auth_del = alice.request("DELETE", f"/api/v1/attachments/{att_id}")
            self.assert_true(
                res_auth_del.status_code in [200, 204],
                "Owner delete own attachment (DELETE /api/v1/attachments/{id})",
                f"Expected 200/204 but got {res_auth_del.status_code}",
                category="STORAGE_ATTACHMENTS"
            )

    # -------------------------------------------------------------
    # 场景 13: [ROUND 8] 全系统随机交错多Agent压力测试与全库不变量完整审计验证
    # -------------------------------------------------------------
    def run_scenario_full_database_invariant_audit(self):
        print("\n=======================================================")
        print("  SCENARIO 13: [ROUND 8] Full Database Invariant & Integrity Audit")
        print("=======================================================")

        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()

        # 13.1 外键完整性审计 (PRAGMA foreign_key_check)
        cur.execute("PRAGMA foreign_key_check;")
        fk_violations = cur.fetchall()
        self.assert_true(
            len(fk_violations) == 0,
            "Database foreign key integrity: 0 foreign key violations",
            f"Foreign key violations found: {fk_violations}",
            category="DATABASE_INTEGRITY",
            severity="CRITICAL"
        )

        # 13.2 账户负余额不变量审计
        cur.execute("SELECT count(*) FROM point_accounts WHERE balance < 0;")
        neg_bal = cur.fetchone()[0]
        self.assert_true(
            neg_bal == 0,
            "Point accounts non-negative balance invariant: 0 negative balances",
            f"Found {neg_bal} accounts with negative balance!",
            category="DATABASE_INTEGRITY",
            severity="CRITICAL"
        )

        cur.execute("SELECT count(*) FROM point_accounts WHERE frozen_balance < 0;")
        neg_froz = cur.fetchone()[0]
        self.assert_true(
            neg_froz == 0,
            "Point accounts non-negative frozen balance invariant: 0 negative frozen balances",
            f"Found {neg_froz} accounts with negative frozen balance!",
            category="DATABASE_INTEGRITY",
            severity="CRITICAL"
        )

        # 13.3 孤儿正文记录审计 (post_contents without posts)
        cur.execute("SELECT count(*) FROM post_contents WHERE post_id NOT IN (SELECT id FROM posts);")
        orphan_contents = cur.fetchone()[0]
        self.assert_true(
            orphan_contents == 0,
            "Post contents relation invariant: 0 orphan content rows",
            f"Found {orphan_contents} orphan post_contents rows!",
            category="DATABASE_INTEGRITY",
            severity="HIGH"
        )

        # 13.4 商城商品负库存不变量审计
        cur.execute("SELECT count(*) FROM shop_products WHERE stock_remaining < 0;")
        neg_stock = cur.fetchone()[0]
        self.assert_true(
            neg_stock == 0,
            "Shop inventory non-negative invariant: 0 negative stock products",
            f"Found {neg_stock} products with negative stock!",
            category="DATABASE_INTEGRITY",
            severity="CRITICAL"
        )

        # 13.5 事务流水账本一致性验证 (point_transactions delta_balance)
        cur.execute("SELECT count(*) FROM point_transactions WHERE delta_balance = 0;")
        zero_delta = cur.fetchone()[0]
        self.assert_true(
            zero_delta == 0,
            "Transaction ledger hygiene: 0 zero-delta dirty transaction entries",
            f"Found {zero_delta} zero-delta transaction records in ledger!",
            category="DATABASE_INTEGRITY",
            severity="MEDIUM"
        )

        conn.close()

    def run_all(self):
        print("\n=======================================================")
        print("  STARTING MULTI-AGENT SYSTEM SIMULATION (ROUNDS 1-8)")
        print("=======================================================")
        start_time = time.time()

        # 清理此前仿真轮次产生的历史临时帖子，避免持续累积触发每小时发帖频次风控
        conn = sqlite3.connect(DB_PATH)
        cur = conn.cursor()
        cur.execute("""
            UPDATE posts SET created_at = created_at - 7200000 
            WHERE title LIKE '%实验%' OR title LIKE '%测试%' OR title LIKE '%架构%' 
               OR title LIKE '%通知%' OR title LIKE '%超高价%' OR title LIKE '%回复可见%' 
               OR title LIKE '%绝密%' OR title LIKE '%独家%' OR title LIKE '%模拟%'
        """)
        conn.commit()
        conn.close()

        self.run_scenario_newbie_auth()
        post_id, comment_id, board_id = self.run_scenario_active_community()
        self.run_scenario_cross_user_interaction(post_id)
        self.run_scenario_shop_and_wardrobe()
        self.run_scenario_malicious_attacker(post_id, comment_id)
        global_mod = self.run_scenario_moderation_scopes(post_id, board_id)
        self.run_scenario_concurrency_race_conditions()
        self.run_scenario_content_access_control(board_id)
        self.run_scenario_sanction_appeal_lifecycle(board_id, post_id, global_mod)
        self.run_scenario_economy_concurrency_ledger()
        self.run_scenario_notifications_network(board_id)
        self.run_scenario_attachments_storage_quota()
        self.run_scenario_full_database_invariant_audit()

        elapsed = time.time() - start_time
        print("\n=======================================================")
        print("  SIMULATION COMPLETED IN {:.2f}s".format(elapsed))
        print(f"  Total tests: {self.test_stats['total']}")
        print(f"  Passed: {self.test_stats['passed']}")
        print(f"  Failed: {self.test_stats['failed']}")
        print(f"  Defects Found: {len(self.issues_found)}")
        print("=======================================================\n")
        return self.issues_found

if __name__ == "__main__":
    rounds = 1
    if len(sys.argv) > 1 and sys.argv[1].isdigit():
        rounds = int(sys.argv[1])

    all_issues = []
    for r in range(1, rounds + 1):
        if r > 1:
            # 等待进入下一个独立的 RFC 6238 TOTP 30秒步长周期，避免触发动态口令防重放拦截
            remaining = 30 - (int(time.time()) % 30) + 1
            print(f"\nWaiting {remaining}s for next 30-second TOTP time step to ensure clean MFA auth...")
            time.sleep(remaining)

        print(f"\n#######################################################")
        print(f"       STARTING CONTINUOUS SIMULATION CYCLE {r}/{rounds}")
        print(f"#######################################################")
        suite = SimulationSuiteRound2()
        issues = suite.run_all()
        if issues:
            all_issues.extend(issues)
            print(f"Cycle {r} encountered {len(issues)} issues!")
            break
        else:
            print(f"Cycle {r} PASSED WITH ZERO DEFECTS!")

    if all_issues:
        with open("simulation_defects_round2.json", "w", encoding="utf-8") as f:
            json.dump(all_issues, f, ensure_ascii=False, indent=2)
        print(f"Defects report saved to simulation_defects_round2.json ({len(all_issues)} issues)")
        sys.exit(1)
    else:
        print(f"\nALL {rounds} CONTINUOUS SIMULATION CYCLES PASSED WITH 0 DEFECTS!")
