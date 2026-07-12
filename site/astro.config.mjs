// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://getax.wenneker.io
export default defineConfig({
	site: 'https://getax.wenneker.io',
	base: '/',
	integrations: [
		starlight({
			title: 'ax',
			description:
				'Structured context for AI agents — entirely on your machine. Graph it. Remember it. Ship it.',
			favicon: '/logo.png',
			head: [
				{
					tag: 'script',
					content:
						"if(!localStorage.getItem('starlight-theme')){try{localStorage.setItem('starlight-theme','dark')}catch(e){}document.documentElement.dataset.theme='dark';document.documentElement.style.colorScheme='dark'}",
				},
				{
					tag: 'script',
					content: `(function(){
  var box,img;
  function open(src,alt){
    if(!box){
      box=document.createElement('div');box.className='ax-lightbox';
      box.innerHTML='<button class="ax-lightbox-close" aria-label="Close">&times;</button><img />';
      img=box.querySelector('img');
      box.addEventListener('click',function(e){if(e.target!==img)close()});
      box.querySelector('.ax-lightbox-close').addEventListener('click',close);
      document.body.appendChild(box);
    }
    img.src=src;img.alt=alt||'';
    requestAnimationFrame(function(){box.classList.add('open')});
    document.addEventListener('keydown',onKey);
  }
  function close(){
    if(!box)return;box.classList.remove('open');
    document.removeEventListener('keydown',onKey);
  }
  function onKey(e){if(e.key==='Escape')close()}
  document.addEventListener('click',function(e){
    var t=e.target;
    if(t.tagName==='IMG'&&t.closest('.sl-markdown-content')){
      e.preventDefault();open(t.src,t.alt);
    }
  });
})();`,
				},
			],
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/GaryWenneker/ax',
				},
			],
			customCss: [
				'@fontsource-variable/archivo',
				'@fontsource/ibm-plex-mono/400.css',
				'@fontsource/ibm-plex-mono/500.css',
				'@fontsource/ibm-plex-mono/600.css',
				'@fontsource/bebas-neue',
				'./src/styles/theme.css',
			],
			components: {
				SiteTitle: './src/components/SiteTitle.astro',
				SocialIcons: './src/components/SocialIcons.astro',
			},
			expressiveCode: {
				themes: ['github-light', 'github-dark'],
				styleOverrides: {
					borderRadius: '0px',
					borderColor: 'transparent',
					borderWidth: '0px',
					codeFontFamily: "'IBM Plex Mono', ui-monospace, monospace",
					codeBackground: '#1a1814',
				},
			},
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Introduction', slug: 'getting-started/introduction' },
						{ label: 'Quickstart', slug: 'getting-started/quickstart' },
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Configuration', slug: 'getting-started/configuration' },
						{ label: 'Your First Graph', slug: 'getting-started/your-first-graph' },
						{ label: 'Next Steps', slug: 'getting-started/next-steps' },
					],
				},
				{
					label: 'Core Concepts',
					items: [
						{ label: 'How It Works', slug: 'core-concepts/how-it-works' },
						{ label: 'The Knowledge Graph', slug: 'core-concepts/knowledge-graph' },
						{ label: 'Resolution & Frameworks', slug: 'core-concepts/resolution' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Indexing a Project', slug: 'guides/indexing' },
						{ label: 'Policy Engine', slug: 'guides/policy-engine' },
						{ label: 'Framework Routes', slug: 'guides/framework-routes' },
						{ label: 'Affected Tests in CI', slug: 'guides/affected-tests' },
						{ label: 'Command Center', slug: 'guides/command-center' },
						{ label: 'Agent Terminal', slug: 'guides/agent-terminal' },
						{ label: 'Memory Vault', slug: 'guides/memory' },
						{ label: 'Token Savings', slug: 'guides/token-savings' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'MCP Server', slug: 'reference/mcp-server' },
						{ label: 'Integrations', slug: 'reference/integrations' },
						{ label: 'CLI', slug: 'reference/cli' },
						{ label: 'Rust API', slug: 'reference/api' },
						{ label: 'Languages', slug: 'reference/languages' },
					],
				},
				{ label: 'Troubleshooting', slug: 'troubleshooting' },
			],
		}),
	],
});
