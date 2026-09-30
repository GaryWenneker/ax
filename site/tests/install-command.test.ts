import assert from 'node:assert/strict';
import test from 'node:test';
import {
	INSTALL_UNIX,
	INSTALL_WINDOWS,
	installBootScript,
	installCommandFor,
} from '../src/lib/install-command.ts';

test('macOS visitors get the shell installer', () => {
	assert.equal(
		installCommandFor('MacIntel', 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)'),
		INSTALL_UNIX,
	);
	assert.equal(installCommandFor('macOS', ''), INSTALL_UNIX);
});

test('Linux and other non-Windows visitors get the shell installer', () => {
	assert.equal(installCommandFor('Linux x86_64', ''), INSTALL_UNIX);
	assert.equal(
		installCommandFor('iPhone', 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)'),
		INSTALL_UNIX,
	);
	assert.equal(installCommandFor('', ''), INSTALL_UNIX);
});

test('Windows visitors get the PowerShell installer', () => {
	assert.equal(installCommandFor('Win32', ''), INSTALL_WINDOWS);
	assert.equal(installCommandFor('Windows', ''), INSTALL_WINDOWS);
	assert.equal(
		installCommandFor('', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'),
		INSTALL_WINDOWS,
	);
});

test('the page boot script uses the same check and both commands', () => {
	const boot = installBootScript();
	assert.match(boot, /windows\|win32\|win64/i);
	assert.ok(boot.includes(INSTALL_UNIX));
	assert.ok(boot.includes(INSTALL_WINDOWS));
	assert.match(boot, /data-install/);
});
