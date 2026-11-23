// One task through IGRIS, drawn as a terminal. The tasks are examples; the steps, the four decision
// questions, the risk rules and the limits (2 review rounds, 2 fix rounds) are the real ones from
// crates/igris-core (decide.rs, orchestrate.rs) and examples/igris.example.toml.
(() => {
const $ = id => document.getElementById(id);
const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
const sleep = ms => (reduce ? Promise.resolve() : new Promise(r => setTimeout(r, ms)));

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

// Marks from Simple Icons (CC0): Telegram, Claude, MiniMax. OpenAI, Nous, Jev and Herdr have none
// there, so they keep the pixel icons. Every mark is one solid colour.
const SI = {
  'd-tg': 'M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.48.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z',
  'd-cla': 'm4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z',
  'd-mm': 'M11.43 3.92a.86.86 0 1 0-1.718 0v14.236a1.999 1.999 0 0 1-3.997 0V9.022a.86.86 0 1 0-1.718 0v3.87a1.999 1.999 0 0 1-3.997 0V11.49a.57.57 0 0 1 1.139 0v1.404a.86.86 0 0 0 1.719 0V9.022a1.999 1.999 0 0 1 3.997 0v9.134a.86.86 0 0 0 1.719 0V3.92a1.998 1.998 0 1 1 3.996 0v11.788a.57.57 0 1 1-1.139 0zm10.572 3.105a2 2 0 0 0-1.999 1.997v7.63a.86.86 0 0 1-1.718 0V3.923a1.999 1.999 0 0 0-3.997 0v16.16a.86.86 0 0 1-1.719 0V18.08a.57.57 0 1 0-1.138 0v2a1.998 1.998 0 0 0 3.996 0V3.92a.86.86 0 0 1 1.719 0v12.73a1.999 1.999 0 0 0 3.996 0V9.023a.86.86 0 1 1 1.72 0v6.686a.57.57 0 0 0 1.138 0V9.022a2 2 0 0 0-1.998-1.997'
};
for (const [id, d] of Object.entries(SI)) $(id).setAttribute('d', d);

const C = { you: '#e6e9ec', her: '#9be8d8', jev: '#ec7fb3', cla: '#e3875c', cdx: '#6aa7f2', hrd: '#a995f0', sh: '#c9ced3' };
const WHO = { you: C.you, hermes: C.her, igris: C.jev, claude: C.cla, codex: C.cdx, shell: C.sh, herdr: C.hrd };
// A pane's state dot: Running, Done, Failed (the task statuses of orchestrate.rs) or idle.
const ST = { idle: ['#3a4650', 'idle'], working: ['#f2c14e', 'Running'], done: ['#bcfaf1', 'Done'], failed: ['#ff3038', 'Failed'] };
const NS = 'http://www.w3.org/2000/svg';

// A short tail of pixels runs along a wire: the task moving from one part to the next.
function fly(id, col) {
  if (reduce) return Promise.resolve();
  const p = $(id), L = p.getTotalLength(), g = $('dots'), dur = Math.max(450, L * 4.2);
  const cs = [5, 4, 3, 2].map((s, i) => {
    const c = document.createElementNS(NS, 'rect');
    c.setAttribute('width', s); c.setAttribute('height', s); c.setAttribute('fill', col); c.setAttribute('opacity', 1 - i * 0.24);
    g.appendChild(c); return [c, s];
  });
  return new Promise(res => {
    const t0 = performance.now();
    const f = now => {
      const t = now - t0;
      cs.forEach(([c, s], i) => {
        const d = Math.max(0, Math.min(L, t / dur * L - i * 8)), pt = p.getPointAtLength(d);
        c.setAttribute('x', Math.round(pt.x - s / 2)); c.setAttribute('y', Math.round(pt.y - s / 2));
      });
      if (t < dur) requestAnimationFrame(f); else { cs.forEach(([c]) => c.remove()); res(); }
    };
    requestAnimationFrame(f);
  });
}
const on = (id, v) => $(id).classList.toggle('on', !!v);
const mark = i => { for (let k = 0; k < 5; k++) { const m = $('m' + k); m.classList.toggle('on', k === i); m.classList.toggle('done', i >= 0 && k < i); } };
const cap = (s, col) => { $('cap').textContent = s; if (col) $('hex').style.background = col; };
const status = s => { $('sH').textContent = '[' + s + ']'; };

let lines = [], clk = 0;
function log(who, msg, tag) {
  clk += 1 + ((lines.length * 5 + msg.length) % 3);
  lines.push({ t: `0:${String(clk).padStart(2, '0')}`, who, msg, tag: tag || '' });
  lines = lines.slice(-5);
  $('log').innerHTML = lines.map((l, i) =>
    `<div style="opacity:${(0.45 + 0.55 * (i + 1) / lines.length).toFixed(2)}"><span class="d"></span><b style="color:${WHO[l.who]}">${l.who}</b><span class="ell"></span><span class="d ell" style="text-align:right"></span></div>`).join('');
  [...$('log').children].forEach((d, i) => {
    const s = d.querySelectorAll('span'); s[0].textContent = lines[i].t; s[1].textContent = lines[i].msg; s[2].textContent = lines[i].tag;
  });
}

// The decision panel: the four questions the rules answer (decide.rs), each with its confidence.
function setRow(i, v, flag) {
  const r = $('v' + i); r.textContent = v; r.style.color = flag ? '#ff8a8f' : '';
  $('b' + i).style.width = '100%'; $('p' + i).textContent = '1.00';
}
const resetRows = () => { for (let i = 0; i < 4; i++) { $('v' + i).textContent = '…'; $('v' + i).style.color = ''; $('b' + i).style.width = '0'; $('p' + i).textContent = '—'; } };

const panes = ['idle', 'idle', 'idle'];
function pane(i, st, pct, act) {
  panes[i] = st; const [c, w] = ST[st];
  $('pb' + i).style.width = pct + '%'; $('pp' + i).textContent = pct + '%'; if (act) $('pa' + i).textContent = act;
  $('pd' + i).style.background = c; $('sd' + i).style.background = c; $('ps' + i).textContent = w; $('ps' + i).style.color = st === 'idle' ? '' : c;
  on('pn' + i, st === 'working');
  const roll = panes.includes('failed') ? 'failed' : panes.includes('working') ? 'working' : panes.includes('done') ? 'done' : 'idle';
  on('bHrd', roll === 'working'); $('sApp').style.background = ST[roll][0];
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

async function runTask(T) {
  clk = 0; lines = []; $('log').innerHTML = '';
  mark(-1); resetRows();
  for (let i = 0; i < 3; i++) pane(i, 'idle', 0, 'waiting');
  ['bYou', 'bHer', 'bDec', 'bRep'].forEach(id => on(id, 0));
  $('repMsg').textContent = 'nothing to approve';
  const high = !!T.risky, cmd = `igris run --task "${T.msg}"`;
  $('cmd').textContent = 'igris run';

  mark(0); on('bYou', 1); $('youMsg').textContent = '» ' + T.msg; $('hTask').textContent = '» ' + T.msg; status('reading');
  cap(`1. You message Hermes: "${T.msg}".`, C.you); log('you', `"${T.msg}"`, 'telegram');
  await fly('pYou', C.you); on('bYou', 0); on('bHer', 1); await sleep(450);
  $('cmd').textContent = cmd; log('hermes', cmd, ''); await sleep(350);

  mark(1); on('bDec', 1); status('deciding');
  cap('2. IGRIS decides: which agent, how big, how risky, and whether it needs your yes.', C.jev);
  await fly('pDec', C.her);
  const out = [['claude'], [T.tier], [high ? 'high' : 'low', high], [high ? 'yes' : 'no', high]];
  for (let i = 0; i < 4; i++) { setRow(i, ...out[i]); await sleep(480); }
  log('igris', `agent=claude tier=${T.tier} risk=${high ? 'high' : 'low'} needs_approval=${high ? 'yes' : 'no'}`, 'rules 1.00');
  if (high) {
    cap(`The task looks risky (${T.risky}), so Hermes asks you before anything runs.`, C.her);
    log('igris', `exit 3 · needs approval (${T.risky})`, 'exit 3');
    on('bDec', 0); on('bYou', 1); $('youMsg').textContent = '» approve the run? y/n';
    await fly('pYouB', C.her); await sleep(1300);
    $('youMsg').textContent = '» y'; log('you', 'y', 'approved'); await fly('pYou', C.you); on('bYou', 0);
    $('cmd').textContent = cmd + ' --approved'; log('hermes', 'igris run --approved', '');
    on('bDec', 1); await sleep(300);
  }
  on('bDec', 0);

  mark(2); status('dispatching'); on('bHer', 1);
  cap('3. IGRIS starts Claude Code in its own worktree, in a Herdr pane.', C.cla);
  log('igris', 'build: claude · worktree', '');
  await fly('pBld', C.jev); pane(0, 'working', 5, 'starting');
  for (let k = 0; k < 3; k++) { await sleep(750); pane(0, 'working', (k + 1) * 28, T.acts[k]); }
  await sleep(500); pane(0, 'done', 100, T.files.length + (T.files.length > 1 ? ' files changed' : ' file changed'));
  log('claude', T.files.length + (T.files.length > 1 ? ' files changed' : ' file changed') + ' · ' + T.files[0], '');

  cap('4. Codex reads the diff, read-only, and approves it or asks for changes.', C.cdx);
  await fly('pRev', C.cla); pane(1, 'working', 25, 'reading the diff'); log('codex', 'reading the diff', 'read-only');
  await sleep(800); pane(1, 'working', 65, 'checking the tests');
  await sleep(700);
  if (T.changes) {
    pane(1, 'done', 100, 'changes asked'); log('codex', `changes requested: ${T.changes}`, 'review round 1 of 2');
    cap('Codex asked for changes. Claude Code fixes them before anything else runs (round 1 of 2).', C.cla);
    await fly('pFix', C.cdx); pane(0, 'working', 80, 'adding the test');
    await sleep(900); pane(0, 'done', 100, 'test added'); log('claude', 'added the missing test', 'review fix');
    await fly('pRev', C.cla); pane(1, 'working', 50, 're-reading the diff'); await sleep(700);
  }
  pane(1, 'done', 100, 'approved'); log('codex', 'approved', '');

  mark(3); status('verifying');
  cap('5. The shell runs your check command. Exit 0 means a pass, whatever an agent says.', C.sh);
  await fly('pChk', C.cdx); pane(2, 'working', 10, 'npm test'); log('shell', 'npm test', '');
  await sleep(700); pane(2, 'working', 55, 'running ' + T.tests + ' tests'); await sleep(800);
  if (T.fail) {
    pane(2, 'failed', 100, 'exit 1'); log('shell', `exit 1 · ${T.fail}`, 'fix round 1 of 2');
    cap('The checks failed (exit 1). Claude Code fixes it and they run again (fix round 1 of 2).', C.cla);
    await sleep(500); await fly('pFixS', C.sh); pane(0, 'working', 60, 'updating snapshots');
    await sleep(1000); pane(0, 'done', 100, 'snapshots updated'); log('claude', 'updated the snapshots', 'fix round 1');
    await fly('pChk', C.cdx); pane(2, 'working', 40, 'npm test'); log('shell', 'npm test', ''); await sleep(900);
  }
  pane(2, 'done', 100, T.tests + ' passed'); log('shell', `exit 0 · ${T.tests} passed`, 'exit 0');

  mark(4); status('reporting');
  cap('6. Hermes reports back. Merging waits for your yes: igris merge.', C.her);
  await fly('pRep', C.sh); on('bRep', 1);
  $('repMsg').textContent = `${T.files.length} file${T.files.length > 1 ? 's' : ''} · ${T.tests} passed · approved. merge with igris merge`;
  log('hermes', 'report sent · waiting for you', ''); await sleep(1600);
  on('bYou', 1); $('youMsg').textContent = '» igris merge'; log('you', 'igris merge', ''); await sleep(900);
  log('igris', 'merged into main (fast-forward)', ''); $('repMsg').textContent = 'merged into main ✓';
  done++; $('hDone').textContent = done; on('bYou', 0);
  await sleep(2800);
}

// Fit: the terminal is drawn at 1400 x 624 and scaled to the page, never below 0.72 (it scrolls then).
const wrap = $('wrap'), scroll = $('scroll'), stage = $('stage');
function fit() {
  const s = Math.max(0.72, Math.min(1, wrap.clientWidth / 1400));
  stage.style.transform = 'scale(' + s + ')'; scroll.style.height = Math.ceil(624 * s) + 'px';
}
new ResizeObserver(fit).observe(wrap); fit();

// Still frame: with reduced motion the page shows one finished task and stops. ?task=0..3 picks it.
const q = new URLSearchParams(location.search).get('task');
(async () => {
  if (reduce) { await runTask(TASKS[q !== null && TASKS[Number(q)] ? Number(q) : 1]); return; }
  const order = TASKS.map((_, i) => i).sort(() => Math.random() - 0.5);
  for (;;) for (const i of order) await runTask(TASKS[i]);
})();
})();
