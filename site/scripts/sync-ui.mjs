// Copies the shared files into the site's public folder before dev and build.
// crates/igris-setup/ui/ and assets/logo/ are the only sources (AGENTS.md); the copies are git-ignored.
//   favicon.svg and logo.svg   -> public/                (the Home page and the docs)
//   the pipeline animation     -> public/pipeline/      (its own page, /pipeline/)
import { copyFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const at = (p) => fileURLToPath(new URL(p, import.meta.url));
const ui = '../../crates/igris-setup/ui/';
mkdirSync(at('../public/pipeline/'), { recursive: true });
copyFileSync(at(`${ui}favicon.svg`), at('../public/favicon.svg'));
copyFileSync(at('../../assets/logo/svg/IGRIS_full_colour_light_background.svg'), at('../public/logo.svg'));
copyFileSync(at(`${ui}index.html`), at('../public/pipeline/index.html'));
for (const f of ['pipeline.css', 'pipeline.js', 'favicon.svg']) copyFileSync(at(ui + f), at(`../public/pipeline/${f}`));
console.log('sync-ui: favicon.svg, logo.svg -> public/; the pipeline page -> public/pipeline/');
