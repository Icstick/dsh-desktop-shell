// M6 live QA: daemon 化端到端验证（零依赖 Node 24）
// 阶段 A: daemon 直连（spawn dsh-desktop-daemon.exe → claim 端口 → 凭证文件 → envelope 握手/协商/调用/事件）
// 阶段 B: Shell 端到端（启动 Shell → daemon 存活保持 → 关闭 Shell daemon 不退出 → 重启 Shell 重连）
// 前置: 已构建 target/debug/dsh-desktop-daemon.exe 与 dsh-desktop-shell.exe
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import net from 'node:net';

const ROOT = new URL('../..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const DAEMON_EXE = join(ROOT, 'target', 'debug', 'dsh-desktop-daemon.exe');
const SHELL_EXE = join(ROOT, 'target', 'debug', 'dsh-desktop-shell.exe');
const EVIDENCE = join(ROOT, 'scripts', 'qa', 'evidence', 'live-daemon-qa');
const QA_DATA = join(ROOT, 'scripts', 'qa', 'evidence', 'qa-data');
const CLAIM_PORT = 37771;
const CRED_FILE = join(QA_DATA, 'daemon-credential.json');

mkdirSync(EVIDENCE, { recursive: true });
rmSync(QA_DATA, { recursive: true, force: true });
mkdirSync(QA_DATA, { recursive: true });

const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok, detail }); console.log((ok ? 'PASS' : 'FAIL') + ' ' + name + (detail ? ' — ' + detail : '')); };
const failFast = (name, e) => { check(name, false, String(e)); writeFileSync(join(EVIDENCE, 'results.json'), JSON.stringify(results, null, 2)); process.exit(1); };

const env = { ...process.env, DSH_DAEMON_DATA_DIR: QA_DATA };

let daemonProc = null;
let daemonPid = null;
let shellProc = null;
let cred = null;

async function waitPort(port, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const ok = await new Promise((res) => {
      const s = net.connect({ host: '127.0.0.1', port }, () => { s.destroy(); res(true); });
      s.on('error', () => res(false));
    });
    if (ok) return true;
    await sleep(200);
  }
  return false;
}

class EnvelopeClient {
  // 裸 socket 风格（debug-handshake2 验证可行）：每个连接一个专用解析循环，
  // 帧 = u32 LE 长度前缀 + JSON；Result 按 correlationId 关联，Event 入 events。
  static async connect(port, token) {
    const socket = net.connect({ host: '127.0.0.1', port });
    await new Promise((res, rej) => { socket.once('connect', res); socket.once('error', rej); });
    const client = new EnvelopeClient(socket);
    client.write({ token });
    const hello = await client.readFrame(5000);
    if (!hello.accepted) throw new Error('handshake rejected: ' + JSON.stringify(hello));
    return client;
  }
  constructor(socket) {
    this.socket = socket;
    this.buf = Buffer.alloc(0);
    this.pending = new Map();
    this.events = [];
    this.inbox = [];
    this.waiters = [];
    socket.on('data', (d) => { this.buf = Buffer.concat([this.buf, d]); this.pump(); });
    socket.on('error', () => {});
  }
  write(obj) {
    const payload = Buffer.from(JSON.stringify(obj));
    const frame = Buffer.alloc(4 + payload.length);
    frame.writeUInt32LE(payload.length, 0);
    payload.copy(frame, 4);
    this.socket.write(frame);
  }
  pump() {
    while (this.buf.length >= 4) {
      const len = this.buf.readUInt32LE(0);
      if (this.buf.length < 4 + len) break;
      const msg = JSON.parse(this.buf.subarray(4, 4 + len).toString());
      this.buf = this.buf.subarray(4 + len);
      const key = msg.correlationId || msg.replyTo;
      if (msg.kind === 'Result' && key && this.pending.has(key)) {
        const resolve = this.pending.get(key);
        this.pending.delete(key);
        clearTimeout(resolve.timer);
        resolve.fn(msg);
      } else if (msg.kind === 'Event') {
        this.events.push(msg);
      } else {
        this.pushFrame(msg);
      }
    }
  }
  pushFrame(msg) {
    const waiter = this.waiters.shift();
    if (waiter) { clearTimeout(waiter.timer); waiter.resolve(msg); }
    else this.inbox.push(msg);
  }
  readFrame(timeout) {
    if (this.inbox.length) return Promise.resolve(this.inbox.shift());
    return new Promise((resolve, reject) => {
      const waiter = {
        resolve, reject,
        timer: setTimeout(() => {
          this.waiters = this.waiters.filter((w) => w !== waiter);
          reject(new Error('frame timeout'));
        }, timeout),
      };
      this.waiters.push(waiter);
    });
  }
  async negotiate(supports, instanceId) {
    const envelope = (kind, extra = {}) => ({
      protocol: 'interop.dsh-desktop.local/v1alpha1',
      id: 'qa-' + Math.random().toString(36).slice(2, 12),
      kind,
      participant: { component: 'qa-harness', facet: 'live-qa' },
      timestamp: new Date().toISOString(),
      generation: 0,
      ...extra,
    });
    this.write(envelope('Hello', { payload: { instanceId, supports, requires: [] } }));
    const agreement = await this.readFrame(5000);
    if (agreement.kind !== 'Agreement') throw new Error('expected Agreement, got ' + agreement.kind + ' -> ' + JSON.stringify(agreement).slice(0, 200));
    this.activationId = agreement.payload && agreement.payload.activationId;
    return agreement;
  }
  async invoke(capability, method, payload = {}) {
    const correlationId = 'qa-' + Math.random().toString(36).slice(2, 12);
    this.write({
      protocol: 'interop.dsh-desktop.local/v1alpha1',
      id: correlationId,
      kind: 'Invocation',
      participant: { component: 'qa-harness', facet: 'live-qa', ...(this.activationId ? { activationId: this.activationId } : {}) },
      timestamp: new Date().toISOString(),
      generation: 0,
      capability,
      method,
      payload,
    });
    return await new Promise((res, rej) => {
      const timer = setTimeout(() => rej(new Error('invoke timeout: ' + method)), 15000);
      this.pending.set(correlationId, { fn: res, timer });
    });
  }
  close() { this.socket.destroy(); }
}

const CAP = {
  terminal: { apiVersion: 'terminal.dsh-desktop.local/v1alpha1', kind: 'Terminal' },
  browser: { apiVersion: 'browser.dsh-desktop.local/v1alpha1', kind: 'Browser' },
  runtime: { apiVersion: 'runtime.dsh-desktop.local/v1alpha1', kind: 'Runtime' },
  daemon: { apiVersion: 'daemon.dsh-desktop.local/v1alpha1', kind: 'Daemon' },
  scheduler: { apiVersion: 'scheduler.dsh-desktop.local/v1alpha1', kind: 'Scheduler' },
  system: { apiVersion: 'system.dsh-desktop.local/v1alpha1', kind: 'System' },
};
const supports = Object.values(CAP);

async function phaseA() {
  console.log('\n===== Phase A: daemon direct =====');
  if (!existsSync(DAEMON_EXE)) throw new Error('daemon exe missing: ' + DAEMON_EXE);
  daemonProc = spawn(DAEMON_EXE, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  daemonPid = daemonProc.pid;
  daemonProc.stdout.on('data', () => {});
  daemonProc.stderr.on('data', () => {});

  const claimUp = await waitPort(CLAIM_PORT);
  check('A1 claim port 37771 reachable', claimUp);

  for (let i = 0; i < 100 && !cred; i++) {
    if (existsSync(CRED_FILE)) {
      try { cred = JSON.parse(readFileSync(CRED_FILE, 'utf8')); } catch { /* partial write */ }
    }
    if (!cred) await sleep(100);
  }
  check('A2 credential file written', !!cred, cred ? 'pid=' + cred.pid + ' port=' + cred.port : '');
  if (!cred) throw new Error('no credential file');
  check('A3 credential schema v1 + pid match', cred.schemaVersion === 1 && cred.pid === daemonPid, 'schema=' + cred.schemaVersion + ' pid=' + cred.pid + '/' + daemonPid);

  let client;
  try {
    client = await EnvelopeClient.connect(cred.port, cred.credential.token);
    check('A4 envelope handshake (one-time credential)', true);
  } catch (e) { failFast('A4 envelope handshake (one-time credential)', e); }

  const agreement = await client.negotiate(supports, 'qa-instance-0001');
  const granted = agreement.payload.granted.map((c) => c.kind).sort();
  check('A5 negotiation grants six capabilities', JSON.stringify(granted) === JSON.stringify(['Browser', 'Daemon', 'Runtime', 'Scheduler', 'System', 'Terminal']), granted.join(','));

  try {
    await EnvelopeClient.connect(cred.port, cred.credential.token);
    failFast('A6 credential one-time (replay rejected)', new Error('second handshake was accepted'));
  } catch (e) {
    check('A6 credential one-time (replay rejected)', true, String(e.message || e).slice(0, 60));
  }

  const ping = await client.invoke(CAP.system, 'ping');
  check('A7 system.ping', ping.kind === 'Result' && ping.error === undefined, JSON.stringify(ping.payload ?? ping.error ?? ping).slice(0, 60));

  const dstat = await client.invoke(CAP.daemon, 'status');
  check('A8 daemon.status', dstat.kind === 'Result' && dstat.error === undefined, JSON.stringify(dstat.payload ?? dstat.error ?? dstat).slice(0, 80));

  const tstat = await client.invoke(CAP.terminal, 'terminal.status');
  check('A9 terminal.status empty view', tstat.kind === 'Result' && tstat.error === undefined, JSON.stringify(tstat.payload ?? tstat.error ?? tstat).slice(0, 80));

  const created = await client.invoke(CAP.terminal, 'terminal.create', {
    schemaVersion: 1,
    mode: 'human_surface',
    cols: 80,
    rows: 24,
  });
  const sessionId = created.payload && created.payload.sessionId;
  check('A10 terminal.create real PTY', !!sessionId, sessionId ? 'session=' + sessionId : JSON.stringify(created).slice(0, 120));
  if (!sessionId) failFast('A10 terminal.create real PTY', new Error('no session id'));

  const afterCreate = await client.invoke(CAP.terminal, 'terminal.status');
  const sessions = afterCreate.payload && afterCreate.payload.sessions;
  check('A11 terminal.status contains session', Array.isArray(sessions) && sessions.some((s) => s.sessionId === sessionId), 'count=' + (Array.isArray(sessions) ? sessions.length : '?'));

  await client.invoke(CAP.terminal, 'terminal.write', { schemaVersion: 1, sessionId, data: 'echo LIVE_QA_OK\r' });
  let outputText = '';
  for (let i = 0; i < 50 && !outputText.includes('LIVE_QA_OK'); i++) {
    const hit = client.events.find((e) => e.payload && e.payload.sessionId === sessionId && JSON.stringify(e.payload).includes('LIVE_QA_OK'));
    if (hit) outputText = JSON.stringify(hit.payload);
    else await sleep(100);
  }
  check('A12 terminal.output event bridge', outputText.includes('LIVE_QA_OK'), outputText.slice(0, 100));

  const closed = await client.invoke(CAP.terminal, 'terminal.close', { schemaVersion: 1, sessionId });
  check('A13 terminal.close', closed.kind === 'Result' && closed.error === undefined, '');

  const bcreate = await client.invoke(CAP.browser, 'browser.create', { schemaVersion: 1, mode: 'human_surface' });
  const bsid = bcreate.payload && bcreate.payload.sessionId;
  check('A14 browser.create', !!bsid, bsid ? 'session=' + bsid : JSON.stringify(bcreate).slice(0, 80));
  if (bsid) {
    await sleep(300);
    const blist = await client.invoke(CAP.browser, 'browser.list');
    check('A15 browser.list contains session', JSON.stringify(blist.payload).includes(bsid), JSON.stringify(blist.payload ?? blist.error ?? blist).slice(0, 120));
    await client.invoke(CAP.browser, 'browser.close', { schemaVersion: 1, sessionId: bsid });
    await sleep(300);
    const closedEvent = client.events.some((e) => e.payload && e.payload.sessionId === bsid && e.payload.kind === 'closed');
    check('A16 browser.session-closed event', closedEvent, '');
  }

  const rstat = await client.invoke(CAP.runtime, 'runtime.status', { schemaVersion: 1, environmentId: 'no-such-env' });
  check('A17 runtime.status unknown env UNAVAILABLE', rstat.kind === 'Result' && rstat.error && rstat.error.code === 'UNAVAILABLE', JSON.stringify(rstat.error ?? rstat).slice(0, 120));

  const wake = await client.invoke(CAP.scheduler, 'wake', { wakeId: 'qa-wake-1', requestedAt: new Date().toISOString(), reason: 'user_requested' });
  check('A18 scheduler.wake', wake.kind === 'Result' && wake.error === undefined, JSON.stringify(wake.payload ?? wake.error ?? wake).slice(0, 400));

  client.close();
}

async function phaseB() {
  console.log('\n===== Phase B: Shell end-to-end =====');
  if (!existsSync(SHELL_EXE)) throw new Error('shell exe missing: ' + SHELL_EXE);

  const alive = (pid) => new Promise((res) => { try { process.kill(pid, 0); res(true); } catch { res(false); } });
  const shellStderr = [];
  const spawnShell = () => {
    const proc = spawn(SHELL_EXE, [], { env, stdio: ['ignore', 'ignore', 'pipe'] });
    proc.stderr.on('data', (d) => shellStderr.push(d.toString()));
    return proc;
  };

  shellProc = spawnShell();
  await sleep(4000);
  check('B1 daemon alive after Shell start', await alive(daemonPid));
  check('B2 Shell process alive', await alive(shellProc.pid));

  shellProc.kill();
  await sleep(1500);
  check('B3 daemon survives Shell close (M6 core semantics)', await alive(daemonPid));

  // B4: 断开后 daemon 重签凭证文件（HIGH-1），新凭证可直连重入
  let fresh = null;
  for (let i = 0; i < 100 && !fresh; i++) {
    try {
      const f = JSON.parse(readFileSync(CRED_FILE, 'utf8'));
      if (f.credential.token !== cred.credential.token) fresh = f;
    } catch { /* partial write */ }
    if (!fresh) await sleep(50);
  }
  check('B4 daemon re-issues credential after disconnect', !!fresh, fresh ? 'new token: ' + fresh.credential.token.slice(0, 12) : '');
  if (!fresh) failFast('B4 daemon re-issues credential after disconnect', new Error('no reissued credential'));
  let bclient = null;
  try {
    bclient = await EnvelopeClient.connect(fresh.port, fresh.credential.token);
    const agreement = await bclient.negotiate(supports, 'qa-restart-0002');
    const tstat = await bclient.invoke(CAP.terminal, 'terminal.status');
    const ok = agreement.payload.granted.length >= 5 && tstat.payload && tstat.payload.count === 0;
    check('B5 re-attach with reissued credential (handshake+invoke)', !!ok, ok ? 'granted=' + agreement.payload.granted.length : JSON.stringify(tstat).slice(0, 100));
    bclient.close();
  } catch (e) { failFast('B5 re-attach with reissued credential (handshake+invoke)', e); }

  // B6: Shell 重启（第二次）—— 真实重连到存活 daemon（stderr connected 证据）
  shellStderr.length = 0;
  shellProc = spawnShell();
  await sleep(5000);
  const connectedLog = shellStderr.join('').includes('[daemon-client] connected');
  check('B6 Shell restart reconnects to surviving daemon', connectedLog, connectedLog ? '' : 'stderr: ' + shellStderr.join('').slice(0, 200));
  check('B7 Shell process alive after restart', await alive(shellProc.pid));
  shellProc.kill();
}

async function cleanup() {
  try { if (shellProc) shellProc.kill(); } catch {}
  try { if (daemonProc) daemonProc.kill(); } catch {}
  await sleep(500);
  writeFileSync(join(EVIDENCE, 'results.json'), JSON.stringify(results, null, 2));
  const passed = results.filter((r) => r.ok).length;
  console.log('\n===== RESULT: ' + passed + '/' + results.length + ' PASS =====');
  console.log('evidence: ' + EVIDENCE);
}

try {
  await phaseA();
  await phaseB();
} catch (e) {
  console.error('QA aborted: ' + e);
  failFast('QA overall', e);
} finally {
  await cleanup();
}