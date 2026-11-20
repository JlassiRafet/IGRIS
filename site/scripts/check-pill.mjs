#!/usr/bin/env node
// Checks the Home page in real Chromium (DevTools protocol): the Index pill, reduced motion,
// a phone width and scripting off. Run by hand against the built site:
//   npm run build && npx astro preview --port 4399 &
//   node scripts/check-pill.mjs http://localhost:4399/IGRIS/
// Needs Node 22+ and `chromium` on PATH (or CHROME=/path). No packages.
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const url = process.argv[2];
if (!url) {
  console.error('usage: node scripts/check-pill.mjs <home url>');
  process.exit(2);
}
const PORT = 9300 + Math.floor(Math.random() * 600);
const dir = mkdtempSync(join(tmpdir(), 'igris-site-'));
const chrome = spawn(process.env.CHROME || 'chromium', ['--headless=new', '--no-sandbox', '--disable-gpu', `--remote-debugging-port=${PORT}`, `--user-data-dir=${dir}`, '--window-size=1280,800', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const ok = (name, cond, extra = '') => results.push(`${cond ? 'PASS' : 'FAIL'} ${name}${extra ? ` (${extra})` : ''}`);
const until = async (fn, ms = 8000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    try {
      const v = await fn();
      if (v) return v;
    } catch {
      /* not ready */
    }
    await sleep(100);
  }
  return null;
};

try {
  await until(async () => (await fetch(`http://127.0.0.1:${PORT}/json/version`)).ok);
  const t = await until(async () => (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).find((x) => x.type === 'page'));
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => ((ws.onopen = res), (ws.onerror = rej)));
  let id = 0;
  const pending = new Map();
  const problems = [];
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.id && pending.has(d.id)) pending.get(d.id)(d);
    if (d.method === 'Runtime.exceptionThrown') problems.push(d.params.exceptionDetails.text);
    if (d.method === 'Log.entryAdded' && d.params.entry.level === 'error') problems.push(d.params.entry.text + ' ' + (d.params.entry.url || ''));
  };
  const send = (method, params = {}) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  const ev = async (expression) => (await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
  await send('Page.enable');
  await send('Runtime.enable');
  await send('Log.enable');
  await send('Page.navigate', { url });
  await until(async () => (await ev(`document.getElementById('pct')?.textContent`)) !== undefined);
  await sleep(400);

  const pct = () => ev(`document.getElementById('pct').textContent`);
  ok('the pill shows 0% at the top', (await pct()) === '0%');
  await ev(`document.documentElement.style.scrollBehavior = 'auto'; window.scrollTo(0, 1500)`);
  await sleep(250);
  const mid = parseInt(await pct(), 10);
  ok('the percentage rises as you scroll', mid > 20 && mid < 100, `${mid}%`);
  ok('the current section is marked in the list', (await ev(`document.querySelectorAll('.menu a[aria-current="true"]').length`)) === 1);
  await ev(`window.scrollTo(0, document.documentElement.scrollHeight)`);
  await sleep(250);
  ok('the percentage reaches 100% at the bottom', (await pct()) === '100%');
  await ev(`document.getElementById('pill').click()`);
  ok('clicking the pill opens the list', (await ev(`document.getElementById('dock').dataset.open === 'true' && document.getElementById('pill').getAttribute('aria-expanded') === 'true'`)) === true);
  if (process.env.SHOT) {
    await sleep(600); // let the open animation settle
    const { writeFileSync } = await import('node:fs');
    writeFileSync(process.env.SHOT, Buffer.from((await send('Page.captureScreenshot', { format: 'png' })).result.data, 'base64'));
  }
  ok('the list has six sections', (await ev(`document.querySelectorAll('.menu a').length`)) === 6);
  await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  ok('Esc closes the list', (await ev(`document.getElementById('dock').dataset.open`)) === 'false');
  await ev(`document.getElementById('pill').focus()`);
  ok('the pill shows a focus ring from the keyboard', (await ev(`getComputedStyle(document.getElementById('pill')).outlineStyle !== 'none' || document.getElementById('pill').matches(':focus-visible')`)) === true);

  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] });
  await ev(`document.documentElement.style.scrollBehavior = ''`);
  ok('reduced motion: no smooth scrolling', (await ev(`getComputedStyle(document.documentElement).scrollBehavior`)) === 'auto');
  ok('reduced motion: the list does not animate', (await ev(`getComputedStyle(document.getElementById('menu')).transitionDuration`)).split(',').every((d) => d.trim() === '0s'));
  await send('Emulation.setEmulatedMedia', { features: [] });

  await send('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: false });
  await sleep(300);
  const wide = await ev(`[...document.querySelectorAll('body *')].filter(e => e.getBoundingClientRect().right > innerWidth + 1 && getComputedStyle(e).position !== 'fixed' && !e.closest('pre')).map(e => e.tagName.toLowerCase() + (e.className ? '.' + String(e.className).split(' ')[0] : '') + ' ' + Math.round(e.getBoundingClientRect().right)).join(', ')`);
  ok('390 px: no sideways scroll', wide === '', wide);
  ok('390 px: the pill is inside the window', (await ev(`(() => { const r = document.getElementById('pill').getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; })()`)) === true);
  await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });

  await send('Emulation.setScriptExecutionDisabled', { value: true });
  await send('Page.navigate', { url });
  await sleep(1200);
  ok('scripting off: all six sections are readable', (await ev(`document.querySelectorAll('main h2').length`)) === 6 && (await ev(`getComputedStyle(document.querySelector('#how')).display !== 'none'`)));
  ok('scripting off: the pill is not shown', (await ev(`document.getElementById('dock').hidden`)) === true);
  await send('Emulation.setScriptExecutionDisabled', { value: false });
  ok('no console or script errors', problems.length === 0, problems.join(' | '));
  ws.close();
} catch (e) {
  results.push(`FAIL harness: ${e}`);
} finally {
  chrome.kill();
  try {
    rmSync(dir, { recursive: true, force: true });
  } catch {
    /* small temp folder */
  }
  console.log(results.join('\n'));
  process.exit(results.some((r) => r.startsWith('FAIL')) ? 1 : 0);
}
