import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

// Derive the repo root from this file's location so the QA also runs on
// CI runners (the old hard-coded local path made it fail with ENOENT).
const ROOT = new URL('../..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const EVIDENCE = join(ROOT, 'scripts/qa/evidence/live-m7-qa');
mkdirSync(EVIDENCE, { recursive: true });
const PORT = 9333;

const shell = spawn(join(ROOT, 'target/debug/dsh-desktop-shell.exe'), [], { stdio: 'ignore' });

async function getPageTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const list = await (await fetch('http://127.0.0.1:' + PORT + '/json/list')).json();
      const page = list.find((t) => t.type === 'page' && !t.url.startsWith('devtools://'));
      if (page) return page;
    } catch {}
    await sleep(500);
  }
  throw new Error('no CDP target');
}

const page = await getPageTarget();
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error('ws')); });
const consoleLogs = [];
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.method === 'Runtime.consoleAPICalled') consoleLogs.push(JSON.stringify(m.params.args || []).slice(0, 200));
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
const send = (method, params = {}) => new Promise((resolve) => { const i = ++id; pending.set(i, resolve); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  return r.result && r.result.result ? r.result.result.value : undefined;
};

const results = [];
const check = (name, ok, detail) => { results.push({ name, ok, detail: detail || '' }); console.log((ok ? 'PASS ' : 'FAIL ') + name + (detail ? ' - ' + detail : '')); };

await sleep(6000);
console.log('console logs: ' + consoleLogs.join(' | ').slice(0, 600));
const app = await evaluate("document.querySelector('.shell-app') ? 'ok' : 'missing'");
check('C1 Shell UI loaded', app === 'ok');

const clicked = await evaluate("(() => { const buttons = [...document.querySelectorAll('button')]; const b = buttons.find(x => (x.textContent || '').toLowerCase().includes('settings')); if (!b) return 'NOT_FOUND'; b.click(); return 'CLICKED'; })()");
check('C2 settings surface opened', clicked === 'CLICKED', clicked);
await sleep(1200);

const wizard = await evaluate("!!document.querySelector('[data-testid=setup-wizard]')");
check('C3 SetupWizard rendered', wizard === true);

const steps = await evaluate("document.querySelectorAll('.setup-wizard__step').length");
check('C4 wizard shows six steps', steps === 6, 'steps=' + steps);

const list = await evaluate("!!document.querySelector('[data-testid=environment-list]')");
check('C5 EnvironmentList rendered', list === true);

const n1 = await evaluate("(() => { const b = document.querySelector('[data-testid=wizard-next]'); if (!b) return 'NO_NEXT'; b.click(); return 'OK'; })()");
await sleep(500);
const harnessInput = await evaluate("!!document.querySelector('[data-testid=harness-path]')");
check('C6 wizard advances to harness step', n1 === 'OK' && harnessInput === true);

const shot = await send('Page.captureScreenshot', { format: 'png' });
if (shot.result && shot.result.data) {
  writeFileSync(join(EVIDENCE, 'settings-wizard.png'), Buffer.from(shot.result.data, 'base64'));
  console.log('screenshot saved');
}

writeFileSync(join(EVIDENCE, 'results.json'), JSON.stringify(results, null, 2));
const passed = results.filter((r) => r.ok).length;
console.log('RESULT: ' + passed + '/' + results.length + ' PASS');
shell.kill();
ws.close();