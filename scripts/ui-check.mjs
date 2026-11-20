#!/usr/bin/env node
// Checks the setup page in real Chromium (DevTools protocol, real key events) against a running app.
//
//   igris setup --no-open            # prints {"url": "..."}; use that link once
//   node scripts/ui-check.mjs "<url>"            # the main checks
//   node scripts/ui-check.mjs "<url>" stopped    # for an app started with --idle-seconds 3
//
// Needs Node 22+ (global WebSocket) and `chromium` on PATH (or CHROME=/path). No packages.
// Run by hand: it needs a browser and a live app, so it is not part of CI.
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [, , url, mode = 'main'] = process.argv;
if (!url) {
  console.error('usage: node scripts/ui-check.mjs "<url from igris setup>" [main|stopped]');
  process.exit(2);
}
const PORT = 9300 + Math.floor(Math.random() * 600);
const dir = mkdtempSync(join(tmpdir(), 'igris-ui-'));
const chrome = spawn(
  process.env.CHROME || 'chromium',
  ['--headless=new', '--no-sandbox', '--disable-gpu', `--remote-debugging-port=${PORT}`, `--user-data-dir=${dir}`, '--window-size=1360,860', 'about:blank'],
  { stdio: 'ignore' },
);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const ok = (name, cond, extra = '') => results.push(`${cond ? 'PASS' : 'FAIL'} ${name}${extra ? ` (${extra})` : ''}`);

async function until(fn, ms = 8000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    try {
      const v = await fn();
      if (v) return v;
    } catch {
      /* not ready yet */
    }
    await sleep(100);
  }
  return null;
}

async function client(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = rej;
  });
  let id = 0;
  const pending = new Map();
  const problems = [];
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.id && pending.has(d.id)) pending.get(d.id)(d);
    if (d.method === 'Runtime.exceptionThrown') problems.push(d.params.exceptionDetails.text);
    if (d.method === 'Log.entryAdded' && d.params.entry.level === 'error') problems.push(d.params.entry.text);
  };
  const send = (method, params = {}) =>
    new Promise((res) => {
      const i = ++id;
      pending.set(i, res);
      ws.send(JSON.stringify({ id: i, method, params }));
    });
  const ev = async (expression) =>
    (await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
  const key = async (k, code, vk, { modifiers = 0, text } = {}) => {
    await send('Input.dispatchKeyEvent', { type: text ? 'keyDown' : 'rawKeyDown', key: k, code, windowsVirtualKeyCode: vk, modifiers, text });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code, windowsVirtualKeyCode: vk, modifiers });
  };
  await send('Page.enable');
  await send('Runtime.enable');
  await send('Log.enable');
  return { send, ev, key, problems, close: () => ws.close() };
}

const page = async () => {
  const t = await until(async () => (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).find((x) => x.type === 'page'));
  return client(t.webSocketDebuggerUrl);
};
const scanned = (c) => until(async () => (await c.ev(`document.getElementById('bar').textContent`))?.startsWith('scanned'));
const KEY = { tab: ['Tab', 'Tab', 9], esc: ['Escape', 'Escape', 27], down: ['ArrowDown', 'ArrowDown', 40], up: ['ArrowUp', 'ArrowUp', 38], enter: ['Enter', 'Enter', 13, { text: '\r' }] };

try {
  await until(async () => (await fetch(`http://127.0.0.1:${PORT}/json/version`)).ok);
  const a = await page();
  await a.send('Page.navigate', { url });

  if (mode === 'main') {
    ok('page draws a scan', !!(await scanned(a)));
    ok('token is gone from the address bar', (await a.ev('location.hash')) === '');
    const open = await a.ev(`JSON.stringify([...document.querySelectorAll('#groups details.g')].map(d=>d.dataset.id+':'+d.open))`);
    ok('three groups, only the one needing a step is open', /^\["build:false","decision:false","chat:(true|false)"\]$/.test(open), open);
    ok('four machine facts', (await a.ev(`document.querySelectorAll('#facts dt').length`)) === 4);
    ok('summary line is drawn', (await a.ev(`document.getElementById('summary').textContent`)).includes('ready'));
    const on = () => a.ev(`document.getElementById('scrim').classList.contains('on')`);

    await a.key(...KEY.esc.slice(0, 3));
    await a.key('k', 'KeyK', 75, { modifiers: 2 });
    ok('real Ctrl+K opens the palette', await on());
    ok('the search field has focus', (await a.ev(`document.activeElement.id`)) === 'pq');
    await a.key(...KEY.esc);
    ok('Esc closes', !(await on()));
    await a.ev(`document.activeElement.blur(); document.body.focus()`);
    await a.ev(`document.getElementById('qa').click()`);
    await a.key(...KEY.esc);
    ok('Esc returns focus to Quick actions even when the click did not focus it', (await a.ev(`document.activeElement.id`)) === 'qa');
    await a.ev(`document.activeElement.blur(); document.body.focus()`);
    await a.key('/', 'Slash', 191, { text: '/' });
    ok('slash opens outside a text field', await on());
    await a.send('Input.insertText', { text: 'a/b' });
    ok('slash inside the search field is just a slash', (await a.ev(`document.getElementById('pq').value`)) === 'a/b');
    await a.ev(`(() => { const i = document.getElementById('pq'); i.value = ''; i.dispatchEvent(new Event('input')); })()`);
    const before = await a.ev(`document.getElementById('pq').getAttribute('aria-activedescendant')`);
    await a.key(...KEY.down);
    ok('arrow moves the highlighted option', (await a.ev(`document.getElementById('pq').getAttribute('aria-activedescendant')`)) !== before);
    await a.key(...KEY.up);
    await a.key(...KEY.up);
    await a.key(...KEY.enter);
    ok('Enter on a soon item keeps the palette open', await on());
    await a.ev(`(() => { const i = document.getElementById('pq'); i.value = 'expand'; i.dispatchEvent(new Event('input')); })()`);
    await a.key(...KEY.enter);
    ok('Expand all opens every group', await a.ev(`[...document.querySelectorAll('#groups details.g')].every(d=>d.open)`));
    ok('the palette closed after running an action', !(await on()));
    await a.ev(`document.activeElement.blur(); document.body.focus()`);
    await a.key('r', 'KeyR', 82, { text: 'r' });
    ok('r scans again', (await a.ev(`document.getElementById('bar').textContent`)).startsWith('scanning'));
    ok('the rescan finishes', !!(await scanned(a)));
    ok('a rescan keeps what the person opened', await a.ev(`[...document.querySelectorAll('#groups details.g')].every(d=>d.open)`));

    // Keyboard only: Tab through the page; every control must be reached and show a focus ring.
    await a.ev(`document.activeElement.blur(); document.body.focus(); window.scrollTo(0,0)`);
    const seen = new Set();
    const bad = [];
    for (let i = 0; i < 24; i++) {
      await a.key(...KEY.tab);
      const f = JSON.parse(await a.ev(`(() => { const e = document.activeElement, s = getComputedStyle(e); return JSON.stringify({ id: e.id || e.tagName.toLowerCase() + (e.className ? '.' + String(e.className).split(' ')[0] : ''), outline: s.outlineStyle, w: s.outlineWidth }); })()`));
      seen.add(f.id);
      if (f.id !== 'body' && (f.outline === 'none' || f.w === '0px')) bad.push(f.id); // body = Tab wrapped out of the page
    }
    ok('Tab reaches Quick actions, the command bar, the primary button and the group summaries', ['qa', 'cmd', 'cta'].every((x) => seen.has(x)) && [...seen].some((x) => x.startsWith('summary')), [...seen].join(' '));
    ok('every focused control shows a focus ring', bad.length === 0, bad.join(' '));

    // A second tab opened with the same, spent link still renders (the cookie is shared).
    const t = await a.send('Target.createTarget', { url });
    const list = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
    const b = await client(list.find((x) => x.id === t.result.targetId).webSocketDebuggerUrl);
    ok('a second tab with the spent link still renders', !!(await scanned(b)));
    b.close();

    // Reduced motion: no blink, no transition.
    await a.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
    ok('reduced motion: the caret does not blink', (await a.ev(`getComputedStyle(document.querySelector('.caret')).animationName`)) === 'none');
    ok('reduced motion: the primary button does not transition', (await a.ev(`getComputedStyle(document.getElementById('cta')).transitionDuration`)).split(',').every((d) => d.trim() === '0s'));
    await a.send('Emulation.setEmulatedMedia', { features: [] });

    // A narrow window: no sideways scroll, and the dock never covers the last row.
    await a.send('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: false });
    await sleep(300);
    ok('390 px: no horizontal scroll', (await a.ev(`document.documentElement.scrollWidth <= innerWidth`)) === true, await a.ev(`document.documentElement.scrollWidth + ' vs ' + innerWidth`));
    await a.ev(`window.scrollTo(0, document.documentElement.scrollHeight)`);
    await sleep(150);
    ok('390 px: the dock does not hide the last row', await a.ev(`(() => { const rows = document.querySelectorAll('.rows li'); const last = rows[rows.length - 1].getBoundingClientRect(); return last.bottom <= document.querySelector('.dock').getBoundingClientRect().top + 1; })()`));
    ok('no console or script errors', a.problems.length === 0, a.problems.join(' | '));
  } else if (mode === 'stopped') {
    ok('page draws a scan', !!(await scanned(a)));
    await sleep(5000);
    await a.ev(`document.getElementById('scrim').classList.remove('on'); document.activeElement.blur(); document.body.focus()`);
    await a.key('r', 'KeyR', 82, { text: 'r' });
    const msg = await until(async () => {
      const t = await a.ev(`document.getElementById('hints').textContent`);
      return t.includes('stopped') ? t : null;
    });
    ok('after the app stops, scanning again says so', !!msg, msg || 'no message');
    ok('the last scan stays on screen', (await a.ev(`document.querySelectorAll('#groups details.g').length`)) === 3);
  }
  a.close();
} catch (e) {
  results.push(`FAIL harness: ${e}`);
} finally {
  chrome.kill();
  try {
    rmSync(dir, { recursive: true, force: true });
  } catch {
    /* the temp folder is small and lives in the system temp directory */
  }
  console.log(results.join('\n'));
  process.exit(results.some((r) => r.startsWith('FAIL')) ? 1 : 0);
}
