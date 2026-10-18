import type { ThemeChoice } from './types';
import { storage } from './util';

const KEY = 'bh.theme';
export const theme = $state<{ choice: ThemeChoice }>({ choice: 'system' });

function apply(choice: ThemeChoice) {
  if (choice === 'system') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', choice);
}

/** Restore the saved choice (default: follow the OS). */
export function initTheme() {
  const saved = storage((s) => s.getItem(KEY), null);
  theme.choice = saved === 'light' || saved === 'dark' ? saved : 'system';
  apply(theme.choice);
}

export function setTheme(choice: ThemeChoice) {
  theme.choice = choice;
  apply(choice);
  storage((s) => s.setItem(KEY, choice), undefined);
}
