// One task through IGRIS. The tasks are examples; the order of the steps, the four decision questions,
// the risk rules and the limits (2 review rounds, 2 fix rounds) are the real ones from
// crates/igris-core (decide.rs, orchestrate.rs) and examples/igris.example.toml.
//
// Time is virtual: everything waits on one clock that the page advances (live, once per frame) or that
// scripts/preview.py advances to an exact moment (?at=MS) to record the GIF in the README.
(() => {
const $ = id => document.getElementById(id);
const params = new URLSearchParams(location.search);
const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
if (params.has('bare')) document.body.classList.add('bare');
const at = params.has('at') ? Number(params.get('at')) : null; // seek mode: run to this moment, then stop
if (at !== null) document.documentElement.classList.add('still');

// Wordmark: decoded from the same half-block rows the terminal app uses (wordmark.rs).
const ROWS = ['▀█▀ █▀▀ █▀█ ▀█▀ █▀▀', '▄█▄ █▄█ █▀▄ ▄█▄ ▄▄█'];
const GLOW = [0xbc, 0xfa, 0xf1], ACC = [0x08, 0x79, 0xf3];
const wm = $('wm');
for (let r = 0; r < 4; r++) for (let c = 0; c < 19; c++) {
  const px = document.createElement('i');
  if ((r % 2 ? '▄█' : '▀█').includes(ROWS[r >> 1][c])) {
    px.style.background = `rgb(${GLOW.map((g, k) => Math.round(g + (ACC[k] - g) * ((c / 18 + r / 3) / 2))).join()})`;
  }
  wm.append(px);
}
const plume = document.createElement('b');
plume.textContent = 'ʅ'; plume.setAttribute('aria-hidden', 'true');
wm.append(plume);

// The clock. sleep() and fly() both wait on it, so a run is the same whether it is watched or recorded.
const clock = { t: 0, timers: [], flights: [] };
const sleep = ms => new Promise(resolve => clock.timers.push({ at: clock.t + ms, resolve }));
function tick(dt) {
  clock.t += dt;
  clock.flights.forEach(f => f.update(clock.t));
  clock.flights = clock.flights.filter(f => !f.done);
  const due = clock.timers.filter(x => x.at <= clock.t).sort((a, b) => a.at - b.at);
  clock.timers = clock.timers.filter(x => x.at > clock.t);
  due.forEach(x => x.resolve());
}

const C = { you: '#e6e9ec', her: '#9be8d8', jev: '#ec7fb3', cla: '#e3875c', cdx: '#6aa7f2', hrd: '#a995f0', sh: '#c9ced3' };
const WHO = { you: C.you, hermes: C.her, igris: '#bcfaf1', claude: C.cla, codex: C.cdx, shell: C.sh };
// A pane's dot: the task statuses of orchestrate.rs (Running, Done, Failed) or idle.
const ST = { idle: ['#3a4650', 'idle'], working: ['#f2c14e', 'Running'], done: ['#bcfaf1', 'Done'], failed: ['#ff3038', 'Failed'] };
const NS = 'http://www.w3.org/2000/svg';

// A short tail of pixels runs along a wire: the task moving from one part to the next.
function fly(id, col) {
  const p = $(id), L = p.getTotalLength(), dur = Math.max(450, L * 4.2), g = $('dots');
  const cs = [5, 4, 3, 2].map((s, i) => {
    const c = document.createElementNS(NS, 'rect');
    c.setAttribute('width', s); c.setAttribute('height', s); c.setAttribute('fill', col); c.setAttribute('opacity', 1 - i * 0.24);
    g.appendChild(c); return [c, s];
  });
  return new Promise(resolve => {
    const t0 = clock.t;
    const f = { done: false, update(now) {
      const t = now - t0;
      cs.forEach(([c, s], i) => {
        const d = Math.max(0, Math.min(L, t / dur * L - i * 8)), pt = p.getPointAtLength(d);
        c.setAttribute('x', Math.round(pt.x - s / 2)); c.setAttribute('y', Math.round(pt.y - s / 2));
      });
      if (t >= dur) { cs.forEach(([c]) => c.remove()); f.done = true; resolve(); }
    } };
    clock.flights.push(f); f.update(t0);
  });
}
const on = (id, v) => $(id).classList.toggle('on', !!v);
const cap = (s, col) => { $('cap').textContent = s; if (col) $('hex').style.background = col; };
const state = s => { $('sI').textContent = '[' + s + ']'; };

let lines = [], t0 = 0;
function log(who, msg, tag) {
  const s = Math.round((clock.t - t0) / 1000);
  lines.push({ t: `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`, who, msg, tag: tag || '' });
  lines = lines.slice(-5);
  $('log').innerHTML = lines.map((l, i) =>
    `<div style="opacity:${(0.45 + 0.55 * (i + 1) / lines.length).toFixed(2)}"><span class="d"></span><b style="color:${WHO[l.who]}">${l.who}</b><span class="ell"></span><span class="d ell" style="text-align:right"></span></div>`).join('');
  [...$('log').children].forEach((d, i) => {
    const s2 = d.querySelectorAll('span'); s2[0].textContent = lines[i].t; s2[1].textContent = lines[i].msg; s2[2].textContent = lines[i].tag;
  });
}

// The decision: the four questions the rules answer (decide.rs), each with a confidence (rules: 1.00).
function setRow(i, v, flag) {
  const r = $('v' + i); r.textContent = v; r.classList.toggle('flag', !!flag);
  $('b' + i).style.width = '100%'; $('p' + i).textContent = '1.00';
}
const resetRows = () => { for (let i = 0; i < 4; i++) { $('v' + i).textContent = '…'; $('v' + i).classList.remove('flag'); $('b' + i).style.width = '0'; $('p' + i).textContent = '—'; } };

const panes = ['idle', 'idle', 'idle'];
function pane(i, st, pct, act) {
  panes[i] = st; const [c, w] = ST[st];
  $('pb' + i).style.width = pct + '%'; $('pp' + i).textContent = pct + '%'; if (act) $('pa' + i).textContent = act;
  $('pd' + i).style.background = c; $('ps' + i).textContent = w; $('ps' + i).style.color = st === 'idle' ? '' : c;
  on('pn' + i, st === 'working');
  const roll = panes.includes('failed') ? 'failed' : panes.includes('working') ? 'working' : panes.includes('done') ? 'done' : 'idle';
  on('bHrd', roll === 'working');
  $('sR').textContent = '[3 panes · ' + ST[roll][1].toLowerCase() + ']';
  $('sC').textContent = '[' + (panes[1] === 'working' ? 'reviewing' : ST[panes[1]][1].toLowerCase()) + ']';
}

// Example tasks. tier is small for short fix, rename, typo or bump tasks. Risk is high when the task has
// a risky word (migration, deploy, secret, auth, delete, drop) or touches a risky path.
const TASKS = [
  { msg: 'fix the failing cart test', tier: 'small', files: ['src/cart.ts'], tests: 57,
    acts: ['reading the test log', 'editing src/cart.ts', 'running the cart test'] },
  { msg: 'add rate limiting to the api', tier: 'mid', files: ['src/api/limiter.ts', 'src/api/routes.ts', 'test/limiter.test.ts'], tests: 84,
    changes: 'no test for the 429 case', acts: ['reading src/api/', 'writing limiter.ts', 'wiring routes.ts'] },
  { msg: 'add a users table migration', tier: 'mid', risky: 'risky word: migration', files: ['db/migrations/014_users.sql'], tests: 61,
    acts: ['reading db/schema', 'writing 014_users.sql', 'dry run on a copy'] },
  { msg: 'bump axios to 1.7', tier: 'small', files: ['package.json', 'package-lock.json'], tests: 112,
    fail: '3 snapshot tests changed', acts: ['reading package.json', 'bumping axios', 'updating the lockfile'] }
];
let done = 0;

async function runTask(T, last) {
  t0 = clock.t; lines = []; $('log').innerHTML = ''; resetRows();
  for (let i = 0; i < 3; i++) pane(i, 'idle', 0, 'waiting');
  ['bYou', 'bHer', 'bDec', 'bRep'].forEach(id => on(id, 0));
  $('repMsg').textContent = 'nothing to report yet'; $('youMsg').textContent = '» …'; $('hTask').textContent = '» waiting';
  const high = !!T.risky, run = `igris run --task "${T.msg}"`;
  $('cmd').textContent = 'igris run'; state('idle');
  const files = T.files.length + (T.files.length > 1 ? ' files' : ' file');

  on('bYou', 1); $('youMsg').textContent = '» ' + T.msg;
  cap(`1. You message Hermes: "${T.msg}".`, C.you); log('you', `"${T.msg}"`, 'telegram');
  await sleep(700); await fly('pYou', C.you); on('bYou', 0);
  on('bHer', 1); $('hTask').textContent = '» ' + run; await sleep(500);
  $('cmd').textContent = run; log('hermes', run, '');

  cap('2. Hermes runs igris. IGRIS answers four questions: which agent, how big, how risky, and whether it needs your yes.', C.jev);
  await fly('pDec', C.her); on('bHer', 0); on('bDec', 1); state('deciding');
  const out = [['claude'], [T.tier], [high ? 'high' : 'low', high], [high ? 'yes' : 'no', high]];
  for (let i = 0; i < 4; i++) { setRow(i, ...out[i]); await sleep(450); }
  log('igris', `agent=claude tier=${T.tier} risk=${high ? 'high' : 'low'} needs_approval=${high ? 'yes' : 'no'}`, 'rules 1.00');
  if (high) {
    state('needs approval'); cap(`Risky (${T.risky}): IGRIS stops with exit 3 and nothing runs. Hermes asks you.`, C.her);
    log('igris', `exit 3 · needs approval (${T.risky})`, 'exit 3'); await sleep(900);
    on('bDec', 0); on('bHer', 1); log('hermes', 'asks you to approve', ''); on('bYou', 1);
    $('youMsg').textContent = '» approve? y/n'; await fly('pYouB', C.her); await sleep(1200);
    $('youMsg').textContent = '» y'; log('you', 'y', 'approved'); await fly('pYou', C.you); on('bYou', 0);
    $('hTask').textContent = '» ' + run + ' --approved'; $('cmd').textContent = run + ' --approved';
    log('hermes', 'igris run --approved', ''); await sleep(400);
    await fly('pDec', C.her); on('bHer', 0); on('bDec', 1);
  }
  await sleep(300); on('bDec', 0);

  state('running');
  cap('3. IGRIS starts the builder in a Herdr pane, in its own git worktree.', C.cla);
  await fly('pBld', C.jev); pane(0, 'working', 5, 'starting'); log('igris', 'build: claude, own worktree', '');
  for (let k = 0; k < 3; k++) { await sleep(750); pane(0, 'working', (k + 1) * 28, T.acts[k]); }
  await sleep(500); pane(0, 'done', 100, files + ' changed'); log('claude', `${files} changed`, T.files[0]);

  cap('4. A different agent reviews the diff, read-only. It approves, or asks for changes (up to 2 rounds).', C.cdx);
  await fly('pRev', C.cla); pane(1, 'working', 25, 'reading the diff'); log('codex', 'reading the diff', 'read-only');
  await sleep(800); pane(1, 'working', 65, 'checking the change'); await sleep(700);
  if (T.changes) {
    pane(1, 'done', 100, 'changes asked'); log('codex', `changes requested: ${T.changes}`, 'review round 1 of 2');
    cap('Codex asked for changes. Claude Code fixes them, then Codex looks again (round 1 of 2).', C.cla);
    await sleep(600); await fly('pFix', C.cdx); pane(0, 'working', 80, 'adding the test');
    await sleep(900); pane(0, 'done', 100, 'test added'); log('claude', 'added the missing test', 'review fix');
    await fly('pRev', C.cla); pane(1, 'working', 50, 'reading the diff again'); await sleep(700);
  }
  pane(1, 'done', 100, 'approved'); log('codex', 'approved', '');

  cap('5. IGRIS runs your check command in the shell. Exit 0 is a pass, whatever an agent says.', C.sh);
  await fly('pChk', C.cdx); pane(2, 'working', 10, 'npm test'); log('shell', 'npm test', 'your checks.command');
  await sleep(700); pane(2, 'working', 55, 'running ' + T.tests + ' tests'); await sleep(800);
  if (T.fail) {
    pane(2, 'failed', 100, 'exit 1'); log('shell', `exit 1 · ${T.fail}`, 'fix round 1 of 2');
    cap('The checks failed (exit 1). Claude Code fixes it and they run again (fix round 1 of 2).', C.cla);
    await sleep(600); await fly('pFixS', C.sh); pane(0, 'working', 60, 'updating snapshots');
    await sleep(1000); pane(0, 'done', 100, 'snapshots updated'); log('claude', 'updated the snapshots', 'fix round 1');
    await sleep(300); await fly('pChk', C.cdx); pane(2, 'working', 40, 'npm test'); log('shell', 'npm test', ''); await sleep(900);
  }
  pane(2, 'done', 100, T.tests + ' passed'); log('shell', `exit 0 · ${T.tests} passed`, 'exit 0');

  state('done'); cap('6. IGRIS reports the verdict and Hermes tells you. Merging is a separate step: igris merge.', C.her);
  await sleep(400); await fly('pRep', C.sh); on('bRep', 1);
  const reviews = T.changes ? 1 : 0, fixes = T.fail ? 1 : 0;
  $('repMsg').textContent = `approved · ${reviews} review ${reviews === 1 ? 'round' : 'rounds'} · ${fixes} fix ${fixes === 1 ? 'round' : 'rounds'} · ${files} · ${T.tests} tests passed · ready to merge`;
  log('igris', 'done: approved, ready to merge', 'exit 0'); log('hermes', 'tells you', ''); await sleep(1500);
  on('bYou', 1); $('youMsg').textContent = '» merge it'; log('you', 'merge it', ''); await sleep(700);
  log('hermes', 'igris merge', ''); await sleep(500);
  log('igris', 'merged into main (fast-forward only)', ''); $('repMsg').textContent = `merged into main · ${files} · ${T.tests} tests passed`;
  done++; $('hDone').textContent = done; on('bYou', 0);
  if (last) return;
  await sleep(3200);
}

// Fit: the pipeline is drawn at 800 x 850 and scaled to the page, never below 0.72 (it scrolls then).
const wrap = $('wrap'), scroll = $('scroll'), stage = $('stage');
function fit() {
  const s = Math.max(0.72, Math.min(1, wrap.clientWidth / 800));
  stage.style.transform = 'scale(' + s + ')'; scroll.style.height = Math.ceil(850 * s) + 'px';
}
new ResizeObserver(fit).observe(wrap); fit();

// ?task=0..3 picks the task for a still frame or a recording.
const pick = params.has('task') && TASKS[Number(params.get('task'))] ? Number(params.get('task')) : 1;
const flush = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };

(async () => {
  if (at !== null || reduce) {
    // Run the chosen task on the virtual clock: to `at` ms (the GIF frames), or to its end (reduced motion).
    let finished = false;
    runTask(TASKS[pick], true).then(() => { finished = true; document.body.dataset.end = String(Math.round(clock.t)); });
    while (!finished && (at === null || clock.t < at)) { tick(20); await flush(); }
    return;
  }
  // Live: one real frame advances the clock by the time that passed (never more than 100 ms).
  const order = TASKS.map((_, i) => i).sort(() => Math.random() - 0.5);
  (async () => { for (;;) for (const i of order) await runTask(TASKS[i]); })();
  let last;
  const frame = now => { tick(Math.min(100, now - (last ?? now))); last = now; requestAnimationFrame(frame); };
  requestAnimationFrame(frame);
})();
})();
