import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ignoredDirectories = new Set(['.git', 'node_modules', 'target']);
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function stripInlineCode(line) {
  return line.replace(/(`+)(.*?)\1/g, (match) => ' '.repeat(match.length));
}

function markdownLinesOutsideFences(markdown) {
  const result = [];
  let fenceCharacter;
  let fenceLength = 0;

  for (const [index, line] of markdown.split(/\r?\n/u).entries()) {
    const marker = line.match(/^\s{0,3}(`{3,}|~{3,})/u)?.[1];
    if (marker) {
      if (!fenceCharacter) {
        fenceCharacter = marker[0];
        fenceLength = marker.length;
      } else if (marker[0] === fenceCharacter && marker.length >= fenceLength) {
        fenceCharacter = undefined;
        fenceLength = 0;
      }
      continue;
    }

    if (!fenceCharacter) {
      result.push({ line: stripInlineCode(line), lineNumber: index + 1 });
    }
  }

  return result;
}

export function extractMarkdownLinks(markdown) {
  const links = [];
  const inlineLink =
    /!?\[[^\]\n]*\]\(\s*(?:<([^>\n]+)>|((?:\\.|[^)\s])+))(?:\s+(?:"[^"\n]*"|'[^'\n]*'|\([^\n)]*\)))?\s*\)/gu;
  const referenceDefinition = /^\s{0,3}\[[^\]]+\]:\s*(?:<([^>]+)>|(\S+))/u;

  for (const { line, lineNumber } of markdownLinesOutsideFences(markdown)) {
    for (const match of line.matchAll(inlineLink)) {
      if (match.index > 0 && line[match.index - 1] === '\\') {
        continue;
      }
      links.push({ lineNumber, target: match[1] ?? match[2] });
    }

    const definition = line.match(referenceDefinition);
    if (definition) {
      links.push({ lineNumber, target: definition[1] ?? definition[2] });
    }
  }

  return links;
}

function headingSlug(heading) {
  return heading
    .replace(/!\[([^\]]*)\]\([^)]*\)/gu, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/gu, '$1')
    .replace(/<[^>]+>/gu, '')
    .replace(/[`*_~]/gu, '')
    .trim()
    .toLocaleLowerCase('en-US')
    .replace(/[^\p{Letter}\p{Number}\p{Mark}\s_-]/gu, '')
    .replace(/\s/gu, '-');
}

export function markdownAnchors(markdown) {
  const anchors = new Set();
  const slugCounts = new Map();

  for (const { line } of markdownLinesOutsideFences(markdown)) {
    const heading = line.match(/^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$/u)?.[1];
    if (heading) {
      const baseSlug = headingSlug(heading);
      const count = slugCounts.get(baseSlug) ?? 0;
      anchors.add(count === 0 ? baseSlug : `${baseSlug}-${count}`);
      slugCounts.set(baseSlug, count + 1);
    }

    for (const match of line.matchAll(/<a\s+[^>]*(?:id|name)=["']([^"']+)["'][^>]*>/giu)) {
      anchors.add(match[1]);
    }
  }

  return anchors;
}

function isWithin(rootDirectory, candidate) {
  const relative = path.relative(rootDirectory, candidate);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..');
}

function hasExactCase(rootDirectory, candidate) {
  const relative = path.relative(rootDirectory, candidate);
  let current = rootDirectory;

  for (const segment of relative.split(path.sep).filter(Boolean)) {
    const entries = fs.readdirSync(current);
    if (!entries.includes(segment)) {
      return false;
    }
    current = path.join(current, segment);
  }

  return true;
}

function decodeLinkPart(value) {
  return decodeURIComponent(value.replace(/\\([\\() ])/gu, '$1'));
}

export function validateLocalTarget(rootDirectory, sourceFile, rawTarget) {
  const target = rawTarget.trim();
  if (!target || target.startsWith('//')) {
    return { local: false };
  }
  if (/^file:/iu.test(target)) {
    return { local: true, error: 'file: links are not portable repository links' };
  }
  if (/^[a-z][a-z\d+.-]*:/iu.test(target)) {
    return { local: false };
  }

  const hashIndex = target.indexOf('#');
  const targetWithoutFragment = hashIndex >= 0 ? target.slice(0, hashIndex) : target;
  const rawFragment = hashIndex >= 0 ? target.slice(hashIndex + 1) : undefined;
  const queryIndex = targetWithoutFragment.indexOf('?');
  const rawPath =
    queryIndex >= 0 ? targetWithoutFragment.slice(0, queryIndex) : targetWithoutFragment;

  let decodedPath;
  let fragment;
  try {
    decodedPath = decodeLinkPart(rawPath);
    fragment = rawFragment === undefined ? undefined : decodeLinkPart(rawFragment);
  } catch {
    return { local: true, error: 'contains invalid percent encoding' };
  }

  const candidate = decodedPath
    ? path.resolve(
        decodedPath.startsWith('/') ? rootDirectory : path.dirname(sourceFile),
        decodedPath.replace(/^\/+/, ''),
      )
    : sourceFile;
  if (!isWithin(rootDirectory, candidate)) {
    return { local: true, error: 'resolves outside the repository' };
  }
  if (!fs.existsSync(candidate)) {
    return { local: true, error: 'target does not exist' };
  }
  if (!hasExactCase(rootDirectory, candidate)) {
    return { local: true, error: 'target path has incorrect letter case' };
  }

  const realCandidate = fs.realpathSync(candidate);
  if (!isWithin(fs.realpathSync(rootDirectory), realCandidate)) {
    return { local: true, error: 'target symlink resolves outside the repository' };
  }

  if (fragment && /\.md(?:own)?$/iu.test(candidate)) {
    const anchors = markdownAnchors(fs.readFileSync(candidate, 'utf8'));
    if (!anchors.has(fragment)) {
      return { local: true, error: `heading anchor #${fragment} does not exist` };
    }
  }

  return { local: true };
}

function collectMarkdownFiles(directory) {
  const files = [];
  for (const entry of fs
    .readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name))) {
    if (entry.isSymbolicLink() || (entry.isDirectory() && ignoredDirectories.has(entry.name))) {
      continue;
    }

    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectMarkdownFiles(entryPath));
    } else if (entry.isFile() && /\.md(?:own)?$/iu.test(entry.name)) {
      files.push(entryPath);
    }
  }
  return files;
}

export function checkMarkdownTree(rootDirectory = repositoryRoot) {
  const failures = [];
  let localLinkCount = 0;
  const markdownFiles = collectMarkdownFiles(rootDirectory);

  for (const sourceFile of markdownFiles) {
    const markdown = fs.readFileSync(sourceFile, 'utf8');
    for (const link of extractMarkdownLinks(markdown)) {
      const validation = validateLocalTarget(rootDirectory, sourceFile, link.target);
      if (!validation.local) {
        continue;
      }
      localLinkCount += 1;
      if (validation.error) {
        failures.push({
          error: validation.error,
          lineNumber: link.lineNumber,
          sourceFile,
          target: link.target,
        });
      }
    }
  }

  return { failures, localLinkCount, markdownFileCount: markdownFiles.length };
}

function run() {
  const result = checkMarkdownTree();
  for (const failure of result.failures) {
    const relativeSource = path.relative(repositoryRoot, failure.sourceFile);
    console.error(`${relativeSource}:${failure.lineNumber}: ${failure.target}: ${failure.error}`);
  }
  if (result.failures.length > 0) {
    process.exitCode = 1;
    return;
  }
  console.log(
    `Checked ${result.markdownFileCount} Markdown files and ${result.localLinkCount} local links.`,
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : undefined;
if (invokedPath === import.meta.url) {
  run();
}
