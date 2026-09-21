#!/usr/bin/env python3
"""
下载 Steam 点数商店全量动效透明头像框并本地化。

流程：
1. 从 Steam API（community_item_classes[0]=14）拉取最新头像框定义列表，
   与现有 JSON 目录按 defid 合并（保留旧条目的 shape/scale 等富字段）；
2. 并发下载全部缺失的 PNG 到 frontend/static/cosmetics/frames/steam/。
"""
import urllib.request
import urllib.parse
import json
import os
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JSON_PATH = os.path.join(BASE_DIR, 'frontend', 'src', 'lib', 'data', 'steam-avatar-frames.json')
OUT_DIR = os.path.join(BASE_DIR, 'uploads', 'steam-assets', 'frames')

os.makedirs(OUT_DIR, exist_ok=True)

# ── 1. 刷新目录（class 14 = avatar frames，language=schinese 与既有目录一致） ──
print("Fetching Steam avatar frame definitions (class 14)...")
cursor = ''
fetched = {}
for page in range(30):
    qs = urllib.parse.urlencode({
        'count': '500',
        'community_item_classes[0]': '14',
        'language': 'schinese',
        'cursor': cursor,
    })
    url = f'https://api.steampowered.com/ILoyaltyRewardsService/QueryRewardItems/v1/?{qs}'
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    try:
        data = json.loads(urllib.request.urlopen(req, timeout=15).read())
    except Exception as e:
        print(f"Error fetching page {page}: {e}")
        break

    defs = data.get('response', {}).get('definitions', [])
    if not defs:
        break
    for item in defs:
        defid = item.get('defid')
        if defid is None or defid in fetched:
            continue
        c = item.get('community_item_data') or {}
        image = c.get('item_image_large') or c.get('item_image_small')
        if not image:
            continue
        fetched[defid] = {
            'id': f'steam_{defid}',
            'defid': defid,
            'appid': item.get('appid'),
            'name': c.get('item_title') or c.get('item_name') or '未命名头像框',
            'image': image,
            'cost': int(item.get('point_cost', '2000'))
        }
    cursor = data.get('response', {}).get('next_cursor', '')
    if not cursor:
        break

print(f"Fetched {len(fetched)} avatar frame definitions from Steam.")

# 与现有目录合并：保留旧条目（含 shape/scale），追加新 defid。
existing = []
if os.path.exists(JSON_PATH):
    with open(JSON_PATH, 'r', encoding='utf-8') as f:
        existing = json.load(f)

existing_by_defid = {x['defid']: x for x in existing if 'defid' in x}
merged = []
for x in existing:
    fresh = fetched.get(x.get('defid'))
    if fresh:
        # 保留富字段，仅回填可能缺失的 appid/name/image/cost
        x.setdefault('appid', fresh['appid'])
        merged.append(x)
    else:
        merged.append(x)
added = 0
for defid, fresh in fetched.items():
    if defid not in existing_by_defid:
        merged.append(fresh)
        added += 1

print(f"Catalog merged: {len(existing)} existing + {added} new = {len(merged)} entries.")
with open(JSON_PATH, 'w', encoding='utf-8') as f:
    json.dump(merged, f, ensure_ascii=False, separators=(',', ':'))

# ── 2. 并发下载缺失文件 ──
items = merged
total = len(items)
print(f"Total frames to download: {total}")


def download_one(item):
    fname = item['image']
    dest = os.path.join(OUT_DIR, fname)

    # 如果本地已经完整下载过且大小合理则跳过
    if os.path.exists(dest) and os.path.getsize(dest) > 1000:
        return item['defid'], True, 'exists'

    url = f"https://shared.fastly.steamstatic.com/community_assets/images/items/{item['appid']}/{fname}"
    for attempt in range(3):
        try:
            req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'})
            with urllib.request.urlopen(req, timeout=12) as response:
                data = response.read()
            if len(data) > 0:
                with open(dest, 'wb') as f:
                    f.write(data)
                return item['defid'], True, 'downloaded'
        except Exception:
            time.sleep(0.5)
            continue

    # 如果 small 失败，尝试 large
    large_img = item.get('large')
    if large_img and large_img != fname:
        url_large = f"https://shared.fastly.steamstatic.com/community_assets/images/items/{item['appid']}/{large_img}"
        try:
            req = urllib.request.Request(url_large, headers={'User-Agent': 'Mozilla/5.0'})
            with urllib.request.urlopen(req, timeout=12) as response:
                data = response.read()
            if len(data) > 0:
                with open(dest, 'wb') as f:
                    f.write(data)
                return item['defid'], True, 'downloaded_large'
        except Exception:
            pass

    return item['defid'], False, 'failed'


done_count = 0
success_count = 0
failed_count = 0
failed_defids = []
start_time = time.time()

with ThreadPoolExecutor(max_workers=35) as executor:
    futures = {executor.submit(download_one, item): item for item in items}
    for future in as_completed(futures):
        defid, success, reason = future.result()
        done_count += 1
        if success:
            success_count += 1
        else:
            failed_count += 1
            failed_defids.append(defid)

        if done_count % 100 == 0 or done_count == total:
            elapsed = time.time() - start_time
            rate = done_count / max(0.1, elapsed)
            percent = (done_count / total) * 100
            print(f"[{percent:5.1f}%] {done_count}/{total} (Success: {success_count}, Fail: {failed_count}) - {rate:.1f} items/s")

elapsed = time.time() - start_time
print(f"\nAll done in {elapsed:.1f}s! Total success: {success_count}, Total failed: {failed_count}")
if failed_defids:
    print("Failed defids:", failed_defids[:50])
