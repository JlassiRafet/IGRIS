// The Check setup page, terminal skin. It asks the local server for a "view" (groups, rows, plain
// sentences) and draws it. All text goes in through textContent: nothing from the snapshot is ever
// parsed as HTML.
import { filterActions, move, isOpenKey } from './palette.js';

const DOCS = 'https://jlassirafet.github.io/IGRIS/docs/';
const SEGMENTS = 24;
const $ = (id) => document.getElementById(id);
const reduceMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

/** An element with an optional class and plain text. */
function el(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}

// ---------- drawing the view ----------

const BOX = { ready: 'done', needs_step: 'cur', optional: 'opt', info: 'opt' };

function rowEl(r, last) {
  const li = el('li', BOX[r.status] ?? 'opt');
  li.append(el('span', 'tree', last ? '└' : '├'), el('span', 'box'), el('span', 'name2', r.name), el('span', 'det', r.detail));
  return li;
}

/** The folded part of a group (more agents, the ports checked) as one last row. */
function moreRow(m) {
  const names = m.rows.map((r) => r.name).join(', ');
  const note = m.rows.length ? ` (${m.rows[0].detail.toLowerCase()})` : '';
  return { name: m.title, detail: names ? names + note : m.chips.join(', '), status: 'optional' };
}

function stateText(g) {
  const live = g.rows.filter((r) => r.status !== 'info');
  const ready = live.filter((r) => r.status === 'ready').length;
  const need = live.filter((r) => r.status === 'needs_step').length;
  if (need) return need === 1 ? '1 needs a step' : `${need} need a step`;
  if (ready && ready === live.length && live.length > 1) return `${ready} of ${live.length} ready`;
  return ready ? 'ready' : 'optional';
}

function groupEl(g, open) {
  const d = el('details', 'g');
  d.dataset.id = g.id;
  d.open = open;
  const needs = g.status === 'needs_step';
  const hex = el('span', needs ? 'hex on' : 'hex');
  hex.setAttribute('aria-hidden', 'true');
  const summary = el('summary');
  summary.append(hex, el('span', 'gname', g.title), el('span', needs ? 'state on' : 'state', stateText(g)));
  const rows = g.rows.slice();
  if (g.more) rows.push(moreRow(g.more));
  const ul = el('ul', 'rows');
  rows.forEach((r, i) => ul.append(rowEl(r, i === rows.length - 1)));
  d.append(summary, ul);
  return d;
}

// What the person opened or closed by hand, so a rescan does not undo it.
const userOpen = new Map();
const expected = new Map();
$('groups').addEventListener(
  'toggle',
  (e) => {
    const d = e.target;
    if (!d.matches || !d.matches('details.g')) return;
    const id = d.dataset.id;
    if (d.open === expected.get(id)) userOpen.delete(id);
    else userOpen.set(id, d.open);
  },
  true,
);

function summaryLine(view) {
  const c = view.counts;
  const parts = [];
  if (c.ready) parts.push(el('span', 'val', `${c.ready} ready`));
  if (c.needs_step) parts.push(el('span', 'val', c.needs_step === 1 ? '1 needs a step' : `${c.needs_step} need a step`));
  if (c.optional) parts.push(document.createTextNode(`${c.optional} optional`));
  const line = $('summary');
  line.replaceChildren();
  parts.forEach((p, i) => {
    if (i) line.append(document.createTextNode(', '));
    line.append(p);
  });
  line.append(document.createTextNode(`. ${view.lead}`));
}

function draw(view) {
  const machine = view.groups.find((g) => g.id === 'machine');
  const facts = $('facts');
  facts.replaceChildren();
  for (const f of machine ? machine.facts : []) {
    const item = el('div');
    item.append(el('dt', '', f.label), el('dd', '', f.value));
    facts.append(item);
  }
  $('hint').textContent = machine?.hint ?? '';
  summaryLine(view);
  expected.clear();
  const els = view.groups
    .filter((g) => g.id !== 'machine')
    .map((g) => {
      expected.set(g.id, g.open);
      return groupEl(g, userOpen.has(g.id) ? userOpen.get(g.id) : g.open);
    });
  $('groups').replaceChildren(...els);
}

// ---------- the block bar and the hints line ----------

const bar = $('bar');
const segs = Array.from({ length: SEGMENTS }, () => bar.appendChild(document.createElement('i')));
const barText = bar.appendChild(el('span', 't', ''));
let timer = null;
function barTo(n, text, pct) {
  segs.forEach((s, i) => s.classList.toggle('on', i < n));
  barText.textContent = text;
  bar.setAttribute('aria-valuenow', String(pct));
}
function barStart() {
  clearInterval(timer);
  barTo(0, 'scanning', 0);
  if (reduceMotion()) return;
  let n = 0;
  timer = setInterval(() => {
    n = Math.min(n + 1, SEGMENTS - 4);
    barTo(n, 'scanning', Math.round((100 * n) / SEGMENTS));
    if (n >= SEGMENTS - 4) clearInterval(timer);
  }, 45);
}
function barDone(text) {
  clearInterval(timer);
  barTo(SEGMENTS, text, 100);
}
function barFail() {
  clearInterval(timer);
  barTo(0, 'scan failed', 0);
}

const hints = $('hints');
function defaultHints() {
  hints.classList.remove('bad');
  hints.replaceChildren(
    el('b', '', '/'), document.createTextNode(' quick actions · '),
    el('b', '', 'ctrl k'), document.createTextNode(' same · '),
    el('b', '', 'r'), document.createTextNode(' scan again · 127.0.0.1 only'),
  );
}
function say(text, bad = false) {
  hints.classList.toggle('bad', bad);
  hints.textContent = text;
}

// ---------- talking to the local server ----------

async function post(path, body) {
  return fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

function fail(message) {
  barFail();
  say(`error: ${message}`, true);
}

let scanning = false;
async function scan() {
  if (scanning) return;
  scanning = true;
  barStart();
  try {
    let r;
    try {
      r = await post('/api/action', { action: 'check' });
    } catch {
      return fail('The setup app stopped. Run igris setup again.');
    }
    if (r.status === 401) return fail('Session expired. Run igris setup again.');
    if (!r.ok) return fail(`Something went wrong (${r.status}). Run igris setup again.`);
    let view;
    try {
      view = await r.json();
    } catch {
      return fail('The setup app sent an answer this page cannot read. Run igris setup again.');
    }
    draw(view);
    barDone(`scanned ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`);
    defaultHints();
  } finally {
    scanning = false;
  }
}

/** Trade the one-time token in the link for the session cookie, and take it out of the address bar. */
async function boot() {
  const m = /(?:^|[#&])t=([0-9a-f]{64})(?:&|$)/.exec(location.hash);
  if (m) {
    history.replaceState(null, '', location.pathname + location.search);
    // A spent token is fine when the cookie is already valid (a second tab), so the scan decides.
    try {
      await post('/auth', { token: m[1] });
    } catch {
      /* the scan below reports a stopped app */
    }
  }
  await scan();
}

// ---------- quick actions ----------

const setAll = (open) => document.querySelectorAll('#groups details.g').forEach((d) => (d.open = open));
const actions = [
  { group: 'Actions', title: 'Scan again', keywords: 'refresh detect', run: scan },
  { group: 'Actions', title: 'Expand all groups', keywords: 'open show', run: () => setAll(true) },
  { group: 'Actions', title: 'Collapse all groups', keywords: 'close hide fold', run: () => setAll(false) },
  { group: 'Actions', title: 'Open the docs', keywords: 'help guide', run: () => window.open(DOCS, '_blank', 'noopener,noreferrer') },
  { group: 'Go to', title: 'Check', keywords: 'home', run: () => $('main').focus() },
  { group: 'Go to', title: 'Pipeline', keywords: 'configure', soon: true },
  { group: 'Go to', title: 'Keys', keywords: 'secrets logins', soon: true },
  { group: 'Go to', title: 'Review', keywords: 'install apply', soon: true },
  { group: 'Go to', title: 'Test', keywords: 'smoke doctor run', soon: true },
];

const scrim = $('scrim');
const input = $('pq');
const list = $('pl');
let current = 0;
let shown = [];
let opener = null;

function renderList() {
  shown = filterActions(actions, input.value);
  list.replaceChildren();
  if (!shown.length) {
    list.append(el('li', 'empty', 'No matching action. Try “scan” or “expand”.'));
    input.setAttribute('aria-activedescendant', '');
    return;
  }
  current = Math.min(current, shown.length - 1);
  let lastGroup = '';
  shown.forEach((a, n) => {
    if (a.group !== lastGroup) {
      lastGroup = a.group;
      const h = el('li', 'pgrp', a.group);
      h.setAttribute('role', 'presentation');
      list.append(h);
    }
    const li = el('li', 'opt-i');
    li.id = `o${n}`;
    li.setAttribute('role', 'option');
    li.setAttribute('aria-selected', String(n === current));
    if (a.soon) li.setAttribute('aria-disabled', 'true');
    li.append(el('span', '', '▸'), el('span', '', a.title));
    if (a.soon) li.append(el('span', 'tag', 'soon'));
    li.addEventListener('mouseenter', () => {
      current = n;
      markCurrent();
    });
    li.addEventListener('click', () => runAt(n));
    list.append(li);
  });
  input.setAttribute('aria-activedescendant', `o${current}`);
}

function markCurrent() {
  list.querySelectorAll('.opt-i').forEach((li, n) => li.setAttribute('aria-selected', String(n === current)));
  input.setAttribute('aria-activedescendant', `o${current}`);
  const li = $(`o${current}`);
  if (li) li.scrollIntoView({ block: 'nearest' });
}

function runAt(n) {
  const a = shown[n];
  if (!a || a.soon) return;
  closePalette();
  a.run();
}

/** `from` is the control that was clicked: Safari and macOS Firefox do not focus a button on click. */
function openPalette(from) {
  opener = from ?? document.activeElement;
  scrim.classList.add('on');
  input.value = '';
  current = 0;
  renderList();
  input.focus();
}

function closePalette() {
  scrim.classList.remove('on');
  input.blur();
  if (opener && opener !== document.body && opener.focus) opener.focus();
}

$('qa').addEventListener('click', (e) => openPalette(e.currentTarget));
$('cmd').addEventListener('click', (e) => openPalette(e.currentTarget));
scrim.addEventListener('mousedown', (e) => {
  if (e.target === scrim) closePalette();
});
input.addEventListener('input', () => {
  current = 0;
  renderList();
});
document.addEventListener('keydown', (e) => {
  const open = scrim.classList.contains('on');
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName) || e.target.isContentEditable;
  if (isOpenKey(e, typing)) {
    e.preventDefault();
    if (e.key === '/') {
      if (!open) openPalette();
    } else if (open) closePalette();
    else openPalette();
    return;
  }
  if (!open) {
    if (!typing && !e.ctrlKey && !e.metaKey && !e.altKey && e.key === 'r') {
      e.preventDefault();
      scan();
    }
    return;
  }
  if (e.key === 'Escape') {
    e.preventDefault();
    closePalette();
  } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    current = move(current, e.key === 'ArrowDown' ? 1 : -1, shown.length);
    if (shown.length) markCurrent();
  } else if (e.key === 'Enter') {
    e.preventDefault();
    runAt(current);
  } else if (e.key === 'Tab') {
    e.preventDefault();
  }
});

$('cta').addEventListener('click', () => say('The pipeline editor arrives in the next release.'));
$('qk').textContent = /Mac|iPhone|iPad/.test(navigator.userAgent) ? '⌘ K' : 'Ctrl K';
boot();
