#!/usr/bin/env python3
"""
下载 Steam 点数商店全量动态迷你个人资料背景 (Class 13 - cluster/2)
"""
import urllib.request
import json
import os
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_DIR = os.path.join(BASE_DIR, 'uploads', 'steam-assets', 'backgrounds')
JSON_PATH = os.path.join(BASE_DIR, 'frontend', 'src', 'lib', 'data', 'steam-profile-backgrounds.json')

os.makedirs(OUT_DIR, exist_ok=True)
os.makedirs(os.path.dirname(JSON_PATH), exist_ok=True)

print("Fetching Steam Mini-Profile Backgrounds definition list...")
cursor = ''
all_bg = []
seen_defids = set()

for page in range(30):
    url = f'https://api.steampowered.com/ILoyaltyRewardsService/QueryRewardItems/v1/?count=500&community_item_classes[0]=13&cursor={cursor}'
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    try:
        data = json.loads(urllib.request.urlopen(req, timeout=15).read())
    except Exception as e:
        print(f"Error fetching page {page}: {e}")
        break

    defs = data.get('response', {}).get('definitions', [])
    if not defs: break
    for item in defs:
        defid = item.get('defid')
        if defid in seen_defids: continue
        seen_defids.add(defid)

        c = item.get('community_item_data', {})
        image_name = c.get('item_image_large') or c.get('item_image_small')
        if not image_name: continue

        all_bg.append({
            'id': f'steam_bg_{defid}',
            'defid': defid,
            'appid': item.get('appid'),
            'name': c.get('item_title') or c.get('item_name') or '未命名背景',
            'image': image_name,
            'webm': c.get('item_movie_webm'),
            'mp4': c.get('item_movie_mp4'),
            'animated': bool(c.get('animated')),
            'cost': int(item.get('point_cost', '2000'))
        })
    cursor = data.get('response', {}).get('next_cursor', '')
    if not cursor: break

print(f"Found {len(all_bg)} backgrounds. Saving JSON index to {JSON_PATH}...")
with open(JSON_PATH, 'w', encoding='utf-8') as f:
    json.dump(all_bg, f, ensure_ascii=False, separators=(',', ':'))

print("Starting concurrent download of panorama posters and dynamic media...")


def download_one(task):
    item, field = task
    fname = item.get(field)
    if not fname:
        return task, True
    dest = os.path.join(OUT_DIR, fname)
    if os.path.exists(dest) and os.path.getsize(dest) > 1000:
        return task, True

    url = f"https://shared.fastly.steamstatic.com/community_assets/images/items/{item['appid']}/{fname}"
    for _ in range(3):
        try:
            req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
            data = urllib.request.urlopen(req, timeout=15).read()
            if len(data) > 0:
                with open(dest, 'wb') as f:
                    f.write(data)
                return task, True
        except Exception:
            time.sleep(0.5)
    return task, False

# Keep the existing 1000 static posters and also pull every catalogued WebM/MP4
# locally. The UI can then render the selected panorama without a runtime CDN
# dependency; the catalog still retains CDN metadata for recovery/debugging.
tasks = [(item, field) for item in all_bg for field in ('image', 'webm', 'mp4') if item.get(field)]
total = len(tasks)
done = 0
success = 0
start_time = time.time()

with ThreadPoolExecutor(max_workers=35) as executor:
    futures = {executor.submit(download_one, task): task for task in tasks}
    for future in as_completed(futures):
        _task, ok = future.result()
        done += 1
        if ok:
            success += 1
        if done % 100 == 0 or done == total:
            elapsed = time.time() - start_time
            rate = done / max(0.1, elapsed)
            print(f"[{done*100/total:5.1f}%] {done}/{total} (Success: {success}) - {rate:.1f} assets/s")

elapsed = time.time() - start_time
print(f"Panorama asset download complete in {elapsed:.1f}s! Total: {success}/{total}")
