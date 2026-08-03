import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const reportPath = path.join(repositoryRoot, 'target', 'reports', 'dependency-license-report.json');

function licenseFromManifest(manifest) {
  if (typeof manifest.license === 'string' && manifest.license.trim()) {
    return manifest.license.trim();
  }
  if (Array.isArray(manifest.licenses)) {
    const licenses = manifest.licenses
      .map((license) => (typeof license === 'string' ? license : license?.type))
      .filter(Boolean);
    if (licenses.length > 0) {
      return licenses.join(' OR ');
    }
  }
  return 'UNKNOWN';
}

function packageIdentity(left, right) {
  return (
    left.name.localeCompare(right.name) ||
    left.version.localeCompare(right.version) ||
    left.license.localeCompare(right.license)
  );
}

function deduplicateAndSort(packages) {
  const byIdentity = new Map();
  for (const packageRecord of packages) {
    const key = `${packageRecord.name}\0${packageRecord.version}\0${packageRecord.license}`;
    const existing = byIdentity.get(key);
    if (!existing || existing.source !== 'workspace') {
      byIdentity.set(key, packageRecord);
    }
  }
  return [...byIdentity.values()].sort(packageIdentity);
}

function collectManifest(directory, source, packages) {
  const manifestPath = path.join(directory, 'package.json');
  if (!fs.existsSync(manifestPath)) {
    return;
  }
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  if (typeof manifest.name === 'string' && typeof manifest.version === 'string') {
    packages.push({
      license: licenseFromManifest(manifest),
      name: manifest.name,
      source,
      version: manifest.version,
    });
  }
}

function walkInstalledManifests(directory, depth, packages) {
  if (depth < 0 || !fs.existsSync(directory)) {
    return;
  }
  collectManifest(directory, 'pnpm', packages);

  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.isSymbolicLink()) {
      continue;
    }
    walkInstalledManifests(path.join(directory, entry.name), depth - 1, packages);
  }
}

export function collectJavascriptPackages(rootDirectory = repositoryRoot) {
  const packages = [];
  collectManifest(rootDirectory, 'workspace', packages);

  const applicationsDirectory = path.join(rootDirectory, 'apps');
  for (const entry of fs.readdirSync(applicationsDirectory, { withFileTypes: true })) {
    if (entry.isDirectory() && !entry.isSymbolicLink()) {
      collectManifest(path.join(applicationsDirectory, entry.name), 'workspace', packages);
    }
  }

  const pnpmStore = path.join(rootDirectory, 'node_modules', '.pnpm');
  if (!fs.existsSync(pnpmStore)) {
    throw new Error('node_modules/.pnpm is missing; run pnpm install before generating the report');
  }
  walkInstalledManifests(pnpmStore, 5, packages);
  return deduplicateAndSort(packages);
}

export function collectRustPackages(rootDirectory = repositoryRoot) {
  const result = spawnSync('cargo', ['metadata', '--format-version', '1', '--locked'], {
    cwd: rootDirectory,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    shell: false,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || `cargo metadata exited with status ${result.status}`);
  }

  const metadata = JSON.parse(result.stdout);
  return metadata.packages
    .map((packageRecord) => ({
      license:
        packageRecord.license ?? (packageRecord.license_file ? 'SEE LICENSE FILE' : 'UNKNOWN'),
      name: packageRecord.name,
      source: packageRecord.source ? packageRecord.source.split('+')[0] : 'workspace',
      version: packageRecord.version,
    }))
    .sort(packageIdentity);
}

function summarize(packages) {
  const licenseCounts = new Map();
  for (const packageRecord of packages) {
    licenseCounts.set(packageRecord.license, (licenseCounts.get(packageRecord.license) ?? 0) + 1);
  }
  return {
    licenseCounts: Object.fromEntries(
      [...licenseCounts.entries()].sort(([left], [right]) => left.localeCompare(right)),
    ),
    packageCount: packages.length,
    unknownLicensePackages: packages
      .filter((packageRecord) => packageRecord.license === 'UNKNOWN')
      .map((packageRecord) => `${packageRecord.name}@${packageRecord.version}`),
  };
}

export function buildDependencyReport(rootDirectory = repositoryRoot) {
  const javascriptPackages = collectJavascriptPackages(rootDirectory);
  const rustPackages = collectRustPackages(rootDirectory);
  const rootManifest = JSON.parse(
    fs.readFileSync(path.join(rootDirectory, 'package.json'), 'utf8'),
  );

  return {
    schemaVersion: 1,
    project: {
      license: rootManifest.license,
      name: 'A^3',
      version: rootManifest.version,
    },
    rust: {
      packages: rustPackages,
      summary: summarize(rustPackages),
    },
    javascript: {
      packages: javascriptPackages,
      summary: summarize(javascriptPackages),
    },
  };
}

function run() {
  const report = buildDependencyReport();
  fs.mkdirSync(path.dirname(reportPath), { recursive: true });
  fs.writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${path.relative(repositoryRoot, reportPath)}.`);
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : undefined;
if (invokedPath === import.meta.url) {
  try {
    run();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
