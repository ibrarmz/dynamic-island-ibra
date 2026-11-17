// Writes the in-app updater's latest.json for a release.
// Usage: node latest-json.mjs <version> <setup.exe.sig> <msi.sig>
// The files are uploaded under the names the release workflow gives them.
import { readFileSync } from 'node:fs';

const [version, exeSig, msiSig] = process.argv.slice(2);
const repo = process.env.GITHUB_REPOSITORY || 'ibrarmz/ibrae';
const base = `https://github.com/${repo}/releases/download/v${version}`;
const sig = (f) => readFileSync(f, 'utf8').trim();

console.log(
  JSON.stringify(
    {
      version,
      notes: `IBRÆ ${version}`,
      pub_date: new Date().toISOString(),
      platforms: {
        'windows-x86_64-nsis': { signature: sig(exeSig), url: `${base}/IBRAE_${version}_x64-setup.exe` },
        'windows-x86_64-msi': { signature: sig(msiSig), url: `${base}/IBRAE_${version}_x64.msi` },
      },
    },
    null,
    2,
  ),
);
