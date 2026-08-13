import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  extractMarkdownLinks,
  markdownAnchors,
  validateLocalTarget,
} from './check-markdown-links.mjs';
import { buildDependencyReport } from './generate-dependency-report.mjs';

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('Markdown extraction ignores code and retains source lines', () => {
  const markdown = [
    '[valid](docs/README.md)',
    '`[inline](missing.md)`',
    '~~~text',
    '[fenced](missing.md)',
    '~~~',
    '[reference]: AGENTS.md',
  ].join('\n');

  assert.deepEqual(extractMarkdownLinks(markdown), [
    { lineNumber: 1, target: 'docs/README.md' },
    { lineNumber: 6, target: 'AGENTS.md' },
  ]);
});

test('Markdown anchors use stable GitHub-style duplicate suffixes', () => {
  assert.deepEqual(
    [...markdownAnchors('# A^3\n## Wiederholt\n## Wiederholt')],
    ['a3', 'wiederholt', 'wiederholt-1'],
  );
});

test('Local target validation rejects missing files and anchors', () => {
  const sourceFile = path.join(repositoryRoot, 'docs', 'README.md');
  assert.deepEqual(validateLocalTarget(repositoryRoot, sourceFile, '../README.md'), {
    local: true,
  });
  assert.match(
    validateLocalTarget(repositoryRoot, sourceFile, 'missing.md').error,
    /does not exist/u,
  );
  assert.match(
    validateLocalTarget(repositoryRoot, sourceFile, 'README.md#missing-anchor').error,
    /anchor/u,
  );
});

test('Dependency report is deterministic and contains both ecosystems', () => {
  const first = buildDependencyReport(repositoryRoot);
  const second = buildDependencyReport(repositoryRoot);

  assert.deepEqual(first, second);
  assert.equal(first.schemaVersion, 1);
  assert.equal(first.project.license, 'GPL-3.0-only');
  assert(first.rust.packages.some((packageRecord) => packageRecord.name === 'a3-domain'));
  assert(first.javascript.packages.some((packageRecord) => packageRecord.name === 'svelte'));
  assert.deepEqual(first.rust.summary.unknownLicensePackages, []);
  assert.deepEqual(first.javascript.summary.unknownLicensePackages, []);
  assert(!JSON.stringify(first).includes(repositoryRoot));
});

test('native UX smoke stays in every platform job without a shell-enabled Node runner', () => {
  const workflow = readFileSync(
    path.join(repositoryRoot, '.github', 'workflows', 'ci.yml'),
    'utf8',
  );
  const runner = readFileSync(
    path.join(repositoryRoot, 'scripts', 'run-desktop-ux-smoke.mjs'),
    'utf8',
  );

  for (const platform of ['linux-x86_64', 'windows-x86_64', 'macos-arm64', 'macos-x86_64']) {
    assert.match(workflow, new RegExp(`artifact: ${platform}`, 'u'));
  }
  assert.match(workflow, /node scripts\/run-desktop-ux-smoke\.mjs/u);
  assert.match(workflow, /desktop-ux-smoke-\$\{\{ matrix\.artifact \}\}/u);
  assert.match(workflow, /!cancelled\(\) && env\.ACT != 'true'/u);
  assert.doesNotMatch(runner, /shell\s*:\s*true/u);
  assert.match(runner, /WEBKIT_DISABLE_COMPOSITING_MODE: '1'/u);
  assert.match(runner, /env: desktopEnvironment/u);
  assert.match(runner, /screenshot\.size < 4096/u);
});
