/** Copy Material Icon Theme SVGs into public/ for ax web UI. */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const src = path.join(root, 'node_modules', 'material-icon-theme', 'icons');
const dest = path.join(root, 'public', 'material-icons');

if (!fs.existsSync(src)) {
  console.error('material-icon-theme icons not found — run npm install first');
  process.exit(1);
}

fs.mkdirSync(dest, { recursive: true });
let copied = 0;
for (const name of fs.readdirSync(src)) {
  if (!name.endsWith('.svg')) continue;
  fs.copyFileSync(path.join(src, name), path.join(dest, name));
  copied += 1;
}
console.log(`material-icons: copied ${copied} SVGs -> public/material-icons/`);
