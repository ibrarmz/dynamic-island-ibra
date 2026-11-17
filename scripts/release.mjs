// Cuts a release: npm run release -- 0.1.1
// Sets the version in package.json, tauri.conf.json and Cargo.toml, commits,
// tags v<version> and pushes. GitHub Actions then builds and publishes the
// installers (.github/workflows/release.yml).
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) {
  console.error('uso: npm run release -- <versión>   (por ejemplo 0.1.1)');
  process.exit(1);
}

const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
if (git('status', '--porcelain')) {
  console.error('Hay cambios sin commit; haz commit antes de publicar una versión.');
  process.exit(1);
}
if (git('tag', '--list', `v${version}`)) {
  console.error(`La versión v${version} ya existe.`);
  process.exit(1);
}

const edit = (file, fn) => {
  const p = join(root, file);
  writeFileSync(p, fn(readFileSync(p, 'utf8')));
};
const setJsonVersion = (text) => text.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`);
edit('package.json', setJsonVersion);
edit('src-tauri/tauri.conf.json', setJsonVersion);
// Only the [package] version (the first one in the file), not dependency versions.
edit('src-tauri/Cargo.toml', (t) => t.replace(/^version = "[^"]+"/m, `version = "${version}"`));
edit('package-lock.json', (t) => t.replace(/("name": "ibrae-desktop",\s*"version":\s*")[^"]+(")/g, `$1${version}$2`));

// Refresh Cargo.lock's entry for the app without touching dependencies.
execFileSync('cargo', ['update', '--workspace', '--offline'], { cwd: join(root, 'src-tauri'), stdio: 'inherit' });

git('add', 'package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock');
git('commit', '-m', `release: v${version}`);
git('tag', '-a', `v${version}`, '-m', `IBRÆ ${version}`);
execFileSync('git', ['push', 'origin', 'HEAD', `v${version}`], { cwd: root, stdio: 'inherit' });
console.log(`\nv${version} publicada. GitHub está compilando los instaladores:`);
console.log('https://github.com/ibrarmz/ibrae/actions');
