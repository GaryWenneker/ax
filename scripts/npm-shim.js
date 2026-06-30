#!/usr/bin/env node
'use strict';
//
// npm thin-installer for ax (native Rust binary).
//
// Downloads the matching release asset from GitHub (ax-<platform>-<arch>.tar.gz
// or .zip), caches it under AX_INSTALL_DIR, and execs the binary. The user's
// Node is only a launcher.
//
//   "bin": { "ax": "npm-shim.js" }
//
// Knobs:
//   AX_NO_DOWNLOAD=1       disable network fetch (use cache only)
//   AX_INSTALL_DIR=DIR     cache location
//   AX_GITHUB_REPO=owner/repo
//   AX_VERSION=v0.1.0      pin release tag

var childProcess = require('child_process');
var fs = require('fs');
var os = require('os');
var path = require('path');

var REPO = process.env.AX_GITHUB_REPO || 'GaryWenneker/ax';
var isWindows = process.platform === 'win32';
var target = process.platform + '-' + process.arch;

main().catch(function (e) {
  process.stderr.write('ax: ' + (e && e.message ? e.message : String(e)) + '\n');
  process.exit(1);
});

async function main() {
  var bin = resolveCachedBinary() || (await downloadBinary());
  var res = childProcess.spawnSync(bin, process.argv.slice(2), { stdio: 'inherit' });
  if (res.error) {
    process.stderr.write('ax: ' + res.error.message + '\n');
    process.exit(1);
  }
  process.exit(res.status === null ? 1 : res.status);
}

function installRoot() {
  if (process.env.AX_INSTALL_DIR) return process.env.AX_INSTALL_DIR;
  return isWindows
    ? path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local'), 'ax')
    : path.join(os.homedir(), '.ax');
}

function readVersion() {
  if (process.env.AX_VERSION) return normalizeTag(process.env.AX_VERSION);
  try {
    return normalizeTag(require(path.join(__dirname, 'package.json')).version);
  } catch (e) {
    fail('could not read package version.');
  }
}

function normalizeTag(v) {
  v = String(v).trim();
  return v.startsWith('v') ? v : 'v' + v;
}

function binaryIn(dir) {
  var unix = path.join(dir, 'ax');
  var win = path.join(dir, 'ax.exe');
  if (isWindows && fs.existsSync(win)) return win;
  if (!isWindows && fs.existsSync(unix)) return unix;
  return null;
}

function resolveCachedBinary() {
  var version = readVersion();
  var dir = path.join(installRoot(), 'npm-bundles', target + '-' + version);
  return binaryIn(dir);
}

async function downloadBinary() {
  if (process.env.AX_NO_DOWNLOAD) {
    fail('no cached binary and AX_NO_DOWNLOAD is set.');
  }

  var version = readVersion();
  var asset = 'ax-' + target + (isWindows ? '.zip' : '.tar.gz');
  var url =
    'https://github.com/' + REPO + '/releases/download/' + version + '/' + asset;
  var bundlesDir = path.join(installRoot(), 'npm-bundles');
  var dest = path.join(bundlesDir, target + '-' + version);

  var existing = binaryIn(dest);
  if (existing) return existing;

  process.stderr.write('ax: downloading ' + asset + ' (' + version + ')...\n');

  fs.mkdirSync(bundlesDir, { recursive: true });
  var stage = fs.mkdtempSync(path.join(bundlesDir, '.dl-'));
  try {
    var archivePath = path.join(stage, asset);
    await download(url, archivePath);
    fs.mkdirSync(dest, { recursive: true });
    extract(archivePath, dest);
  } catch (e) {
    rmrf(stage);
    fail('download failed: ' + e.message + '\n  URL: ' + url);
  }
  rmrf(stage);

  var bin = binaryIn(dest);
  if (!bin) fail('extracted bundle missing ax binary under ' + dest);
  if (!isWindows) {
    try { fs.chmodSync(bin, 0o755); } catch (e) { /* best effort */ }
  }
  process.stderr.write('ax: ready.\n');
  return bin;
}

function download(url, dest) {
  return new Promise(function (resolve, reject) {
    var https = require('https');
    var req = https.get(url, { headers: { 'User-Agent': 'ax-npm-shim' }, timeout: 60000 }, function (res) {
      var status = res.statusCode || 0;
      if (status >= 300 && status < 400 && res.headers.location) {
        res.resume();
        download(new URL(res.headers.location, url).toString(), dest).then(resolve, reject);
        return;
      }
      if (status !== 200) {
        res.resume();
        reject(new Error('HTTP ' + status));
        return;
      }
      var file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on('finish', function () { file.close(function () { resolve(); }); });
      file.on('error', reject);
    });
    req.on('timeout', function () { req.destroy(new Error('timeout')); });
    req.on('error', reject);
  });
}

function extract(archive, destDir) {
  var args = isWindows
    ? ['-xf', archive, '-C', destDir, '--strip-components=1']
    : ['-xzf', archive, '-C', destDir, '--strip-components=1'];
  var res = childProcess.spawnSync('tar', args, { stdio: 'ignore' });
  if (res.error) throw new Error('tar unavailable: ' + res.error.message);
  if (res.status !== 0) throw new Error('tar exited ' + res.status);
}

function rmrf(p) {
  try { fs.rmSync(p, { recursive: true, force: true }); } catch (e) { /* noop */ }
}

function fail(msg) {
  process.stderr.write(
    'ax: ' + msg + '\n' +
    'Install without npm:\n' +
    '  curl -fsSL https://getax.wenneker.io/install.sh | sh\n'
  );
  process.exit(1);
}
