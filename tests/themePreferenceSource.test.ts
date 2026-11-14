import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const read = (path: string) =>
  readFileSync(new URL(path, import.meta.url), 'utf8');

test('ADC beige theme remains selectable alongside dark mode', () => {
  const settings = read('../src/stores/settingsStore.ts');
  const modal = read('../src/components/modals/SettingsModal.tsx');
  const css = read('../src/index.css');
  const app = read('../src/App.tsx');

  assert.match(settings, /theme:\s*'dark'/);
  assert.match(modal, /<option value="dark">Dark Space \/ Charcoal<\/option>/);
  assert.match(modal, /<option value="light">ADC Beige \/ Light<\/option>/);
  assert.match(app, /root\.dataset\.theme = light \? 'light' : 'dark'/);
  assert.match(css, /:root\[data-theme='light'\][\s\S]*--app-bg:\s*#f3eee4/);
  assert.match(css, /--text-primary:\s*#28241e/);
});
