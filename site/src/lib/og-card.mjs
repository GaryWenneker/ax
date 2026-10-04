/**
 * Share-card paths and HTML. The production build screenshots each page and
 * paints the release from public/releases/latest.txt onto a 1200×630 card.
 */
import path from 'node:path';

const SITE_ORIGIN = 'https://getax.wenneker.io';

const KICKERS = {
	'getting-started': 'Getting started',
	guides: 'Guide',
	reference: 'Reference',
	'core-concepts': 'Core concepts',
};

/** Public path of the PNG for a site pathname. Unexpected paths use the home card. */
export function ogImagePath(pathname) {
	const raw = String(pathname ?? '/').split('?')[0].split('#')[0];
	if (raw.includes('://')) return '/og/home.png';
	let p = raw.startsWith('/') ? raw : `/${raw}`;
	if (p.length > 1 && p.endsWith('/')) p = p.slice(0, -1);
	if (p.includes('..') || !/^\/[a-zA-Z0-9/_-]*$/.test(p)) return '/og/home.png';
	if (p === '/') return '/og/home.png';
	return `/og${p}.png`;
}

export function kickerFor(pathname) {
	const raw = String(pathname ?? '/').split('?')[0].split('#')[0];
	const parts = raw.split('/').filter(Boolean);
	if (parts.length === 0) return 'Current release';
	return KICKERS[parts[0]] ?? 'ax';
}

export function shareTitle(documentTitle) {
	let title = String(documentTitle ?? '').trim();
	title = title.replace(/\s+\|\s+ax\s*$/i, '');
	title = title.replace(/^ax\s+[—–-]\s+/i, '');
	return title || 'ax';
}

/** `latest.txt` stores `v7.0.0`. Anything else is a broken release pointer. */
export function releaseLabel(raw) {
	const version = String(raw ?? '').trim();
	if (!/^v\d+\.\d+\.\d+/.test(version)) {
		throw new Error(`release label must look like v7.0.0, got ${JSON.stringify(version)}`);
	}
	return version;
}

export function escapeHtml(value) {
	return String(value).replace(/[&<>"']/g, (ch) => {
		switch (ch) {
			case '&':
				return '&amp;';
			case '<':
				return '&lt;';
			case '>':
				return '&gt;';
			case '"':
				return '&quot;';
			default:
				return '&#39;';
		}
	});
}

function metaContent(html, attr, name) {
	const re = new RegExp(
		`<meta\\s+[^>]*${attr}=["']${name}["'][^>]*content=["']([^"']*)["'][^>]*>`,
		'i',
	);
	const alt = new RegExp(
		`<meta\\s+[^>]*content=["']([^"']*)["'][^>]*${attr}=["']${name}["'][^>]*>`,
		'i',
	);
	return html.match(re)?.[1] ?? html.match(alt)?.[1] ?? '';
}

export function readShareMeta(html) {
	const titleTag = html.match(/<title>([^<]*)<\/title>/i)?.[1] ?? '';
	return {
		image: metaContent(html, 'property', 'og:image'),
		title: metaContent(html, 'property', 'og:title') || titleTag,
		description: metaContent(html, 'property', 'og:description') || metaContent(html, 'name', 'description'),
	};
}

export function pathnameFromDistHtml(distDir, file) {
	const rel = path.relative(distDir, file).split(path.sep).join('/');
	if (rel === 'index.html') return '/';
	if (rel.endsWith('/index.html')) return `/${rel.slice(0, -'index.html'.length)}`;
	if (rel.endsWith('.html')) return `/${rel.slice(0, -'.html'.length)}`;
	return `/${rel}`;
}

export function distPathForOgUrl(ogUrl, distDir, siteOrigin = SITE_ORIGIN) {
	const url = new URL(ogUrl);
	if (url.origin !== siteOrigin) {
		throw new Error(`unexpected og origin ${url.origin}`);
	}
	const rel = url.pathname.replace(/^\//, '');
	if (!rel.startsWith('og/') || !rel.endsWith('.png') || rel.includes('..')) {
		throw new Error(`unexpected og path ${url.pathname}`);
	}
	return path.join(distDir, ...rel.split('/'));
}

/**
 * Clip a screenshot to the viewport, keeping the top of the element and
 * capping height so a long article does not become a thin strip.
 */
export function clipBox(box, viewport) {
	const x = Math.max(0, Math.min(box.x, viewport.width - 1));
	const y = Math.max(0, Math.min(box.y, viewport.height - 1));
	const width = Math.max(1, Math.min(box.width, viewport.width - x));
	const height = Math.max(1, Math.min(box.height, viewport.height - y, 640));
	return { x, y, width, height };
}

/** Runs inside the browser. Must not close over Node values. */
export function prepareShareShot() {
	document.documentElement.dataset.theme = 'dark';
	document.documentElement.style.colorScheme = 'dark';
	const style = document.createElement('style');
	style.textContent = `
		*, *::before, *::after { animation: none !important; transition: none !important; }
		.reveal { opacity: 1 !important; transform: none !important; }
		header.header, header.nav, nav.sidebar, aside.right-sidebar-container, footer { display: none !important; }
		.main-frame { padding: 1.25rem 1.5rem 0 !important; }
		#hero, main { background: #121110 !important; }
		#hero { min-height: 0 !important; height: auto !important; justify-content: flex-start !important; padding-top: 1.25rem !important; }
	`;
	document.head.appendChild(style);
	document.querySelectorAll('.reveal').forEach((el) => el.classList.add('revealed'));
}

export function cardHtml({ version, kicker, title, description, shotUrl, logoUrl, shotPosition = 'left top' }) {
	if (shotPosition !== 'left top' && shotPosition !== 'center top') {
		throw new Error(`unexpected shot position ${shotPosition}`);
	}
	const safeVersion = releaseLabel(version);
	const safeTitle = escapeHtml(shareTitle(title));
	const safeKicker = escapeHtml(kicker);
	const safeDescription = escapeHtml(description);
	const safeShot = escapeHtml(shotUrl);
	const safeLogo = escapeHtml(logoUrl);
	return `<!doctype html><html><head><meta charset="utf-8"><style>
		:root { --paper:#121110; --ink:#f3f1ea; --ink2:#b8b5a8; --accent:#c9a55c; }
		* { box-sizing:border-box; margin:0; padding:0; }
		html, body { width:1200px; height:630px; overflow:hidden; background:#121110; }
		body { display:flex; align-items:center; gap:36px; padding:48px 40px 48px 56px;
			background:radial-gradient(120% 90% at 85% 10%, #23211a 0%, var(--paper) 55%);
			color:var(--ink); font-family:system-ui, sans-serif; }
		.text { flex:0 0 460px; display:flex; flex-direction:column; gap:16px; min-width:0; }
		.logo { width:150px; height:91px; margin-left:-8px;
			background:url('${safeLogo}') no-repeat; background-size:200% auto; background-position:48% 45%; }
		.kicker { font-family:ui-monospace, monospace; color:var(--accent); letter-spacing:.08em;
			text-transform:uppercase; font-size:15px; }
		.version { font-size:84px; line-height:.9; color:var(--accent); font-weight:700; letter-spacing:-.02em; }
		h1 { font-size:40px; line-height:1.05; font-weight:700; display:-webkit-box; -webkit-line-clamp:3;
			-webkit-box-orient:vertical; overflow:hidden; }
		p { color:var(--ink2); font-size:18px; line-height:1.35; display:-webkit-box; -webkit-line-clamp:3;
			-webkit-box-orient:vertical; overflow:hidden; }
		.url { font-family:ui-monospace, monospace; color:var(--ink2); font-size:15px; }
		.frame { flex:1; min-width:0; height:520px; border:1px solid #2c2a23; border-radius:14px; overflow:hidden;
			background:#1a1814; box-shadow:0 30px 80px rgba(0,0,0,.55); }
		.bar { height:28px; background:#1e1c16; display:flex; gap:8px; align-items:center; padding:0 12px; }
		.bar i { width:11px; height:11px; border-radius:50%; background:#57554c; display:block; }
		.frame img { display:block; width:100%; height:492px; object-fit:cover; object-position:${shotPosition}; }
	</style></head><body>
		<div class="text">
			<div class="logo"></div>
			<div class="kicker">${safeKicker}</div>
			<div class="version">${safeVersion}</div>
			<h1>${safeTitle}</h1>
			<p>${safeDescription}</p>
			<div class="url">getax.wenneker.io</div>
		</div>
		<div class="frame"><div class="bar"><i></i><i></i><i></i></div><img src="${safeShot}" alt=""></div>
	</body></html>`;
}
