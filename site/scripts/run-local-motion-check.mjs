import { spawn } from 'node:child_process';
import { openSync, closeSync } from 'node:fs';

const log = openSync('/tmp/ax-controls-site-preview.log','w');
const preview = spawn(process.execPath,['node_modules/astro/bin/astro.mjs','dev','--host','127.0.0.1','--port','4322'],{stdio:['ignore',log,log]});
try {
  let ready = false;
  for (let i=0;i<600;i++) {
    if (preview.exitCode !== null) throw Error('Local preview failed to start; see /tmp/ax-controls-site-preview.log');
    try { if ((await fetch('http://127.0.0.1:4322',{signal:AbortSignal.timeout(1000)})).ok) {ready=true;break;} } catch {}
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  if (!ready) throw Error('Local preview did not become ready');
  const result = await new Promise(resolve=>{
    const check = spawn(process.execPath,['scripts/check-moire-motion.mjs'],{stdio:'inherit'});
    check.on('exit',resolve);
  });
  process.exitCode = result ?? 1;
} finally { preview.kill('SIGTERM');closeSync(log); }
