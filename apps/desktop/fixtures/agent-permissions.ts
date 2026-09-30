// Offline visual QA only. These synthetic permissions never authorize an actual tool.
import { mount } from 'svelte';
import PermissionsFixture from './PermissionsFixture.svelte';
import '../src/styles.css';
import designTokens from '../src/design-tokens.css?raw';

// Exercise the real theme tokens on two isolated fixture panels.
const themeStyle = document.createElement('style');
themeStyle.textContent = designTokens.replaceAll(':root', '[data-permission-theme]');
document.head.append(themeStyle);

const target = document.getElementById('app');
if (!target) throw new Error('Missing fixture mount');
mount(PermissionsFixture, { target });
