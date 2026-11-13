// Builds island-hook and places it where Tauri's bundler expects a sidecar:
// src-tauri/binaries/island-hook-<target-triple>[.exe]. The installer then
// ships it next to the app as island-hook[.exe].
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const tauriDir = join(root, 'src-tauri');
const triple = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1].trim();
const exe = process.platform === 'win32' ? '.exe' : '';
const targetDir = process.env.CARGO_TARGET_DIR ?? join(tauriDir, 'target');

execFileSync('cargo', ['build', '--release', '-p', 'island-hook'], { cwd: tauriDir, stdio: 'inherit' });

mkdirSync(join(tauriDir, 'binaries'), { recursive: true });
const dest = join(tauriDir, 'binaries', `island-hook-${triple}${exe}`);
copyFileSync(join(targetDir, 'release', `island-hook${exe}`), dest);
console.log(`island-hook → ${dest}`);
