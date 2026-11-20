// Checks the built Home page (run after `npm run build`): node scripts/check-home.mjs
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const dist = fileURLToPath(new URL('../dist/', import.meta.url));
const failures = [];
const need = (cond, msg) => cond || failures.push(msg);

const html = readFileSync(`${dist}index.html`, 'utf8');
need(/<h1[\s>]/.test(html), 'the Home page has an <h1>');
for (const id of ['what', 'how', 'setup', 'agents', 'privacy', 'status']) {
  need(new RegExp(`<h2[^>]*>[\\s\\S]*?</h2>`).test(html) && html.includes(`id="${id}"`), `section #${id} is present`);
}
need((html.match(/<h2[\s>]/g) || []).length === 6, 'exactly six <h2> sections');

const cssDir = `${dist}_astro/`;
const css = existsSync(cssDir) ? readdirSync(cssDir).filter((f) => f.endsWith('.css')).map((f) => readFileSync(cssDir + f, 'utf8')).join('\n') : '';
const all = html + css;
// Nothing may load from outside. Plain links (the GitHub link) are fine; resource loads are not.
const external = (page) => /<(?:script|link|img|iframe|source)[^>]+(?:src|href)=["']https?:\/\//i.test(page.replace(/<a\b[^>]*>/gi, ''));
need(!external(html), 'no external script, stylesheet or image on the Home page');
need(!external(readFileSync(`${dist}pipeline/index.html`, 'utf8')), 'no external script, stylesheet or font on the pipeline page');
need(!/@import\s+url\(["']?https?:/i.test(css) && !/url\(["']?https?:/i.test(css), 'no external url() in the CSS');
need(!/transition:\s*all/i.test(all), 'no `transition: all`');
need(/overflow-x:\s*clip/.test(css), '`overflow-x: clip` is set');
need(/prefers-reduced-motion/.test(css), 'a prefers-reduced-motion rule exists');
need(/:focus-visible/.test(css), 'a :focus-visible rule exists');
need(html.includes('rel="icon"'), 'a favicon link exists');
need(existsSync(`${dist}pipeline/index.html`), 'the pipeline animation page lives at /pipeline/');

// Contrast of every text pair, read from the tokens in the source stylesheet.
const tokens = Object.fromEntries([...readFileSync(fileURLToPath(new URL('../src/styles/home.css', import.meta.url)), 'utf8').matchAll(/(--[\w-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1], m[2]]));
const lum = (hex) => {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const ratio = (a, b) => (Math.max(lum(a), lum(b)) + 0.05) / (Math.min(lum(a), lum(b)) + 0.05);
for (const [fg, bg] of [['--ink', '--paper'], ['--body', '--paper'], ['--mute', '--paper'], ['--accent', '--paper'], ['--pill-text', '--pill'], ['--pill-dim', '--pill'], ['--accent-hot', '--pill'], ['--accent-hot', '--pill-2'], ['--pill-text', '--ink'], ['--paper', '--accent']]) {
  const r = ratio(tokens[fg], tokens[bg]);
  need(r >= 4.5, `${fg} on ${bg} is ${r.toFixed(2)}:1, needs 4.5`);
}

const docs = readFileSync(`${dist}docs/index.html`, 'utf8');
need(/href="[^"]*pipeline\/"[^>]*>pipeline demo/.test(docs), 'the docs link to the pipeline demo points at /pipeline/');

if (failures.length) {
  console.error(failures.map((f) => `FAIL ${f}`).join('\n'));
  process.exit(1);
}
console.log('Home page checks passed');
