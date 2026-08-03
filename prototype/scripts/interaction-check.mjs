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
for (const file of ['icons.js', 'mock.js', 'store.js', 'ui/atoms.js', 'ui/composites.js', 'ui/overlays.js', 'ui/bundle.js', 'pages.js', 'pages2.js', 'pages3.js', 'router.js', 'app.js']) {
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

const beforeCoins = w.Store.state.user.coins;
assert(w.Store.unlockPaid(202, 10), 'paid unlock failed');
assert(w.Store.state.user.coins === beforeCoins - 10, 'paid unlock did not deduct coins');
w.location.hash = '#/topics/202';
w.Router.render();
assert(w.document.querySelector('.restricted-unlocked'), 'paid body missing after unlock');

const reportCount = w.Store.state.reports.length;
w.Store.createReport({ topicId: 201, reason: '其他', detail: '交互测试' });
assert(w.Store.state.reports.length === reportCount + 1, 'report was not added');

const post = w.Store.createPost({ type: 'topic', title: '交互测试新主题', board: 'rust', content: '正文', tags: ['Rust'] });
assert(w.Store.getPost(post.id)?.title === '交互测试新主题', 'created post cannot be read');
w.location.hash = '#/topics/' + post.id;
w.Router.render();
assert(w.document.getElementById('app').textContent.includes('交互测试新主题'), 'created post route did not render');

const storage = w.Store.updateStorageConfig({ backend: 's3', endpoint: 'https://s3.example.test', region: 'auto', bucket: 'bblbb-test', accessKeyId: 'test-key', secretAccessKey: 'test-secret' });
assert(storage.secretConfigured, 'S3 secret was not marked configured');
assert(!Object.prototype.hasOwnProperty.call(storage, 'secretAccessKey'), 'S3 secret leaked into state');
assert(w.Store.testStorageConnection(), 'valid S3 configuration did not connect');
const retention = w.Store.updateStorageConfig({ backend: 's3', defaultAttachmentTtlDays: 30, maxAttachmentTtlDays: 365, maxUploadMb: 128 });
assert(retention.defaultAttachmentTtlDays === 30 && retention.maxAttachmentTtlDays === 365, 'attachment retention configuration was not saved');
assert(w.MockData.levels.every(level => level.attachmentMaxMb > 0 && level.attachmentTotalMb >= level.attachmentMaxMb && level.attachmentTtlDays > 0), 'level attachment quotas are incomplete');

const adminRoutes = ['/admin/users','/admin/roles','/admin/boards','/admin/posts','/admin/reports','/admin/tags','/admin/points','/admin/levels','/admin/attachments','/admin/storage','/admin/notifications','/admin/themes','/admin/plugins','/admin/oauth','/admin/audit','/admin/settings'];
for (const route of adminRoutes) {
  w.location.hash = '#' + route;
  w.Router.render();
  assert(w.document.querySelector('.admin-layout'), `admin route failed: ${route}`);
}

if (errors.length) throw new Error('console errors: ' + errors.join('; '));
console.log('interaction checks: passed');
console.log('admin routes checked:', adminRoutes.length);
console.log('core flows checked: reply unlock, paid unlock, report, publish, S3 storage');
