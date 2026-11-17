// Copies the one shared UI file into the site's public folder before dev and build.
// ui/ is the only source (AGENTS.md); public/index.html is git-ignored.
import { copyFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const from = fileURLToPath(new URL('../../ui/index.html', import.meta.url));
const to = fileURLToPath(new URL('../public/index.html', import.meta.url));
mkdirSync(fileURLToPath(new URL('../public/', import.meta.url)), { recursive: true });
copyFileSync(from, to);
console.log('sync-ui: ui/index.html -> site/public/index.html');
