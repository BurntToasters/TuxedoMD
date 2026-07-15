import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

if (process.platform !== 'win32') throw new Error('MSIX packaging must run on Windows.');
const required = ['MSSTORE_IDENTITY_NAME', 'MSSTORE_PUBLISHER', 'MSSTORE_PUBLISHER_DISPLAY_NAME'];
const missing = required.filter((name) => !process.env[name]?.trim());
if (missing.length) throw new Error(`Missing environment variables: ${missing.join(', ')}`);

const root = path.resolve(import.meta.dirname, '..');
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const numericVersion = pkg.version
  .replace(/-.+$/, '')
  .split('.')
  .concat('0', '0', '0', '0')
  .slice(0, 4)
  .join('.');

const outputDir = path.join(root, 'msstore');
fs.mkdirSync(outputDir, { recursive: true });

const TARGETS = [
  { arch: 'x64', rustTarget: 'x86_64-pc-windows-msvc', msixArch: 'x64' },
  { arch: 'arm64', rustTarget: 'aarch64-pc-windows-msvc', msixArch: 'arm64' },
];

const generatedMsixFiles = [];

for (const { arch, rustTarget, msixArch } of TARGETS) {
  const executable = path.join(root, `src-tauri/target/${rustTarget}/release/tuxedomd.exe`);
  if (!fs.existsSync(executable)) {
    console.log(`Skipping ${arch}, missing ${executable}`);
    continue;
  }

  const stage = path.join(outputDir, `stage-${arch}`);
  const assets = path.join(stage, 'Assets');
  fs.rmSync(stage, { recursive: true, force: true });
  fs.mkdirSync(assets, { recursive: true });

  fs.copyFileSync(executable, path.join(stage, 'TuxedoMD.exe'));
  for (const [source, target] of [
    ['Square30x30Logo.png', 'Square44x44Logo.png'],
    ['Square142x142Logo.png', 'Square150x150Logo.png'],
    ['Square284x284Logo.png', 'Square310x310Logo.png'],
    ['StoreLogo.png', 'StoreLogo.png'],
  ]) {
    const from = path.join(root, 'src-tauri/icons', source);
    if (!fs.existsSync(from))
      throw new Error(`Missing store icon ${from}; run npm run icons:normalize.`);
    fs.copyFileSync(from, path.join(assets, target));
  }
  const manifest = `<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10" xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10" IgnorableNamespaces="uap">
  <Identity Name="${process.env.MSSTORE_IDENTITY_NAME}" Publisher="${process.env.MSSTORE_PUBLISHER}" Version="${numericVersion}" ProcessorArchitecture="${msixArch}" />
  <Properties><DisplayName>Tuxedo MD Pro</DisplayName><PublisherDisplayName>${process.env.MSSTORE_PUBLISHER_DISPLAY_NAME}</PublisherDisplayName><Logo>Assets\\StoreLogo.png</Logo></Properties>
  <Dependencies><TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.26100.0" /></Dependencies>
  <Resources><Resource Language="en-us" /></Resources>
  <Applications><Application Id="TuxedoMD" Executable="TuxedoMD.exe" EntryPoint="Windows.FullTrustApplication"><uap:VisualElements DisplayName="Tuxedo MD Pro" Description="A sleek Markdown editor" Square44x44Logo="Assets\\Square44x44Logo.png" Square150x150Logo="Assets\\Square150x150Logo.png" BackgroundColor="transparent"><uap:DefaultTile Square310x310Logo="Assets\\Square310x310Logo.png" /></uap:VisualElements></Application></Applications>
</Package>\n`;
  fs.writeFileSync(path.join(stage, 'AppxManifest.xml'), manifest);

  const msixOut = path.join(outputDir, `TuxedoMD.Pro_${numericVersion}_${arch}.msix`);
  fs.rmSync(msixOut, { force: true });
  execFileSync('makeappx.exe', ['pack', '/d', stage, '/p', msixOut, '/o'], { stdio: 'inherit' });
  console.log(`Created ${path.relative(root, msixOut)}`);

  generatedMsixFiles.push(msixOut);
}

if (generatedMsixFiles.length === 0) {
  throw new Error('No executables were found to package.');
}

const bundleInput = path.join(outputDir, 'bundle-input');
fs.rmSync(bundleInput, { recursive: true, force: true });
fs.mkdirSync(bundleInput, { recursive: true });
for (const msix of generatedMsixFiles) {
  fs.copyFileSync(msix, path.join(bundleInput, path.basename(msix)));
}
const bundleOut = path.join(outputDir, `TuxedoMD.Pro_${numericVersion}.msixbundle`);
fs.rmSync(bundleOut, { force: true });
execFileSync('makeappx.exe', ['bundle', '/d', bundleInput, '/p', bundleOut, '/o'], {
  stdio: 'inherit',
});
console.log(`Created bundle ${path.relative(root, bundleOut)}`);
fs.rmSync(bundleInput, { recursive: true, force: true });
