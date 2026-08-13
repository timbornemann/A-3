import { execFile, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);
const workspaceRoot = process.cwd();
const artifactDirectory = path.join(workspaceRoot, 'target', 'platform-smoke');
const screenshotPath = path.join(artifactDirectory, `${process.platform}.png`);
const reportPath = path.join(artifactDirectory, `${process.platform}.json`);
const binaryPath = path.join(
  workspaceRoot,
  'target',
  'release',
  process.platform === 'win32' ? 'a3-desktop.exe' : 'a3-desktop',
);
const timeoutSeconds = 30;
const retainedOutputLimit = 16 * 1024;

function retainOutput(current, chunk) {
  return `${current}${chunk.toString('utf8')}`.slice(-retainedOutputLimit);
}

async function runTool(command, args, timeout = timeoutSeconds * 1000) {
  return execFileAsync(command, args, {
    cwd: workspaceRoot,
    encoding: 'utf8',
    maxBuffer: 1024 * 1024,
    timeout,
    windowsHide: true,
  });
}

async function captureWindows(processId) {
  const scriptPath = path.join(workspaceRoot, 'scripts', 'capture-a3-window.ps1');
  const { stdout } = await runTool('powershell.exe', [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-ExecutionPolicy',
    'Bypass',
    '-File',
    scriptPath,
    '-ProcessId',
    String(processId),
    '-OutputPath',
    screenshotPath,
    '-TimeoutSeconds',
    String(timeoutSeconds),
  ]);
  return JSON.parse(stdout.trim());
}

async function captureLinux(processId) {
  const { stdout: searchOutput } = await runTool('xdotool', [
    'search',
    '--sync',
    '--onlyvisible',
    '--pid',
    String(processId),
    '--name',
    'A\\^3',
  ]);
  const windowId = searchOutput.trim().split(/\s+/u)[0];
  if (!/^\d+$/u.test(windowId)) throw new Error('xdotool returned no A^3 window identifier.');

  const { stdout: geometryOutput } = await runTool('xdotool', [
    'getwindowgeometry',
    '--shell',
    windowId,
  ]);
  const width = Number(/^WIDTH=(\d+)$/mu.exec(geometryOutput)?.[1]);
  const height = Number(/^HEIGHT=(\d+)$/mu.exec(geometryOutput)?.[1]);
  if (width < 720 || height < 520) {
    throw new Error(
      `The A^3 native window is smaller than its minimum product viewport: ${width}x${height}.`,
    );
  }

  await new Promise((resolve) => setTimeout(resolve, 1500));
  await runTool('import', ['-window', windowId, screenshotPath]);
  const { stdout: imageOutput } = await runTool('identify', [
    '-format',
    '%w|%h|%[fx:standard_deviation]',
    screenshotPath,
  ]);
  const [imageWidth, imageHeight, standardDeviation] = imageOutput.trim().split('|').map(Number);
  if (imageWidth < 720 || imageHeight < 520 || !(standardDeviation > 0.01)) {
    throw new Error('The A^3 native WebKitGTK screenshot is empty or has invalid dimensions.');
  }
  return { height, imageHeight, imageWidth, processId, standardDeviation, width, windowId };
}

async function captureMacOs(processId) {
  const helperPath = path.join(workspaceRoot, 'scripts', 'find-a3-window.swift');
  const { stdout } = await runTool(
    'swift',
    [helperPath, String(processId), String(timeoutSeconds)],
    (timeoutSeconds + 15) * 1000,
  );
  const [windowId, widthValue, heightValue, ...titleParts] = stdout.trim().split('|');
  const width = Number(widthValue);
  const height = Number(heightValue);
  if (!/^\d+$/u.test(windowId) || width < 720 || height < 520) {
    throw new Error('CoreGraphics returned an invalid A^3 window projection.');
  }

  await new Promise((resolve) => setTimeout(resolve, 1500));
  await runTool('/usr/sbin/screencapture', ['-x', '-l', windowId, screenshotPath]);
  const { stdout: imageOutput } = await runTool('/usr/bin/sips', [
    '-g',
    'pixelWidth',
    '-g',
    'pixelHeight',
    screenshotPath,
  ]);
  const imageWidth = Number(/pixelWidth:\s+(\d+)/u.exec(imageOutput)?.[1]);
  const imageHeight = Number(/pixelHeight:\s+(\d+)/u.exec(imageOutput)?.[1]);
  if (imageWidth < 720 || imageHeight < 520) {
    throw new Error('The A^3 native WKWebView screenshot has invalid dimensions.');
  }
  return {
    height,
    imageHeight,
    imageWidth,
    processId,
    title: titleParts.join('|'),
    width,
    windowId,
  };
}

async function stopDesktop(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill('SIGTERM');
  await Promise.race([once(child, 'exit'), new Promise((resolve) => setTimeout(resolve, 5000))]);
  if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
}

await mkdir(artifactDirectory, { recursive: true });
const child = spawn(binaryPath, [], {
  cwd: workspaceRoot,
  env: process.env,
  stdio: ['ignore', 'pipe', 'pipe'],
  windowsHide: false,
});
let stdout = '';
let stderr = '';
child.stdout.on('data', (chunk) => {
  stdout = retainOutput(stdout, chunk);
});
child.stderr.on('data', (chunk) => {
  stderr = retainOutput(stderr, chunk);
});

try {
  const platformResult =
    process.platform === 'win32'
      ? await captureWindows(child.pid)
      : process.platform === 'linux'
        ? await captureLinux(child.pid)
        : process.platform === 'darwin'
          ? await captureMacOs(child.pid)
          : (() => {
              throw new Error(`Unsupported desktop smoke platform: ${process.platform}.`);
            })();
  const screenshot = await stat(screenshotPath);
  if (screenshot.size < 4096) throw new Error('The native A^3 screenshot is unexpectedly small.');

  const report = {
    binary: path.relative(workspaceRoot, binaryPath).replaceAll('\\', '/'),
    platform: process.platform,
    platformResult,
    screenshotBytes: screenshot.size,
  };
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  process.stdout.write(`${JSON.stringify(report)}\n`);
} catch (error) {
  const detail = error instanceof Error ? error.message : String(error);
  throw new Error(
    `Native A^3 UX smoke failed: ${detail}\nretained stdout:\n${stdout}\nretained stderr:\n${stderr}`,
    { cause: error },
  );
} finally {
  await stopDesktop(child);
}
