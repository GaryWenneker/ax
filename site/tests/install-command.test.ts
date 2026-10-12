import assert from 'node:assert/strict';
import test from 'node:test';
import {
	INSTALL_UNIX,
	INSTALL_WINDOWS,
	installCommandFor,
	detectInstallOS,
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

test('desktop platform detection distinguishes macOS, Linux, Windows and WSL2', () => {
	assert.equal(detectInstallOS('macOS'), 'macos');
	assert.equal(detectInstallOS('MacIntel'), 'macos');
	assert.equal(detectInstallOS('Linux x86_64'), 'linux');
	assert.equal(detectInstallOS('Windows'), 'windows');
	assert.equal(detectInstallOS('Win32'), 'windows');
	assert.equal(detectInstallOS('Linux', 'Microsoft WSL2'), 'wsl2');
	assert.equal(installCommandFor('Linux', 'Microsoft WSL2'), INSTALL_UNIX);
});

test('mobile and unknown browsers ask for the development computer', () => {
	assert.equal(detectInstallOS('Linux armv8l', 'Android 16'), null);
	assert.equal(detectInstallOS('iPhone', 'iPhone OS 18'), null);
	assert.equal(detectInstallOS('MacIntel', 'Macintosh', 5), null);
	assert.equal(detectInstallOS('', ''), null);
});

import { installerCommand, installerShell } from '../src/scripts/minimal-download.ts';
test('all primary installation paths offer script commands and correct shells', () => {
  for (const os of ['macos', 'linux', 'windows', 'wsl2'] as const) {
    assert.equal(installerCommand(os), os === 'windows' ? INSTALL_WINDOWS : INSTALL_UNIX);
    assert.ok(installerShell(os).length > 0);
    assert.doesNotMatch(installerCommand(os), /releases\/|tar\.gz|\.zip/);
  }
  assert.equal(installerShell('windows'), 'PowerShell');
  assert.equal(installerShell('wsl2'), 'your WSL2 Linux terminal');
});
