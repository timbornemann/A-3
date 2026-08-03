import sveltePlugin from 'prettier-plugin-svelte';

export default {
  plugins: [sveltePlugin],
  printWidth: 100,
  semi: true,
  singleQuote: true,
  tabWidth: 2,
  trailingComma: 'all',
  overrides: [
    {
      files: '*.svelte',
      options: {
        parser: 'svelte',
      },
    },
  ],
};
