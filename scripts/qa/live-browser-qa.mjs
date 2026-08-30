// M4-D live desktop QA: CDP 驱动 shell UI 验证 Shared Browser（零依赖 Node 24）
// 前置：tauri.conf.json 的 shell window 配 additionalBrowserArgs="--remote-debugging-port=9333"（QA 专用，验证后还原）
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

const PORT = 9333;
const EVIDENCE = new URL('./evidence/live-qa/', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
mkdirSync(EVIDENCE, { recursive: true });
const out = [];
const log = (s) => { out.push(s); console.log(s); };

// ---- CDP 客户端（复用 PoC B 模式）----
async function getPageTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const list = await (await fetch('http://127.0.0.1:' + PORT + '/json/list')).json();
      const page = list.find((t) => t.type === 'page' && !t.url.startsWith('devtools://'));
      if (page) return page;
    } catch { /* not up yet */ }
    await sleep(500);
  }
  throw new Error('no CDP target after 30s');
}

async function connect(page) {
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  let id = 0; const pending = new Map();
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  };
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error('ws fail')); });
  const send = (method, params = {}) => new Promise((resolve) => {
    const i = ++id; pending.set(i, resolve);
    ws.send(JSON.stringify({ id: i, method, params }));
  });
  const evaluate = async (expression) => {
    const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (r.result && r.result.exceptionDetails) throw new Error('eval exception: ' + JSON.stringify(r.result.exceptionDetails).slice(0, 300));
    return r.result && r.result.result ? r.result.result.value : undefined;
  };
  return { send, evaluate, ws };
}

// ---- 主流程 ----
const page = await getPageTarget();
log('[target] ' + page.url);
const cdp = await connect(page);
log('[cdp] connected');

// 1. UI 结构 dump（rail 按钮）
const buttons = await cdp.evaluate(`JSON.stringify([...document.querySelectorAll('button')].map(b => ({t: (b.textContent||'').trim().slice(0,30), title: b.title, aria: b.getAttribute('aria-label')})))`);
log('[buttons] ' + buttons);

// 2. 点击 rail 的 Browser 入口
const clicked = await cdp.evaluate(`(() => {
  const btns = [...document.querySelectorAll('button')];
  const b = btns.find(x => (x.textContent||'').includes('Browser') || (x.getAttribute('aria-label')||'').includes('Browser'));
  if (!b) return 'NOT_FOUND';
  b.click(); return 'CLICKED';
})()`);
log('[rail-click] ' + clicked);
await sleep(1500);

// 3. 找 URL 输入框 + 输入
const input = await cdp.evaluate(`(() => {
  const els = [...document.querySelectorAll('input')];
  const i = els.find(x => (x.placeholder||'').includes('http') || x.type === 'url' || (x.placeholder||'').toLowerCase().includes('url'));
  if (!i) return 'NO_INPUT';
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(i, 'https://example.com');
  i.dispatchEvent(new Event('input', { bubbles: true }));
  return 'SET:' + i.placeholder;
})()`);
log('[url-input] ' + input);
await sleep(300);

// 4. 点 Open（面板主按钮）
const open = await cdp.evaluate(`(() => {
  const btns = [...document.querySelectorAll('button')];
  const b = btns.find(x => (x.textContent||'').trim() === 'Open');
  if (!b) return 'NO_OPEN';
  b.click(); return 'CLICKED';
})()`);
log('[open-click] ' + open);
await sleep(4000);

// 5. 读面板状态（session/state/url 显示）
const status = await cdp.evaluate(`document.body.innerText.replace(/\\n+/g,' | ').slice(0, 600)`);
log('[panel-status] ' + status);

// 6. API 级：snapshot + 非法导航拒绝
const snap = await cdp.evaluate(`(async () => {
  const sid = (document.body.innerText.match(new RegExp('brw-[a-z0-9-]+')) || [''])[0];
  if (!sid) return 'NO_SESSION';
  try {
    const r = await window.__TAURI_INTERNALS__.invoke('snapshot_browser', { request: { schemaVersion: 1, sessionId: sid, snapshotMode: 'text' } });
    return 'SNAP_OK:' + JSON.stringify(r).slice(0, 150);
  } catch (e) { return 'SNAP_ERR:' + String(e); }
})()`);
log('[snapshot] ' + snap);

const badNav = await cdp.evaluate(`(async () => {
  const sid = (document.body.innerText.match(new RegExp('brw-[a-z0-9-]+')) || [''])[0];
  try {
    await window.__TAURI_INTERNALS__.invoke('navigate_browser', { request: { schemaVersion: 1, sessionId: sid, url: 'file:///C:/Windows/win.ini' } });
    return 'BAD_NAV_ALLOWED!';
  } catch (e) { return 'BAD_NAV_REJECTED:' + String(e).slice(0, 100); }
})()`);
log('[bad-nav] ' + badNav);

// 7. 验证 profile 目录
const appData = process.env.APPDATA + '\\dev.dsh.desktop-shell\\browser-profiles';
let profiles = [];
if (existsSync(appData)) profiles = readdirSync(appData);
log('[profiles] ' + JSON.stringify(profiles));

// 8. 窗口验证（新 browser 窗口）
try {
  const ps = execFileSync('powershell.exe', ['-NoProfile', '-Command', "Get-Process dsh-desktop-shell | ForEach-Object { $_.MainWindowTitle }"]).toString().trim().split('\n').filter(Boolean);
  log('[windows] ' + JSON.stringify(ps));
} catch (e) { log('[windows] err ' + e.message); }

// 9. close
const close = await cdp.evaluate(`(async () => {
  const sid = (document.body.innerText.match(new RegExp('brw-[a-z0-9-]+')) || [''])[0];
  if (!sid) return 'NO_SESSION';
  try { await window.__TAURI_INTERNALS__.invoke('close_browser', { request: { schemaVersion: 1, sessionId: sid } }); return 'CLOSED'; }
  catch (e) { return 'CLOSE_ERR:' + String(e).slice(0, 100); }
})()`);
log('[close] ' + close);
await sleep(1500);

writeFileSync(join(EVIDENCE, 'live-qa-output.txt'), out.join('\n'));
log('[evidence] ' + EVIDENCE + 'live-qa-output.txt');
cdp.ws.close();
process.exit(0);