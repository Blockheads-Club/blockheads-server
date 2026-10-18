import type { IconName } from '../icons';
import type { Screen } from '../types';

export interface NavItem {
  id: Screen;
  icon: IconName;
}

/** Desktop / tablet navigation (design: Dashboard, Worlds, Console, Players, Access lists, Settings). */
export const NAV: NavItem[] = [
  { id: 'dashboard', icon: 'gauge' },
  { id: 'worlds', icon: 'map' },
  { id: 'console', icon: 'terminal' },
  { id: 'players', icon: 'users' },
  { id: 'access', icon: 'shield' },
  { id: 'settings', icon: 'sliders' }
];

/** Phone bottom bar: four items and "More" (Access lists, Settings). */
export const PHONE_NAV: NavItem[] = [
  { id: 'dashboard', icon: 'gauge' },
  { id: 'worlds', icon: 'map' },
  { id: 'console', icon: 'terminal' },
  { id: 'players', icon: 'users' },
  { id: 'more', icon: 'menu' }
];
