import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const tag = `v${pkg.version}`;
const repo = `${process.env.GH_REPO_OWNER || 'BurntToasters'}/${process.env.GH_REPO_NAME || 'Tuxedo-MD'}`;
const allowed = /\.(zip|dmg|AppImage|deb|rpm|exe|msi|msix|sig|asc)$/i;
const files = [];
function walk(folder) {
  if (!fs.existsSync(folder)) return;
  for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
    const item = path.join(folder, entry.name);
    if (entry.isDirectory() && !entry.name.startsWith('stage-')) walk(item);
    else if (allowed.test(entry.name) && !/pro/i.test(entry.name)) files.push(item);
  }
}
const target = path.join(root, 'src-tauri/target');
const targetRoots = fs.existsSync(target)
  ? [
      path.join(target, 'release/bundle'),
      ...fs
        .readdirSync(target, { withFileTypes: true })
        .filter((entry) => entry.isDirectory())
        .map((entry) => path.join(target, entry.name, 'release/bundle')),
    ]
  : [];
for (const folder of [path.join(root, 'release'), path.join(root, 'msstore'), ...targetRoots]) {
  walk(folder);
}
if (!files.length) throw new Error('No release assets found.');
execFileSync(
  process.platform === 'win32' ? 'gh.exe' : 'gh',
  ['release', 'upload', tag, '--repo', repo, '--clobber', ...files],
  { stdio: 'inherit' }
);
console.log(`Uploaded ${files.length} assets to ${tag}. The release remains a draft for review.`);
