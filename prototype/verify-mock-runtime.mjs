#!/usr/bin/env node
import { chromium } from '/data/projects/bblbb/frontend/node_modules/playwright/index.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
const base=(process.env.PROTOTYPE_BASE||'http://127.0.0.1:8765').replace(/\/$/,'');
const out=join('/data/projects/bblbb/prototype/.verify','mock-'+new Date().toISOString().replace(/[:T]/g,'-').slice(0,19));mkdirSync(out,{recursive:true});
const results=[];const errors=[];const add=(name,pass,detail={})=>results.push({name,pass:!!pass,...detail});
const b=await chromium.launch();const p=await b.newPage({viewport:{width:1440,height:900}});p.on('pageerror',e=>errors.push(e.message));p.on('console',m=>{if(m.type()==='error')errors.push(m.text())});
const wait=(ms=80)=>p.waitForTimeout(ms);const go=async(h)=>{await p.evaluate(x=>location.hash=x,h);await wait();};const root=()=>p.locator('#mock-workflow-root');
try{
 await p.goto(base+'/',{waitUntil:'domcontentloaded'});await p.evaluate(()=>{localStorage.clear();sessionStorage.clear();});await go('#login');
 add('login screen',await root().locator('h1').innerText()==='登录');await root().locator('[data-login-user]').fill('admin');await root().locator('[data-login-password]').fill('admin123');await root().locator('[data-login]').click();await wait();
 add('login state persisted',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).auth===true));
 await go('#publish:article');await root().locator('[data-title]').fill('验收发布内容');await root().locator('[data-board]').selectOption({label:'Rust'});await root().locator('input.mock-input[data-tags]').fill('Rust,验收');await root().locator('[data-body]').fill('用于验证 Mock 发布、草稿和详情页流程的正文。');await root().locator('[data-save-draft]').click();await wait();
 add('draft saved',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).draft.title==='验收发布内容'));
 await root().locator('[data-publish]').click();await wait();add('publish opens detail',/topic:p/.test(p.url())&&await root().locator('h1').innerText()==='验收发布内容');
 await go('#topic:202');await root().locator('[data-paid-unlock]').click();await wait();await p.locator('[data-confirm-paid]').click();await wait();add('paid unlock persisted',await p.evaluate(()=>{const s=JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1'));return s.unlocked['202']&&s.coins===318;}));
 await go('#topic:201');await root().locator('[data-comment]').fill('验证回复解锁。');await root().locator('[data-comment-submit]').click();await wait(500);add('reply unlock persisted',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).unlocked['201']===true));
 await go('#notifications');await root().locator('[data-read-all]').click();await wait();add('notifications read persisted',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).notifications.every(n=>!n.unread)));
 await go('#messages');await root().locator('[data-message-fail]').click();await root().locator('[data-chat-input]').fill('失败测试');await root().locator('[data-send-message]').click();await wait();add('message failure saved',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).messages.Lin.some(m=>m.failed)));
 await go('#mfa');await root().locator('[data-mfa-code]').fill('123456');await root().locator('[data-enable-mfa]').click();await wait();add('mfa state persisted',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).mfa===true));
 await go('#admin-reports');await root().locator('[data-select-all]').check();await root().locator('[data-bulk-close]').click();await wait();add('admin bulk report closure',await p.evaluate(()=>Object.values(JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).reports).every(r=>r.status==='resolved')));
 await go('#admin-points');await root().locator('[data-adjust-amount]').fill('15');await root().locator('[data-adjust-reason]').fill('验收调整');await root().locator('[data-adjust]').click();await wait();await p.locator('[data-final-adjust]').click();await wait();add('admin adjustment ledger',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).ledger.some(x=>x.kind==='admin_adjust'&&x.amount===15)));
 await go('#admin-ai');await root().locator('[data-ai-new]').click();await wait(750);add('ai task lifecycle',await p.evaluate(()=>JSON.parse(localStorage.getItem('bblbb:mock-runtime:v1')).aiTasks.some(x=>x.status==='completed')));
 for(const h of ['#articles','#boards','#tags','#billing','#shop','#achievements','#appeals','#checkout','#purchases','#apikeys','#admin-themes','#admin-plugins','#admin-video','#admin-storage','#admin-marketplace','#admin-audit','#404']){await go(h);add('route '+h,(await root().locator('h1').count()>0)||(h==='#404'&&await root().locator('.mock-empty').count()>0),{hash:p.url().split('#')[1]});}
}finally{const report={base,results,errors};writeFileSync(join(out,'report.json'),JSON.stringify(report,null,2));console.log('RESULT '+results.filter(x=>x.pass).length+'/'+results.length+' failed '+results.filter(x=>!x.pass).length+' errors '+errors.length+' report '+join(out,'report.json'));await b.close();}
if(results.some(x=>!x.pass)||errors.length)process.exitCode=1;
