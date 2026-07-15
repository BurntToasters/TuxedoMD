import { execFileSync, execSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
const prerelease = /-(?:beta|alpha|rc)(?:[.-]?\d+)?/i.test(pkg.version);
const rawArgs = process.argv.slice(2);
const required = {
  macSigning: rawArgs.includes('--require-macos-signing'),
  macNotarization: rawArgs.includes('--require-macos-notarization'),
  tauriSigning: rawArgs.includes('--require-tauri-signing'),
  windowsSigning: rawArgs.includes('--require-windows-signing'),
};
const skipWindowsCodeSigning = process.env.SKIP_WIN_CODESIGN?.trim() === '1';
const args = rawArgs.filter((arg) => !arg.startsWith('--require-'));
const has = (name) => Boolean(process.env[name]?.trim());
const valueAfter = (flag) => {
  const index = args.indexOf(flag);
  return index >= 0
    ? (args[index + 1] ?? '')
    : (args
        .find((arg) => arg.startsWith(`${flag}=`))
        ?.split('=')
        .slice(1)
        .join('=') ?? '');
};
const target = valueAfter('--target');
const macBuild = target ? target.includes('apple-darwin') : process.platform === 'darwin';
const windowsBuild = target ? target.includes('windows') : process.platform === 'win32';

if (!has('APPLE_PASSWORD') && has('APPLE_APP_SPECIFIC_PASSWORD')) {
  process.env.APPLE_PASSWORD = process.env.APPLE_APP_SPECIFIC_PASSWORD;
}

const missing = [];
if (required.tauriSigning && !args.includes('--no-bundle') && !has('TAURI_SIGNING_PRIVATE_KEY')) {
  missing.push('TAURI_SIGNING_PRIVATE_KEY');
}
if (macBuild && required.macSigning && !has('APPLE_SIGNING_IDENTITY')) {
  missing.push('APPLE_SIGNING_IDENTITY');
}
if (macBuild && required.macNotarization) {
  for (const name of ['APPLE_ID', 'APPLE_PASSWORD', 'APPLE_TEAM_ID'])
    if (!has(name)) missing.push(name);
}
if (required.windowsSigning) {
  if (!windowsBuild || args.includes('--no-bundle')) {
    console.error('[tauri-build] Windows signing requires a bundled Windows target.');
    process.exit(1);
  }
  if (process.platform !== 'win32') {
    console.error('[tauri-build] Signed Windows builds must run on Windows.');
    process.exit(1);
  }
  if (skipWindowsCodeSigning) {
    console.warn('[tauri-build] SKIP_WIN_CODESIGN=1; producing unsigned Windows artifacts.');
  } else {
    for (const name of [
      'AZURE_CLIENT_ID',
      'AZURE_TENANT_ID',
      'AZURE_CLIENT_SECRET',
      'AZURE_ARTIFACT_SIGNING_ENDPOINT',
      'AZURE_ARTIFACT_SIGNING_ACCOUNT',
      'AZURE_ARTIFACT_SIGNING_PROFILE',
      'AZURE_ARTIFACT_SIGNING_PUBLISHER',
    ]) {
      if (!has(name)) missing.push(name);
    }
  }
}
if (missing.length) {
  console.error(`[tauri-build] Missing required environment variables: ${missing.join(', ')}`);
  process.exit(1);
}

if (macBuild && required.macSigning) {
  const identities = execSync('security find-identity -v -p codesigning', { encoding: 'utf8' });
  if (!identities.includes(process.env.APPLE_SIGNING_IDENTITY)) {
    console.error(`[tauri-build] APPLE_SIGNING_IDENTITY was not found in the active keychain.`);
    process.exit(1);
  }
}

if (prerelease && windowsBuild && !args.includes('--no-bundle')) {
  const bundlesIndex = args.findIndex((arg) => arg === '--bundles' || arg.startsWith('--bundles='));
  if (bundlesIndex < 0) {
    args.push('--bundles', 'nsis');
  } else if (args[bundlesIndex] === '--bundles') {
    args[bundlesIndex + 1] =
      (args[bundlesIndex + 1] ?? '')
        .split(',')
        .filter((x) => x !== 'msi')
        .join(',') || 'nsis';
  } else {
    const filtered =
      args[bundlesIndex]
        .slice(10)
        .split(',')
        .filter((x) => x !== 'msi')
        .join(',') || 'nsis';
    args[bundlesIndex] = `--bundles=${filtered}`;
  }
  console.log(`[tauri-build] ${pkg.version} is a pre-release; MSI output is disabled.`);
}

execFileSync(process.platform === 'win32' ? 'npx.cmd' : 'npx', ['tauri', 'build', ...args], {
  stdio: 'inherit',
  env: process.env,
});

if (required.windowsSigning && !skipWindowsCodeSigning) {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const targetReleaseDir = path.join(root, 'src-tauri', 'target', target, 'release');
  const signScript = fileURLToPath(new URL('./windows-artifact-sign.ps1', import.meta.url));
  const runtimeExecutables = readdirSync(targetReleaseDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.exe'))
    .map((entry) => path.join(targetReleaseDir, entry.name));
  if (!runtimeExecutables.length) {
    throw new Error(`No final Windows runtime executables found under ${targetReleaseDir}`);
  }
  for (const executable of runtimeExecutables) {
    console.log(`[tauri-build] Finalizing Authenticode signature: ${executable}`);
    execFileSync(
      'powershell.exe',
      [
        '-NoProfile',
        '-NonInteractive',
        '-ExecutionPolicy',
        'Bypass',
        '-File',
        signScript,
        '-FilePath',
        executable,
      ],
      { stdio: 'inherit', env: process.env }
    );
  }
  execFileSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-NonInteractive',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      fileURLToPath(new URL('./verify-windows-authenticode.ps1', import.meta.url)),
      '-TargetReleaseDir',
      targetReleaseDir,
    ],
    { stdio: 'inherit', env: process.env }
  );
}
