import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

const desktopRoot = process.cwd();
const workspaceRoot = resolve(desktopRoot, '../..');
const tauriConfig = JSON.parse(
  readFileSync(resolve(desktopRoot, 'src-tauri/tauri.conf.json'), 'utf8'),
) as {
  bundle?: { active?: boolean; icon?: string[] };
};
const packageManifest = JSON.parse(readFileSync(resolve(desktopRoot, 'package.json'), 'utf8')) as {
  scripts?: Record<string, string>;
};

const bundledIcons = [
  'icons/32x32.png',
  'icons/128x128.png',
  'icons/128x128@2x.png',
  'icons/icon.icns',
  'icons/icon.ico',
];

describe('desktop application icon contract', () => {
  it('uses the generated A^3 icon set for native application bundles', () => {
    expect(tauriConfig.bundle?.active).toBe(false);
    expect(tauriConfig.bundle?.icon).toEqual(bundledIcons);

    for (const icon of bundledIcons) {
      expect(existsSync(resolve(desktopRoot, 'src-tauri', icon))).toBe(true);
    }
  });

  it('keeps icon generation tied to the canonical vector logo', () => {
    const canonicalVectorLogo = resolve(workspaceRoot, 'docs/logo/A^3-Icon.Scharz-Weiß.svg');
    const canonicalRasterLogo = resolve(workspaceRoot, 'docs/logo/A^3-Icon.Schwarz-Weiß.png');

    expect(existsSync(canonicalVectorLogo)).toBe(true);
    expect(existsSync(canonicalRasterLogo)).toBe(true);
    expect(packageManifest.scripts?.icons).toBe(
      'tauri icon "../../docs/logo/A^3-Icon.Scharz-Weiß.svg"',
    );
  });

  it('uses a generated application icon for the WebView document', () => {
    const document = readFileSync(resolve(desktopRoot, 'index.html'), 'utf8');

    expect(document).toContain(
      '<link rel="icon" type="image/png" href="/src-tauri/icons/32x32.png" />',
    );
  });
});
