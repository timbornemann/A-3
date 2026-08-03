import desktopConfig from './apps/desktop/eslint.config.js';

export default [
  {
    ignores: ['node_modules/**', 'target/**', 'apps/desktop/dist/**', 'apps/desktop/src-tauri/**'],
  },
  ...desktopConfig,
];
