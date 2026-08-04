#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { JSDOM, VirtualConsole } from 'jsdom';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const html = fs.readFileSync(path.join(ROOT, 'index.html'), 'utf8');
const errors = [];
const vc = new VirtualConsole();
vc.on('error', (...args) => errors.push(args.join(' ')));
const dom = new JSDOM(html, { runScripts: 'dangerously', url: 'http://localhost/', pretendToBeVisual: true, virtualConsole: vc });
const w = dom.window;
w.scrollTo = () => {};
w.HTMLElement.prototype.scrollIntoView = () => {};
for (const file of ['icons.js', 'mock.js', 'store.js', 'ui/atoms.js', 'ui/composites.js', 'ui/overlays.js', 'ui/bundle.js', 'pages.js', 'pages2.js', 'pages3.js', 'lazy-loader.js', 'router.js', 'app.js']) {
  w.eval(fs.readFileSync(path.join(ROOT, 'js', file), 'utf8'));
}
w.Toast.init();
w.Modal.init();

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

w.location.hash = '#/topics/201';
w.Router.render();
assert(w.document.querySelector('.restricted-card'), 'reply-restricted card missing before reply');
const beforeReplies = w.Store.getPost(201).replies;
const reply = w.Store.addReply(201, '交互测试回复');
assert(reply.ok && reply.unlocked, 'reply did not unlock restricted content');
assert(w.Store.getPost(201).replies === beforeReplies + 1, 'reply count did not update');
w.Router.render();
assert(!w.document.querySelector('.restricted-card'), 'restricted card remained after reply');
assert(w.document.querySelector('.restricted-unlocked'), 'restricted body missing after reply');

// Level-gated content must not leak its title to a lower-level member.
w.Store.state.user = JSON.parse(JSON.stringify(w.MockData.users.Alice));
w.location.hash = '#/topics/206';
w.Router.render();
assert(w.document.querySelector('.level-access-card'), 'level access guard missing');
assert(!w.document.querySelector('#app').textContent.includes('从零搭建自托管监控系统'), 'level-gated title leaked to lower-level member');
w.Store.state.user = JSON.parse(JSON.stringify(w.MockData.currentUser));

const beforeCoins = w.Store.state.user.coins;
assert(w.Store.unlockPaid(202, 10), 'paid unlock failed');
assert(w.Store.state.user.coins === beforeCoins - 10, 'paid unlock did not deduct coins');
w.location.hash = '#/topics/202';
w.Router.render();
assert(w.document.querySelector('.restricted-unlocked'), 'paid body missing after unlock');

const reportCount = w.Store.state.reports.length;
w.Store.createReport({ topicId: 201, reason: '其他', detail: '交互测试' });
assert(w.Store.state.reports.length === reportCount + 1, 'report was not added');

const createResult = w.Store.createPost({ type: 'topic', title: '交互测试新主题', board: 'rust', content: '正文', tags: ['Rust'], visibilityLevel: w.Store.state.user.level });
assert(createResult.ok, 'valid post creation failed');
const post = createResult.post;
assert(w.Store.getPost(post.id)?.title === '交互测试新主题', 'created post cannot be read');
w.location.hash = '#/topics/' + post.id;
w.Router.render();
assert(w.document.getElementById('app').textContent.includes('交互测试新主题'), 'created post route did not render');
const rejectedPost = w.Store.createPost({ type: 'topic', title: '不可见测试', board: 'rust', content: '正文', visibilityLevel: w.Store.state.user.level + 1 });
assert(!rejectedPost.ok && rejectedPost.reason === 'visibility_level_exceeds_author', 'visibility level above author was not rejected');

const storage = w.Store.updateStorageConfig({ backend: 's3', endpoint: 'https://s3.example.test', region: 'auto', bucket: 'bblbb-test', accessKeyId: 'test-key', secretAccessKey: 'test-secret' });
assert(storage.secretConfigured, 'S3 secret was not marked configured');
assert(!Object.prototype.hasOwnProperty.call(storage, 'secretAccessKey'), 'S3 secret leaked into state');
assert(w.Store.testStorageConnection(), 'valid S3 configuration did not connect');
const linkPolicy = w.Store.updateStorageConfig({ backend: 's3', signedUrlTtl: 900, maxUploadMb: 128 });
assert(linkPolicy.signedUrlTtl === 900, 'S3 public link TTL configuration was not saved');
const quota = w.Store.updateAttachmentLevelQuota(6, { maxFileMb: 64, totalCapacityMb: 4096 });
assert(quota.maxFileMb === 64 && quota.totalCapacityMb === 4096, 'level attachment quota was not saved');
assert(Object.values(w.Store.state.attachmentLevelQuotas).every(item => item.maxFileMb > 0 && item.totalCapacityMb >= item.maxFileMb), 'level attachment quotas are incomplete');

// UI-level checks: exercise rendered controls instead of only calling Store APIs.
w.location.hash = '#/settings?tab=notifications';
w.Router.render();
const settingSwitch = w.document.querySelector('.settings-content .switch');
assert(settingSwitch, 'notification switch missing');
const switchBefore = settingSwitch.getAttribute('aria-checked');
settingSwitch.click();
assert(settingSwitch.getAttribute('aria-checked') !== switchBefore, 'rendered switch did not toggle');

w.location.hash = '#/search?q=rust&tab=invalid';
w.Router.render();
assert(w.document.querySelector('.tab.is-active')?.textContent.includes('帖子'), 'invalid search tab did not fall back to posts');
assert(w.document.querySelector('.search-submit'), 'search submit control missing');

w.location.hash = '#/users/Chaos?tab=points';
w.Router.render();
assert(w.document.querySelector('.points-page'), 'points tab rendered without content');

w.location.hash = '#/users/Chaos?tab=replies';
w.Router.render();
const replyHref = w.document.querySelector('.simple-row a')?.getAttribute('href') || '';
assert(/#\/topics\/\d+\?reply=\d+/.test(replyHref), 'profile reply link is not router-safe');

// Article cover is visible and mandatory only for article publishing.
w.location.hash = '#/publish?type=article&title=封面校验&board=rust&content=正文';
w.Router.render();
assert(w.document.querySelector('#article-cover-input'), 'article cover input missing');
const modalCountBeforeCover = w.document.querySelectorAll('.modal-overlay').length;
w.submitPublish('published');
assert(w.document.querySelectorAll('.modal-overlay').length === modalCountBeforeCover, 'article without cover reached confirmation');
assert(w.document.getElementById('toast-container')?.textContent.includes('必须设置封面图'), 'article cover validation warning missing');

w.location.hash = '#/publish?type=topic';
w.Router.render();
assert(!w.document.querySelector('#article-cover-input'), 'topic unexpectedly requires an article cover');

w.Store.setTheme('light');
assert(w.document.documentElement.classList.contains('light') && !w.document.documentElement.classList.contains('dark'), 'light theme class was not applied');
w.Store.setTheme('dark');
assert(w.document.documentElement.classList.contains('dark') && !w.document.documentElement.classList.contains('light'), 'dark theme class was not applied');

const adminRoutes = ['/admin/users','/admin/roles','/admin/boards','/admin/posts','/admin/reports','/admin/tags','/admin/points','/admin/shop','/admin/activity','/admin/levels','/admin/attachments','/admin/download-billing','/admin/ai','/admin/video','/admin/storage','/admin/notifications','/admin/themes','/admin/plugins','/admin/oauth','/admin/marketplace','/admin/audit','/admin/settings'];
for (const route of adminRoutes) {
  w.location.hash = '#' + route;
  w.Router.render();
  assert(w.document.querySelector('.admin-layout'), `admin route failed: ${route}`);
}

if (errors.length) throw new Error('console errors: ' + errors.join('; '));
console.log('interaction checks: passed');
console.log('admin routes checked:', adminRoutes.length);
w.location.hash = '#/shop'; w.Router.render(); assert(w.document.body.textContent.includes('积分商城'), 'shop route failed');
w.buyShopProduct('shop-nickname-blue');
w.location.hash = '#/activity'; w.Router.render(); assert(w.document.body.textContent.includes('社区活跃'), 'activity route failed');
console.log('core flows checked: reply unlock, paid unlock, report, publish, S3 storage, switches, tabs, points, shop, activity, reply links, themes');
