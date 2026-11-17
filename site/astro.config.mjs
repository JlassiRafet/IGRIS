// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Project Pages site: everything lives under /IGRIS/ (decision 0002).
export default defineConfig({
	site: 'https://jlassirafet.github.io',
	base: '/IGRIS',
	integrations: [
		starlight({
			title: 'IGRIS',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/JlassiRafet/IGRIS' }],
			sidebar: [{ label: 'Docs', items: [{ label: 'Overview', slug: 'docs' }] }],
		}),
	],
});
