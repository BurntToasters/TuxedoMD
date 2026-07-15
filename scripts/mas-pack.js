import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

if (process.platform !== 'darwin') throw new Error('MAS packaging must run on macOS.');

const required = ['APPLE_INSTALLER_SIGNING_IDENTITY'];
const missing = required.filter((name) => !process.env[name]?.trim());
if (missing.length) throw new Error(`Missing environment variables: ${missing.join(', ')}`);

const root = path.resolve(import.meta.dirname, '..');
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));

const bundleRoot = path.join(root, 'src-tauri/target/universal-apple-darwin/release/bundle/macos');
const app = path.join(bundleRoot, 'Tuxedo MD.app');

if (!fs.existsSync(app)) {
  throw new Error(`Missing ${app}; run npm run build:mac:appstore first.`);
}

const releaseDir = path.join(root, 'release');
fs.mkdirSync(releaseDir, { recursive: true });

const outputPkg = path.join(releaseDir, `TuxedoMD.Pro_${pkg.version}_universal.pkg`);
fs.rmSync(outputPkg, { force: true });

execFileSync(
  'productbuild',
  [
    '--component',
    app,
    '/Applications',
    '--sign',
    process.env.APPLE_INSTALLER_SIGNING_IDENTITY,
    outputPkg,
  ],
  { stdio: 'inherit' }
);

console.log(`Created MAS package at ${path.relative(root, outputPkg)}`);
