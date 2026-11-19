// The Check setup page. It asks the local server for a "view" (groups, rows, plain sentences) and
// draws it. All text goes in through textContent: nothing from the snapshot is ever parsed as HTML.
import { filterActions, move, isOpenKey } from './palette.js';

const SVG_NS = 'http://www.w3.org/2000/svg';
const DOCS = 'https://jlassirafet.github.io/IGRIS/docs/';
const $ = (id) => document.getElementById(id);

/** An element with an optional class and plain text. */
function el(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}

/** An icon from the sprite in setup.html. `cls` adds to "i" (for example "s" for the small size). */
function icon(id, cls = '') {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('class', `i ${cls}`.trim());
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVG_NS, 'use');
  use.setAttribute('href', `#i-${id}`);
  svg.append(use);
  return svg;
}

const STATUS = {
  ready: { label: 'Ready', icon: 'tick', cls: 'ok', dot: 'check' },
  needs_step: { label: 'Needs a step', icon: 'alert', cls: 'warn', dot: 'alert' },
  optional: { label: 'Optional', icon: 'dash', cls: 'opt', dot: 'dash' },
  info: { label: '', icon: 'dash', cls: 'opt', dot: 'dash' },
};
const GROUP_ICON = { machine: 'cpu', build: 'term', decision: 'split', chat: 'chat' };

function pill(status) {
  const s = STATUS[status];
  if (!s || !s.label) return null;
  const p = el('span', `pill ${s.cls}`);
  p.append(icon(s.icon), document.createTextNode(s.label));
  return p;
}

function rowEl(r) {
  const s = STATUS[r.status] ?? STATUS.info;
  const li = el('li', 'row');
  const dot = el('span', `dot ${s.cls}`);
  dot.append(icon(s.dot, 's'));
  const meta = el('span', 'meta');
  meta.append(el('b', '', r.name), el('span', 'sub', r.detail));
  li.append(dot, meta);
  const p = pill(r.status);
  if (p) li.append(p);
  return li;
}

function rowsEl(rows) {
  const ul = el('ul', 'rows');
  for (const r of rows) ul.append(rowEl(r));
  return ul;
}

function groupEl(g, open) {
  const d = el('details', 'g');
  d.dataset.id = g.id;
  d.open = open;

  const summary = el('summary');
  const gic = el('span', 'gic');
  gic.setAttribute('aria-hidden', 'true');
  gic.append(icon(GROUP_ICON[g.id] ?? 'dash'));
  const gt = el('span', 'gt');
  gt.append(el('h2', '', g.title), el('span', 'gs', g.summary));
  summary.append(gic, gt);
  const p = pill(g.status);
  if (p) summary.append(p);
  summary.append(icon('chev', 'chev'));

  const body = el('div', 'gbody');
  if (g.facts.length) {
    const facts = el('div', 'facts');
    for (const f of g.facts) {
      const fact = el('div', 'fact');
      fact.append(el('span', '', f.label), el('b', '', f.value));
      facts.append(fact);
    }
    body.append(facts);
  }
  if (g.hint) body.append(el('p', 'hint', g.hint));
  if (g.rows.length) body.append(rowsEl(g.rows));
  if (g.more) {
    const more = el('details', 'sub2');
    const ms = el('summary');
    ms.append(icon('chev', 's chev'), document.createTextNode(g.more.title));
    more.append(ms);
    if (g.more.rows.length) more.append(rowsEl(g.more.rows));
    if (g.more.chips.length) {
      const chips = el('div', 'chips');
      for (const c of g.more.chips) chips.append(el('span', 'chip', c));
      more.append(chips);
    }
    body.append(more);
  }
  d.append(summary, body);
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

function draw(view) {
  $('lead').textContent = view.lead;
  const counts = $('counts');
  counts.replaceChildren();
  const c = view.counts;
  const parts = [
    ['ready', c.ready, `${c.ready} ready`],
    ['needs_step', c.needs_step, c.needs_step === 1 ? '1 needs a step' : `${c.needs_step} need a step`],
    ['optional', c.optional, `${c.optional} optional`],
  ];
  for (const [status, n, text] of parts) {
    if (!n) continue;
    const s = STATUS[status];
    const p = el('span', `pill ${s.cls}`);
    p.setAttribute('role', 'listitem');
    p.append(icon(s.icon), document.createTextNode(text));
    counts.append(p);
  }
  const groups = $('groups');
  expected.clear();
  const els = view.groups.map((g) => {
    expected.set(g.id, g.open);
    return groupEl(g, userOpen.has(g.id) ? userOpen.get(g.id) : g.open);
  });
  groups.replaceChildren(...els);
}

function showErr(text) {
  const e = $('err');
  e.textContent = text;
  e.hidden = !text;
}
const setStamp = (text) => {
  $('stamp').textContent = text;
};

async function post(path, body) {
  return fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

let scanning = false;
async function scan() {
  if (scanning) return;
  scanning = true;
  setStamp('Scanning…');
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
    showErr('');
    setStamp(`Scanned ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`);
  } finally {
    scanning = false;
  }
}

function fail(message) {
  showErr(message);
  setStamp('Not scanned');
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

// Theme: follows the system, and can be flipped for this visit. Nothing is stored: the port, and so
// the page's storage, changes on every run.
const root = document.documentElement;
const isDark = () =>
  root.dataset.theme === 'dark' || (!root.dataset.theme && matchMedia('(prefers-color-scheme: dark)').matches);
function paintTheme() {
  const dark = isDark();
  $('thl').textContent = dark ? 'Light' : 'Dark';
  $('thi').firstElementChild.setAttribute('href', dark ? '#i-sun' : '#i-moon');
}
function toggleTheme() {
  root.dataset.theme = isDark() ? 'light' : 'dark';
  paintTheme();
}

const setAll = (open) => document.querySelectorAll('#groups details.g').forEach((d) => (d.open = open));

// Quick actions
const actions = [
  { group: 'Actions', title: 'Scan again', keywords: 'refresh detect', icon: 'refresh', run: scan },
  { group: 'Actions', title: 'Expand all sections', keywords: 'open show', icon: 'chev', run: () => setAll(true) },
  { group: 'Actions', title: 'Collapse all sections', keywords: 'close hide fold', icon: 'chev', run: () => setAll(false) },
  { group: 'Actions', title: 'Switch theme', keywords: 'dark light mode', icon: 'sun', run: toggleTheme },
  {
    group: 'Actions',
    title: 'Open the docs',
    keywords: 'help guide',
    icon: 'book',
    run: () => window.open(DOCS, '_blank', 'noopener,noreferrer'),
  },
  { group: 'Go to', title: 'Check setup', keywords: 'home', icon: 'scan', run: () => $('h1').focus() },
  { group: 'Go to', title: 'Pipeline', keywords: 'configure', icon: 'route', soon: true },
  { group: 'Go to', title: 'Keys and logins', keywords: 'secrets', icon: 'key', soon: true },
  { group: 'Go to', title: 'Review and apply', keywords: 'install', icon: 'clip', soon: true },
  { group: 'Go to', title: 'Test run', keywords: 'smoke doctor', icon: 'play', soon: true },
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
    list.append(el('li', 'empty', 'No matching action. Try “scan” or “theme”.'));
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
    li.append(icon(a.icon), el('span', '', a.title));
    if (a.soon) li.append(el('span', 'tag', 'Soon'));
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

function openPalette() {
  opener = document.activeElement;
  scrim.classList.add('on');
  input.value = '';
  current = 0;
  renderList();
  input.focus();
}

function closePalette() {
  scrim.classList.remove('on');
  if (opener && opener.focus) opener.focus();
}

$('qa').addEventListener('click', openPalette);
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
  if (!open) return;
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

$('th').addEventListener('click', toggleTheme);
$('scan').addEventListener('click', scan);
$('cta').addEventListener('click', () => setStamp('The pipeline editor arrives in the next release.'));
$('qk').textContent = /Mac|iPhone|iPad/.test(navigator.userAgent) ? '⌘ K' : 'Ctrl K';
paintTheme();
boot();
