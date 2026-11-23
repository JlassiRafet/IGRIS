// One task through IGRIS, left to right. The tasks are examples; the steps, the four decision
// questions, the risk rules and the limits (2 review rounds, 2 fix rounds) are the real ones
// from crates/igris-core (decide.rs, orchestrate.rs) and examples/igris.example.toml.
(() => {
const $ = id => document.getElementById(id);
const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
const sleep = ms => (reduce ? Promise.resolve() : new Promise(r => setTimeout(r, ms)));

// The wordmark, decoded from the same half-block rows the terminal app uses (wordmark.rs).
const ROWS = ['▀█▀ █▀▀ █▀█ ▀█▀ █▀▀', '▄█▄ █▄█ █▀▄ ▄█▄ ▄▄█'];
const GLOW = [0xbc, 0xfa, 0xf1], ACC = [0x08, 0x79, 0xf3];
const wm = $('wm');
for (let r = 0; r < 4; r++) for (let c = 0; c < 19; c++) {
  const ch = ROWS[r >> 1][c], on = (r % 2 ? '▄█' : '▀█').includes(ch);
  const px = document.createElement('i');
  if (on) px.style.background = `rgb(${GLOW.map((g, k) => Math.round(g + (ACC[k] - g) * ((c / 18 + r / 3) / 2))).join()})`;
  wm.append(px);
}
const plume = document.createElement('b');
plume.textContent = 'ʅ'; plume.setAttribute('aria-hidden', 'true');
wm.append(plume);

// Marks: Simple Icons (CC0) for Telegram, Claude and MiniMax. OpenAI, Nous (Hermes) and Herdr have
// none there, so they get a monogram. Every mark is one solid colour.
const SI = {
  telegram: 'M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.48.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z',
  claude: 'm4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z',
  minimax: 'M11.43 3.92a.86.86 0 1 0-1.718 0v14.236a1.999 1.999 0 0 1-3.997 0V9.022a.86.86 0 1 0-1.718 0v3.87a1.999 1.999 0 0 1-3.997 0V11.49a.57.57 0 0 1 1.139 0v1.404a.86.86 0 0 0 1.719 0V9.022a1.999 1.999 0 0 1 3.997 0v9.134a.86.86 0 0 0 1.719 0V3.92a1.998 1.998 0 1 1 3.996 0v11.788a.57.57 0 1 1-1.139 0zm10.572 3.105a2 2 0 0 0-1.999 1.997v7.63a.86.86 0 0 1-1.718 0V3.923a1.999 1.999 0 0 0-3.997 0v16.16a.86.86 0 0 1-1.719 0V18.08a.57.57 0 1 0-1.138 0v2a1.998 1.998 0 0 0 3.996 0V3.92a.86.86 0 0 1 1.719 0v12.73a1.999 1.999 0 0 0 3.996 0V9.023a.86.86 0 1 1 1.72 0v6.686a.57.57 0 0 0 1.138 0V9.022a2 2 0 0 0-1.998-1.997'
};
const fill = d => `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="${d}"/></svg>`;
const line = d => `<svg class="ln" viewBox="0 0 24 24" aria-hidden="true"><path d="${d}"/></svg>`;
const mini = `<svg class="in" viewBox="0 0 24 24" aria-hidden="true"><path d="${SI.minimax}"/></svg>`;

const NODES = [
  { id: 'you', name: 'You', role: 'Telegram, Discord or the terminal', c: '#c9ced3', mark: fill(SI.telegram) },
  { id: 'hermes', name: 'Hermes', role: `Your assistant<br>${mini}MiniMax M3`, c: '#bcfaf1', mark: '<span>H</span>' },
  { id: 'decide', name: 'Decide', role: 'Picks agent, size and risk', c: '#8fd3f2', mark: line('M12 4v7m0 0-6 8m6-8 6 8') },
  { id: 'build', name: 'Build', role: 'Claude Code, in its own worktree', c: '#6bb8f0', mark: fill(SI.claude) },
  { id: 'review', name: 'Review', role: 'Codex, read-only', c: '#539fdf', mark: '<span>Cx</span>' },
  { id: 'checks', name: 'Checks', role: 'Your command; exit 0 is a pass', c: '#4393f4', mark: line('m5 8 4 4-4 4M12 17h7') },
  { id: 'report', name: 'Report', role: 'Verdict, rounds, files', c: '#c9ced3', mark: line('m5 12.5 4.5 4.5L19 7') }
];
const WHO = { you: '#c9ced3', hermes: '#bcfaf1', igris: '#e6e9ec', claude: '#6bb8f0', codex: '#539fdf', shell: '#4393f4' };

// Build the rail: seven steps and six wires, the three Herdr panes grouped.
const rail = $('rail'), els = {}, wires = [];
const node = n => {
  const e = document.createElement('div');
  e.className = 'node'; e.style.setProperty('--c', n.c);
  e.innerHTML = `<span class="mark">${n.mark}</span><b>${n.name}</b><small>${n.role}</small><i class="st">idle</i>`;
  els[n.id] = { e, st: e.querySelector('.st') };
  return e;
};
const wire = i => { const w = document.createElement('div'); w.className = 'wire'; wires[i] = w; return w; };
const [you, hermes, decide, build, review, checks, report] = NODES.map(node);
const herdr = document.createElement('div');
herdr.className = 'herdr';
herdr.innerHTML = '<span>Herdr · one pane per agent</span>';
herdr.append(build, wire(3), review, wire(4), checks);
rail.append(you, wire(0), hermes, wire(1), decide, wire(2), herdr, wire(5), report);

const STATE = { idle: ['idle', ''], run: ['● Running', ''], ok: ['✓ Done', 'ok'], ask: ['? Needs approval', 'ask'], bad: ['✕ Failed', 'bad'] };
function setNode(id, state, text) {
  const o = els[id];
  o.e.classList.toggle('on', state === 'run');
  o.st.className = 'st ' + STATE[state][1];
  o.st.textContent = text || STATE[state][0];
}
function chip(text, cls) { const c = $('run'); c.textContent = text; c.className = 'chip ' + (cls || ''); }
async function send(i) {
  wires[i].classList.add('lit');
  if (reduce) return;
  const p = document.createElement('i');
  p.className = 'packet'; wires[i].append(p);
  await p.animate([{ left: '0' }, { left: 'calc(100% - 8px)' }], { duration: 480, easing: 'ease-in-out' }).finished;
  p.remove();
}
const say = s => { $('live').textContent = s; };

// The decision panel: the four questions the rules answer (decide.rs).
const Q = ['agent', 'tier', 'risk', 'needs_approval'];
function resetDecision() {
  $('decision').innerHTML = Q.map(q => `<div class="row"><span>${q}</span><b>…</b><em></em></div>`).join('');
}
function answer(i, value, hi) {
  const r = $('decision').children[i];
  r.querySelector('b').textContent = value;
  r.querySelector('em').textContent = '1.00';
  r.classList.toggle('hi', !!hi);
}

// The session log: the last eight lines, newest at the bottom.
let lines = [], clock = 0;
function log(who, msg) {
  clock += 1 + ((lines.length * 5 + msg.length) % 3);
  lines.push({ t: `${Math.floor(clock / 60)}:${String(clock % 60).padStart(2, '0')}`, who, msg });
  lines = lines.slice(-8);
  $('log').innerHTML = lines.map((l, i) =>
    `<div style="opacity:${(0.5 + 0.5 * (i + 1) / lines.length).toFixed(2)}"><time>${l.t}</time><span class="w" style="color:${WHO[l.who]}">${l.who}</span><span class="m"></span></div>`).join('');
  [...$('log').children].forEach((d, i) => { d.querySelector('.m').textContent = lines[i].msg; });
}

// The result panel.
const R0 = { verdict: '—', review: '0 of 2 review rounds', fix: '0 of 2 fix rounds', checks: '—', files: '—' };
const R = { ...R0 };
function result(patch, hint) {
  Object.assign(R, patch);
  $('result').innerHTML = `<dl class="res"><dt>Verdict</dt><dd>${R.verdict}</dd><dt>Review</dt><dd>${R.review}</dd><dt>Checks</dt><dd>${R.checks}</dd><dt>Fixes</dt><dd>${R.fix}</dd><dt>Files</dt><dd></dd></dl>${hint ? `<p class="hint">${hint}</p>` : ''}`;
  $('result').querySelector('dd:last-of-type').textContent = R.files;
}

// Example tasks. "tier" is small for short fix, rename, typo or bump tasks (decide.rs). Risk is
// high when the task has a risky word (migration, deploy, secret, auth, delete, drop) or touches
// a risky path (.env*, **/*.sql, **/migrations/**, **/auth/**).
const TASKS = [
  { msg: 'fix the failing cart test', tier: 'small', files: ['src/cart.ts'], tests: 57,
    note: 'edits src/cart.ts' },
  { msg: 'add rate limiting to the api', tier: 'mid', files: ['src/api/limiter.ts', 'src/api/routes.ts', 'test/limiter.test.ts'], tests: 84,
    changes: 'no test for the 429 case', note: 'edits 3 files' },
  { msg: 'add a users table migration', tier: 'mid', risky: 'risky word: migration', files: ['db/migrations/014_users.sql'], tests: 61,
    note: 'writes 014_users.sql' },
  { msg: 'bump axios to 1.7', tier: 'small', files: ['package.json', 'package-lock.json'], tests: 112,
    fail: '3 snapshot tests changed', note: 'edits package.json' }
];

async function runTask(T) {
  clock = 0; lines = [];
  ['you', 'hermes', 'decide', 'build', 'review', 'checks', 'report'].forEach(id => setNode(id, 'idle'));
  wires.forEach(w => w.classList.remove('lit'));
  $('log').innerHTML = ''; resetDecision(); result(R0);
  $('task').textContent = T.msg;
  const high = !!T.risky;
  let fixes = 0, reviews = 0;

  chip('Running', 'run'); say(`You ask: ${T.msg}`);
  setNode('you', 'run'); log('you', `"${T.msg}"`); await sleep(700);
  await send(0); setNode('you', 'ok', '✓ Sent');
  setNode('hermes', 'run'); log('hermes', `igris run --task "${T.msg}"`); await sleep(500);
  await send(1); setNode('hermes', 'ok', '✓ Handed over');

  say('IGRIS decides which agent, how big the task is, and how risky.');
  setNode('decide', 'run');
  const out = [['claude', false], [T.tier, false], [high ? 'high' : 'low', high], [high ? 'yes' : 'no', high]];
  for (let i = 0; i < 4; i++) { answer(i, out[i][0], out[i][1]); await sleep(420); }
  log('igris', `agent=claude tier=${T.tier} risk=${high ? 'high' : 'low'} needs_approval=${high ? 'yes' : 'no'}`);
  if (high) {
    setNode('decide', 'ask'); chip('Needs approval', 'ask'); say('The task looks risky, so it waits for your yes.');
    log('igris', `exit 3 · needs approval (${T.risky})`); await sleep(1100);
    log('you', 'y'); await sleep(500);
    log('hermes', `igris run --approved --task "${T.msg}"`); chip('Running', 'run'); await sleep(500);
  }
  setNode('decide', 'ok', high ? '✓ Approved' : '✓ Decided'); await sleep(300);

  say('The builder writes the code in its own worktree.');
  await send(2); setNode('build', 'run'); log('claude', `${T.note} in its worktree`); await sleep(1200);
  setNode('build', 'ok');

  say('A different agent reads the diff and approves it or asks for changes.');
  await send(3); setNode('review', 'run');
  log('codex', 'reading the diff, read-only'); await sleep(900);
  if (T.changes) {
    reviews++; result({ review: `${reviews} of 2 review rounds` });
    log('codex', `changes requested: ${T.changes}`); setNode('review', 'bad', '✕ Changes asked'); await sleep(700);
    setNode('build', 'run'); log('claude', `review round ${reviews}: adding the missing test`); await sleep(1000);
    setNode('build', 'ok'); setNode('review', 'run'); log('codex', 're-reading the diff'); await sleep(800);
  }
  log('codex', 'approved'); setNode('review', 'ok'); result({ review: `${reviews} of 2 review rounds` });

  say('Your own command runs in the shell. Exit 0 means done.');
  await send(4); setNode('checks', 'run'); log('shell', 'npm test'); await sleep(900);
  if (T.fail) {
    fixes++; setNode('checks', 'bad', '✕ Exit 1'); result({ checks: 'exit 1', fix: `${fixes} of 2 fix rounds` });
    log('shell', `exit 1 · ${T.fail}`); await sleep(800);
    setNode('build', 'run'); log('claude', `fix round ${fixes}: updating the snapshots`); await sleep(1000);
    setNode('build', 'ok'); setNode('checks', 'run'); log('shell', 'npm test'); await sleep(900);
  }
  log('shell', `exit 0 · ${T.tests} passed`); setNode('checks', 'ok');
  result({ checks: `exit 0 · ${T.tests} passed`, fix: `${fixes} of 2 fix rounds` });

  say('IGRIS reports the verdict. Merging waits for you.');
  await send(5); setNode('report', 'ok', '✓ Approved');
  result({ verdict: 'approved', files: T.files.join(', ') }, 'Done. Merge it with <code>igris merge</code> (fast-forward only).');
  chip('Done', 'ok'); log('igris', 'done · approved · ready to merge'); await sleep(1500);
  log('you', 'igris merge'); log('igris', 'merged into main (fast-forward)'); chip('Merged', 'ok');
  result({}, 'Merged into <code>main</code>.');
  await sleep(3200);
}

// Still frame: with reduced motion the page shows one finished task and stops.
// ?task=0..3 picks it (the README screenshot uses the review-and-fix one).
const pick = Number(new URLSearchParams(location.search).get('task'));
(async () => {
  if (reduce) { await runTask(TASKS[pick >= 0 && pick < TASKS.length && location.search.includes('task') ? pick : 1]); return; }
  const order = TASKS.map((_, i) => i).sort(() => Math.random() - 0.5);
  for (;;) for (const i of order) await runTask(TASKS[i]);
})();
})();
