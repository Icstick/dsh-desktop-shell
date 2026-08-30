// M4-B PoC B: 受管 Edge + CDP 最小验证链 (零依赖, Node 24 原生 WebSocket/fetch)
// 验证点（POC-M4B.md）: P1 profile 隔离(唯一 user-data-dir) / P2 navigate / P3 文本快照 / P4 截图 / P5 进程树清理
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, readFileSync, existsSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const EVIDENCE = new URL('../evidence/poc-b/', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
mkdirSync(EVIDENCE, { recursive: true });

let child;
const profileDirs = [];
const cleanup = (msg) => {
  if (child && child.exitCode === null) {
    try { execFileSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' }); console.log('[kill] tree terminated'); }
    catch { console.log('[kill] already gone'); }
  }
  // 等待浏览器进程树退出释放文件句柄，再删 profile（失败重试一次）
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 800);
  for (const dir of profileDirs) {
    try { rmSync(dir, { recursive: true, force: true }); }
    catch { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 800); try { rmSync(dir, { recursive: true, force: true }); } catch { /* 仍锁定，tmp 可回收 */ } }
  }
  if (msg) console.log(msg);
  process.exit(0);
};

// 1. 唯一 user-data-dir（复用会导致 Edge 进程转发，必须唯一）
const profileDir = mkdtempSync(join(tmpdir(), 'dsh-pocb-'));
profileDirs.push(profileDir);
console.log('[profile]', profileDir);

// 2. spawn Edge
child = spawn(EDGE, [
  '--remote-debugging-port=0',
  '--user-data-dir=' + profileDir,
  '--no-first-run',
  '--no-default-browser-check',
  '--remote-allow-origins=*',
  '--disable-features=msEdgeStartupBoost',
  'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
let stderr = '';
child.stderr.on('data', (d) => { stderr += d; });
child.on('exit', (code) => { if (code !== 0) console.log('[edge exited early]', code); });

// 3. DevToolsActivePort 轮询（≤10s）
const portFile = join(profileDir, 'DevToolsActivePort');
let port = null;
for (let i = 0; i < 100 && !port; i++) {
  if (existsSync(portFile)) {
    const lines = readFileSync(portFile, 'utf8').trim().split('\n');
    port = parseInt(lines[0], 10);
  }
  if (!port) await sleep(100);
}
if (!port) { console.error('[fatal] DevToolsActivePort not found; stderr tail:', stderr.slice(-300)); cleanup(); }
console.log('[debug-port]', port);

// 4. /json/list 找 page target
const targets = await (await fetch('http://127.0.0.1:' + port + '/json/list')).json();
const page = targets.find((t) => t.type === 'page');
console.log('[target]', page ? page.url : 'none', page ? page.webSocketDebuggerUrl : '');
if (!page) { console.error('[fatal] no page target'); cleanup(); }

// 5. WS 连接 + id 匹配
const ws = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
};
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const send = (method, params = {}) => new Promise((resolve) => {
  const id = nextId++;
  pending.set(id, resolve);
  ws.send(JSON.stringify({ id, method, params }));
});

// 6. navigate + readyState 轮询
await send('Page.enable');
await send('Page.navigate', { url: 'https://example.com' });
let ready = '';
for (let i = 0; i < 50; i++) {
  const r = await send('Runtime.evaluate', { expression: 'document.readyState', returnByValue: true });
  ready = r.result && r.result.result ? r.result.result.value : '';
  if (ready === 'complete') break;
  await sleep(200);
}
console.log('[readyState]', ready);

// 7. 文本快照（P3）
const txt = await send('Runtime.evaluate', {
  expression: 'JSON.stringify({t: document.title, n: document.documentElement.innerText.length})',
  returnByValue: true,
});
console.log('[snapshot]', txt.result && txt.result.result ? txt.result.result.value : '?');

// 8. 截图（P4）
const shot = await send('Page.captureScreenshot', { format: 'png' });
if (shot.result && shot.result.data) {
  const buf = Buffer.from(shot.result.data, 'base64');
  const shotPath = join(EVIDENCE, 'pocb-screenshot.png');
  writeFileSync(shotPath, buf);
  console.log('[screenshot]', buf.length, 'bytes ->', shotPath);
} else {
  console.log('[screenshot] FAILED', JSON.stringify(shot).slice(0, 200));
}

// 9. 进程树清理（P5）
cleanup('[done] profile cleanup attempted: ' + profileDir);
