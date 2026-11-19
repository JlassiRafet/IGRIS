
{
const $=id=>document.getElementById(id);
const C={you:'var(--you)',her:'var(--her)',jev:'var(--jev)',cla:'var(--cla)',cdx:'var(--cdx)',hrd:'var(--hrd)',sh:'var(--sh)'};
const tx=c=>`color-mix(in oklab,${c} var(--tmix),var(--tk))`;
const SPLIT='var(--split)';
const ST={idle:['var(--st-idle)','idle'],working:['var(--st-work)','working'],blocked:['var(--st-block)','blocked'],done:['var(--st-done)','done']};
const AG={claude:['Claude Code','claude','#ff6a2b',['haiku','sonnet','opus']],codex:['Codex','codex','#35d0ff',['small','mid','large']],
 gemini:['Gemini CLI','gemini','#c6f432',['flash-lite','flash','pro']],opencode:['OpenCode','opencode','#f0f0f0',['small','mid','large']]};
const DL={laya:['Laya','local'],
 jev:['Jev','hosted']};
const CH={telegram:'Telegram',discord:'Discord',slack:'Slack',whatsapp:'WhatsApp',signal:'Signal',email:'Email',matrix:'Matrix',teams:'Microsoft Teams',sms:'SMS',cli:'Terminal'};
const DEF={ch:'telegram',dl:'laya',b:'claude',r:'codex'},MAP={ch:CH,dl:DL,b:AG,r:AG};
let cfg={...DEF};try{Object.assign(cfg,JSON.parse(localStorage.getItem('aop-cfg')||'{}'))}catch(e){}
for(const k in DEF)if(!Object.hasOwn(MAP[k],cfg[k]))cfg[k]=DEF[k];
const N={};
function applyCfg(){
 const b=AG[cfg.b],r=AG[cfg.r],d=DL[cfg.dl];
 Object.assign(N,{B:b[0],b:b[1],bl:b[0].toLowerCase(),BU:b[0].toUpperCase(),R:r[0],r:r[1],rl:r[0].toLowerCase(),RU:r[0].toUpperCase(),
  D:d[0],d:d[0].toLowerCase(),DU:d[0].toUpperCase(),dh:d[1],CH:CH[cfg.ch],ch:CH[cfg.ch].toLowerCase()});
 document.documentElement.style.setProperty('--cla',b[2]);document.documentElement.style.setProperty('--cdx',r[2]);
 document.querySelectorAll('[data-n]').forEach(e=>e.textContent=N[e.dataset.n]);
 try{localStorage.setItem('aop-cfg',JSON.stringify(cfg))}catch(e){}}
[['selCh','ch',CH,v=>v],['selDl','dl',DL,v=>v[0]+' ('+v[1]+')'],['selB','b',AG,v=>v[0]],['selR','r',AG,v=>v[0]]].forEach(([id,k,M,lab])=>{
 const el=$(id);el.innerHTML=Object.entries(M).map(([key,v])=>`<option value="${key}">${lab(v)}</option>`).join('');el.value=cfg[k];
 el.onchange=()=>{cfg[k]=el.value;applyCfg()}});
applyCfg();
const INFO={
 you:['You','Message Hermes from {CH}, any other chat app Hermes supports (Telegram, Discord, Slack, WhatsApp, Signal, Email, Matrix, Teams, SMS and more), or the terminal. You only step in to approve risky actions, merges and deploys.','var(--you)'],
 hermes:['Hermes · assistant','Takes your request, splits it into tasks, asks {D} for quick calls, drives the agents through the Herdr CLI, checks results in the shell and reports back. Use any cheap model you like: it only plans and summarizes.','var(--her)'],
 jev:['{D} · decision layer','Answers typed questions (which agent, which model tier, how risky, needs approval?) with a confidence score. Sharp answers run right away; split answers go to a stronger model or to you. Hermes and the agents can both ask it. It never writes code.','var(--jev)'],
 herdr:['Herdr · workspaces','One terminal with a pane per agent, each in its own git worktree. The sidebar shows each one: yellow working, red blocked, blue done. Hermes drives it through its CLI, sessions survive closing the terminal, and it can reach other machines over SSH.','var(--hrd)'],
 claude:['{B} · builder','Plans and writes the code in its own worktree, runs it, and fixes what {R} finds. Any agent Herdr supports can take this role.','var(--cla)'],
 codex:['{R} · reviewer','The second opinion. Reviews the diff read-only and flags bugs before anything merges. It never edits the code itself.','var(--cdx)'],
 shell:['Shell · judge','Runs tests, lint and git. Hermes trusts this output, not an agent saying "tests passed".','var(--sh)'],
 report:['Report','Hermes sends you what changed, the test results and the review verdict on {CH}. Merge and deploy wait for your yes.','var(--her)'],
 log:['Session log','Every decision and handoff, with {D}\'s confidence. This is how you audit what the team did, and it becomes training data for the decision layer.','#8c8c8c'],
 status:['Status bar','Live state of each part and the running count of {D} decisions.','#8c8c8c']
};
const T=[
 {msg:'add a login page',agent:.93,tier:[1,.88],risk:['low',.86],file:'src/auth.ts',files:3,tests:42,issue:true,cost:.012,acts:['reading src/','editing auth.ts','writing tests']},
 {msg:'fix the failing cart test',agent:.95,tier:[0,.91],risk:['low',.92],file:'src/cart.ts',files:1,tests:57,cost:.004,acts:['reading test log','editing cart.ts','rerun one test']},
 {msg:'run the db migration',agent:.9,tier:[1,.84],risk:['high',.94],high:true,file:'db/014.sql',files:2,tests:61,cost:.015,acts:['reading schema','writing 014.sql','dry run']},
 {msg:'rename props in settings ui',agent:.88,tier:[0,.55],risk:['low',.9],file:'ui/settings.tsx',files:4,tests:61,cost:.006,acts:['finding usages','renaming props','fixing imports']}
];
let paused=false,speed=1,decisions=1204,done=0,cost=0,clk=new Date();clk.setHours(23,48,10);
const reduce=matchMedia('(prefers-reduced-motion: reduce)').matches;
function sleep(ms){return new Promise(r=>{let left=ms/speed,last=performance.now();const f=now=>{if(!paused)left-=now-last;last=now;left<=0?r():requestAnimationFrame(f)};requestAnimationFrame(f)})}
const NS='http://www.w3.org/2000/svg';
function fly(id,col){
 if(reduce)return sleep(150);
 const p=$(id),L=p.getTotalLength(),g=$('dots'),dur=Math.max(450,L*4.2);
 const cs=[7,5,4,3].map((s,i)=>{const c=document.createElementNS(NS,'rect');c.setAttribute('width',s);c.setAttribute('height',s);c.setAttribute('rx',s/2);c.style.fill=col;c.setAttribute('opacity',1-i*.24);g.appendChild(c);return[c,s]});
 return new Promise(res=>{let t=0,last=performance.now();const f=now=>{if(!paused)t+=(now-last)*speed;last=now;
  cs.forEach(([c,s],i)=>{const d=Math.max(0,Math.min(L,t/dur*L-i*8)),pt=p.getPointAtLength(d);c.setAttribute('x',Math.round(pt.x-s/2));c.setAttribute('y',Math.round(pt.y-s/2))});
  if(t<dur)requestAnimationFrame(f);else{cs.forEach(([c])=>c.remove());res()}};requestAnimationFrame(f)})}
const on=(id,v)=>$(id).classList.toggle('on',!!v);
function mark(i){for(let k=0;k<5;k++){const m=$('m'+k);m.classList.toggle('on',k===i);m.classList.toggle('done',i>=0&&k<i)}}
const cap=s=>$('cap').textContent=s;
const tag=p=>p>=.8?'sharp':'split';
let lines=[];
function log(who,msg,r){
 clk=new Date(clk.getTime()+1000+Math.random()*2000);
 const R={B:[N.bl,C.cla],R:[N.rl,C.cdx],D:[N.d,C.jev]}[who]||[who,{you:C.you,hermes:C.her,shell:C.sh,herdr:C.hrd}[who]];
 lines.push([clk.toTimeString().slice(0,8),R[0],msg,r||'',R[1]]);lines=lines.slice(-5);
 $('log').innerHTML=lines.map((l,i)=>`<div style="opacity:${.45+i*.14}"><span class="d">${l[0]}</span><b style="color:${tx(l[4])}">${l[1]}</b><span class="ell">${l[2]}</span><span class="d ell" style="text-align:right">${l[3]}</span></div>`).join('')}
function setRow(i,v,p){$('v'+i).textContent=v;const b=$('b'+i);b.style.width=Math.round(p*100)+'%';b.style.background=p>=.8?C.jev:SPLIT;
 $('p'+i).textContent=p.toFixed(2);$('g'+i).textContent=tag(p);$('g'+i).style.color=tx(p>=.8?C.jev:SPLIT)}
function resetJev(){for(let i=0;i<3;i++){$('v'+i).textContent='…';$('b'+i).style.width='0';$('p'+i).textContent='—';$('g'+i).textContent=''}}
const panes=['idle','idle','idle'];
function pane(i,st,pct,act){panes[i]=st;const[c,w]=ST[st];
 $('pb'+i).style.width=pct+'%';$('pp'+i).textContent=pct+'%';if(act)$('pa'+i).textContent=act;
 $('pd'+i).style.background=c;$('sd'+i).style.background=c;$('ps'+i).textContent=w;$('ps'+i).style.color=tx(c);on('pn'+i,st==='working'||st==='blocked');
 const roll=panes.includes('blocked')?'blocked':panes.includes('done')?'done':panes.includes('working')?'working':'idle';
 $('sApp').style.background=ST[roll][0];$('sR').textContent='3 panes · '+ST[roll][1];$('sR').style.color=tx(roll==='idle'?C.hrd:ST[roll][0]);
 $('sC').textContent=panes[1]==='working'?'reviewing':panes[1]}
const status=s=>$('sH').textContent=s;
function bump(){decisions++;$('jN').textContent=decisions.toLocaleString('en');$('sJ').textContent=decisions.toLocaleString('en')+' decisions'}

async function task(t){
 const tier=AG[cfg.b][3][t.tier[0]];
 mark(0);on('bYou',1);$('youMsg').textContent='» '+t.msg;$('hTask').textContent='» '+t.msg;status('reading');
 cap('1. You message Hermes on '+N.CH+': "'+t.msg+'".');log('you','"'+t.msg+'"',N.ch);
 await fly('pYou',C.you);on('bYou',0);on('bHer',1);await sleep(500);
 mark(1);on('bJev',1);resetJev();cap('2. Hermes asks '+N.D+' three quick questions.');log('hermes','asks '+N.d+': agent · tier · risk','3 questions');
 await fly('pJev',C.her);
 const rows=[['which agent',[N.bl,t.agent]],['model tier',[tier,t.tier[1]]],['risk',t.risk]];
 for(let i=0;i<3;i++){const[l,[v,p]]=rows[i];setRow(i,v,p);bump();
  log('D',l+' → '+v,'p='+p.toFixed(2)+' '+tag(p)+(p>=.8?' → act':' → escalate'));await sleep(700)}
 if(t.tier[1]<.8){cap('Split answer: Hermes double-checks with a stronger model before acting.');
  log('hermes','split on tier → asks a stronger model','escalate');await sleep(900);log('hermes','stronger model: '+tier+' is enough','→ '+tier);await sleep(600)}
 on('bJev',0);mark(2);status('dispatching');$('tierLbl').textContent=tier;
 cap('3. Hermes opens a Herdr pane and hands the task to '+N.B+' ('+tier+').');log('hermes','herdr agent prompt builder',tier);
 await fly('pD1',C.her);pane(0,'working',5,'starting');
 for(let k=0;k<3;k++){await sleep(750);pane(0,'working',(k+1)*28,t.acts[k]);
  if(t.high&&k===1){
   cap(N.B+' reaches a risky step and asks '+N.D+': does this need your approval?');log('B','asks '+N.d+': needs approval?','mid-task');
   on('bJev',1);resetJev();$('l2').textContent='approval?';await fly('pAsk',C.cla);setRow(2,'yes',.94);bump();log('D','needs approval → yes','p=0.94 sharp');
   await sleep(700);await fly('pD1',C.jev);on('bJev',0);$('l2').textContent='risk';
   pane(0,'blocked',56,'needs approval');log('herdr',N.b+' blocked · waiting for you','red');
   cap('The pane turns red and Hermes asks you on '+N.CH+' before going on.');
   on('bYou',1);$('youMsg').textContent='» run migration on staging? y';
   await fly('pYouB',C.her);await sleep(1200);log('you','approved: staging only','y');on('bYou',0);pane(0,'working',60,'migrating staging')}}
 await sleep(600);pane(0,'done',100,t.files+' files changed');log('B',t.files+' files changed · '+t.file,'');
 cap('4. Hermes hands the diff to '+N.R+' for a read-only review.');log('hermes','herdr agent prompt reviewer','review diff');
 await fly('pCX',C.cla);pane(1,'working',25,'reading task');await sleep(700);pane(1,'working',65,'reading diff');await sleep(700);
 if(t.issue){pane(1,'done',100,'1 issue found');log('R','1 issue: missing input check','fix it');
  cap(N.R+' found a bug. '+N.B+' fixes it before anything merges.');await fly('pXC',C.cdx);pane(0,'working',80,'fixing issue');
  await sleep(900);pane(0,'done',100,'issue fixed');log('B','fixed · ready for re-review','');await fly('pCX',C.cla);pane(1,'working',50,'re-review');await sleep(700)}
 pane(1,'done',100,'approved');log('R','review passed · 0 issues','');
 mark(3);status('verifying');cap('5. Hermes runs the tests itself. The shell decides what "done" means.');
 await fly('pVer',C.her);pane(2,'working',10,'npm test');log('hermes','shell: npm test','');
 await sleep(600);pane(2,'working',55,'running '+t.tests);await sleep(800);pane(2,'done',100,t.tests+' passed');log('shell',t.tests+' passed · 0 failed','exit 0');
 mark(4);status('reporting');cap('6. Hermes reports back on '+N.CH+'. Merge and deploy wait for your yes.');
 await Promise.all([fly('pM1',C.cla),fly('pM2',C.cdx),fly('pM3',C.sh)]);on('bRep',1);
 $('repMsg').textContent=t.files+' files · '+t.tests+' tests · merge? [y/n]';log('hermes','report sent · waiting for you','');
 await fly('pRep',C.her);await sleep(1300);$('repMsg').textContent='you said yes · merged ✓';log('you','approved merge','y');
 done++;cost+=t.cost;$('hDone').textContent=done;$('hCost').textContent='$'+cost.toFixed(3);
 await sleep(1500);on('bRep',0);on('bHer',0);mark(-1);status('listening');$('repMsg').textContent='nothing to approve';
 for(let i=0;i<3;i++)pane(i,'idle',0,'waiting');await sleep(600)}
(async()=>{for(;;)for(const t of T)await task(t)})();

const root=document.documentElement,mq=matchMedia('(prefers-color-scheme: dark)');
let theme=null;try{theme=localStorage.getItem('igris-theme')}catch(e){}
const eff=()=>theme||(mq.matches?'dark':'light');
const IC={pause:'<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="3" width="2.6" height="10" rx="1.1" fill="currentColor"/><rect x="9.4" y="3" width="2.6" height="10" rx="1.1" fill="currentColor"/></svg>',
 play:'<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.2v9.6a.8.8 0 0 0 1.2.7l7.6-4.8a.8.8 0 0 0 0-1.4L6.2 2.5A.8.8 0 0 0 5 3.2z" fill="currentColor"/></svg>',
 moon:'<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13 9.6A5.6 5.6 0 0 1 6.4 3a5.6 5.6 0 1 0 6.6 6.6z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/></svg>',
 sun:'<svg viewBox="0 0 16 16" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="8" cy="8" r="2.8"/><path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1"/></g></svg>'};
const setBtn=(el,icon,label)=>{el.innerHTML=IC[icon];el.setAttribute('aria-label',label);el.title=label};
function applyTheme(){if(theme)root.dataset.theme=theme;else delete root.dataset.theme;eff()==='dark'?setBtn($('bTheme'),'sun','Switch to light mode'):setBtn($('bTheme'),'moon','Switch to dark mode')}
$('bTheme').onclick=()=>{theme=eff()==='dark'?'light':'dark';try{localStorage.setItem('igris-theme',theme)}catch(e){}applyTheme()};
mq.addEventListener('change',applyTheme);applyTheme();
const playBtn=()=>{document.body.classList.toggle('paused',paused);paused?setBtn($('bPlay'),'play','Play'):setBtn($('bPlay'),'pause','Pause')};playBtn();
$('bPlay').onclick=()=>{paused=!paused;playBtn()};
$('bSpd').onclick=()=>{speed=speed===1?2:speed===2?0.5:1;$('bSpd').textContent=speed+'×';$('bSpd').setAttribute('aria-label','Speed '+speed+'×')};

const wrap=$('wrap'),stage=$('stage'),tip=$('tip');
function fit(){const s=Math.min(1,wrap.clientWidth/720);stage.style.transform='scale('+s+')';wrap.style.height=724*s+'px'}
new ResizeObserver(fit).observe(wrap);fit();
let curEl=null;
function show(el){const[h,b,c]=INFO[el.dataset.k].map(x=>x.replace(/\{(\w+)\}/g,(m,k)=>N[k]??m));tip.style.setProperty('--tc',c);tip.innerHTML='<b>'+h+'</b>'+b;
 const r=el.getBoundingClientRect(),tw=tip.offsetWidth,th=tip.offsetHeight,W=innerWidth,H=innerHeight;
 let x=Math.min(Math.max(8,r.left),W-tw-8),y=r.bottom+8;if(y+th>H-8)y=r.top-th-8;if(y<8)y=Math.max(8,Math.min(r.top+16,H-th-8));
 tip.style.left=x+'px';tip.style.top=y+'px';tip.classList.add('show');curEl=el}
function hide(){tip.classList.remove('show');curEl=null}
document.addEventListener('pointerover',e=>{if(e.pointerType!=='mouse')return;const el=e.target.closest('.hot');if(el){if(el!==curEl)show(el)}else if(curEl)hide()});
addEventListener('scroll',()=>{if(curEl)hide()},{passive:true});

document.addEventListener('click',e=>{const el=e.target.closest('.hot');if(el)show(el)});

document.addEventListener('pointerdown',e=>{if(!e.target.closest('.hot'))hide()});
}
