<script lang="ts">
  import { onMount } from 'svelte';
  import { applyThemePreference, type ThemePreference } from './theme';

  let preference = $state<ThemePreference>('system');

  onMount(() => {
    applyThemePreference(preference);
  });

  function selectTheme(next: ThemePreference): void {
    preference = next;
    applyThemePreference(next);
  }
</script>

<fieldset class="theme-controls">
  <legend>Farbschema</legend>
  <button type="button" aria-pressed={preference === 'system'} onclick={() => selectTheme('system')}
    >System</button
  >
  <button type="button" aria-pressed={preference === 'light'} onclick={() => selectTheme('light')}
    >Hell</button
  >
  <button type="button" aria-pressed={preference === 'dark'} onclick={() => selectTheme('dark')}
    >Dunkel</button
  >
</fieldset>
