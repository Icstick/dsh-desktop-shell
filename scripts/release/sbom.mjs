// Release support scripts (M8-D): SBOM generation.
// Usage: node scripts/release/sbom.mjs <out-dir>
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const outDir = process.argv[2] ?? 'target/release/sbom';
mkdirSync(outDir, { recursive: true });

function run(name, args, opts = {}) {
  console.log('> ' + name + ' ' + args.join(' '));
  try {
    execFileSync(name, args, { stdio: 'inherit', ...opts });
  } catch (error) {
    console.error('FAILED: ' + name + ' ' + args.join(' ') + ' - ' + error.message);
    process.exitCode = 1;
  }
}

// Rust SBOM (cargo-cyclonedx 0.5.x). It emits one <crate>.cdx.json
// next to every workspace member Cargo.toml; collect them into outDir.
if (existsSync('Cargo.toml')) {
  run('cargo', ['cyclonedx', '--all', '--format', 'json']);
  const { globSync, copyFileSync, rmSync } = await import('node:fs');
  for (const file of globSync('**/*.cdx.json')) {
    if (/node_modules|[\\/]target[\\/]/.test(file)) continue;
    copyFileSync(file, join(outDir, file.split(/[\\/]/).pop()));
    rmSync(file); // do not leave generated BOMs in member dirs
  }
} else {
  console.log('no Cargo.toml at root; skip cargo SBOM');
}

// npm SBOM (apps/desktop). pnpm projects have no package-lock.json and
// cyclonedx-npm (npm-only) cannot read pnpm-lock.yaml, so parse the v9
// lockfile directly and emit CycloneDX JSON (zero dependencies).
const npmLock = join(process.cwd(), 'apps/desktop', 'pnpm-lock.yaml');
const npmOut = join(outDir, 'npm-sbom.json');
if (existsSync(npmLock)) {
  const lock = readFileSync(npmLock, 'utf8');
  const components = [];
  // packages section: lines "  '<key>':" at depth 2; name/version at depth 4.
  let current = null;
  for (const line of lock.split('\n')) {
    const m = line.match(/^  '([^']+)':\s*$/);
    if (m) {
      // key forms: "name@version" or "name" (registry: tarball keys skipped)
      const key = m[1];
      if (key.includes('(') || key.startsWith('file:')) continue;
      const at = key.lastIndexOf('@');
      const name = at > 0 ? key.slice(0, at) : key;
      const version = at > 0 ? key.slice(at + 1) : '';
      current = { name, version };
      components.push(current);
      continue;
    }
    if (current) {
      const nv = line.match(/^    (name|version): (.+)$/);
      if (nv) current[nv[1]] = nv[2].replace(/^['"]|['"]$/g, '');
    }
  }
  const purl = (name, version) =>
    'pkg:npm/' + encodeURIComponent(name).replace(/%40/g, '@') + '@' + version;
  const sbom = {
    bomFormat: 'CycloneDX',
    specVersion: '1.5',
    version: 1,
    metadata: {
      component: { type: 'application', name: 'dsh-desktop-shell', version: '0.1.0' },
      timestamp: new Date().toISOString(),
    },
    components: components
      .filter((c) => c.name && c.version)
      .map((c) => ({
        type: 'library',
        name: c.name,
        version: c.version,
        purl: purl(c.name, c.version),
        'bom-ref': purl(c.name, c.version),
      })),
  };
  writeFileSync(npmOut, JSON.stringify(sbom, null, 2));
  console.log('pnpm SBOM: ' + components.length + ' components -> ' + npmOut);
} else {
  console.log('no pnpm-lock.yaml at apps/desktop; skip npm SBOM');
}

console.log('SBOM outputs: ' + outDir);
