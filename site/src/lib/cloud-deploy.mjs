// One-click cloud deployments of the headless (slim) build. Used by the homepage and checked
// against the docs (tests/azure-deploy.test.ts). Azure templates are published by
// scripts/write-azure-template.mjs; Render reads render.yaml at the repository root; Koyeb
// deploys the image directly. Server deployments run ax-serve-repo (slim/ax-serve-repo.sh).

const SITE = 'https://getax.wenneker.io';
const REPO = 'https://github.com/GaryWenneker/ax';
// The -server image's entry point is ax-serve-repo, so no command override is needed.
const IMAGE = 'ghcr.io/garywenneker/ax-slim:latest-server';

const azure = (template) =>
	`https://portal.azure.com/#create/Microsoft.Template/uri/${encodeURIComponent(`${SITE}/deploy/${template}`)}`;

const koyebParams = [
	['type', 'docker'],
	['name', 'ax-mcp'],
	['image', IMAGE],
	['ports', '8080;http;/'],
	['env[PORT]', '8080'],
	['env[AX_REPO_URL]', 'https://github.com/OWNER/REPO'],
	// Deliberately too short: ax-serve-repo refuses to start until it is replaced.
	['env[AX_API_KEY]', 'CHANGE_ME'],
	['env[AX_PROJECT_ID]', 'project'],
];

export const cloudDeploys = [
	{
		id: 'azure-server',
		label: 'Azure',
		kind: 'MCP server',
		url: azure('azure-slim-server.json'),
		provider: 'azure',
	},
	{
		id: 'render',
		label: 'Render',
		kind: 'MCP server',
		url: `https://render.com/deploy?repo=${REPO}`,
		provider: 'render',
	},
	{
		id: 'koyeb',
		label: 'Koyeb',
		kind: 'MCP server',
		url: `https://app.koyeb.com/deploy?${koyebParams
			.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
			.join('&')}`,
		provider: 'koyeb',
	},
	{
		id: 'azure-job',
		label: 'Azure job',
		kind: 'index once',
		url: azure('azure-slim.json'),
		provider: 'azure',
	},
];
