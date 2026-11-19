// Copies the shared UI files into the site's public folder before dev and build.
// crates/igris-setup/ui/ is the only source (AGENTS.md); the copies in public/ are git-ignored.
import { copyFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const files = ['index.html', 'pipeline.css', 'pipeline.js'];
const dir = (p) => fileURLToPath(new URL(p, import.meta.url));
mkdirSync(dir('../public/'), { recursive: true });
for (const f of files) copyFileSync(dir(`../../crates/igris-setup/ui/${f}`), dir(`../public/${f}`));
console.log(`sync-ui: ${files.join(', ')} -> site/public/`);
