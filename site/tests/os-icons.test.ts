import assert from 'node:assert/strict';
import test from 'node:test';
import { agentIcons, agents, platformIcons, platforms } from '../src/lib/agent-icons.ts';

test('macOS, Linux, and Windows are a page row, not ticker items, and each has a brand mark', () => {
	assert.deepEqual(platforms, ['macOS', 'Linux', 'Windows']);
	for (const name of platforms) {
		assert.equal(agents.includes(name), false);
		assert.equal(name in agentIcons, false);
	}
	assert.match(platformIcons.macOS, /M12\.152/);
	assert.match(platformIcons.Linux, /M12\.504/);
	assert.match(platformIcons.Windows, /M0,0H11\.377/);
});
