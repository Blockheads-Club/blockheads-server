import { test, type Page } from '@playwright/test';
import { mockTauri } from './mock';

// Screenshots for review (not part of CI): SHOTS_DIR=... bunx playwright test e2e/shots.spec.ts
const dir = process.env.SHOTS_DIR;
test.skip(!dir, 'SHOTS_DIR not set');

async function open(page: Page, theme: string, w: number, h: number) {
  await page.setViewportSize({ width: w, height: h });
  await page.addInitScript((t) => localStorage.setItem('bh.theme', t), theme);
  await mockTauri(page, { worlds: true, mods: 'demo' });
  await page.goto('/');
}
const nav = (page: Page, name: RegExp) => page.getByRole('navigation', { name: 'Server' }).getByRole('button', { name }).click();
const shot = (page: Page, name: string) => page.screenshot({ path: `${dir}/${name}.png` });

test('desktop', async ({ page }, info) => {
  test.skip(info.project.name !== 'desktop');
  for (const theme of ['light', 'dark']) {
    await open(page, theme, 1280, 1060);
    await page.getByRole('button', { name: 'Start server' }).click();
    await page.waitForTimeout(800);
    await shot(page, `dashboard-${theme}`);
    await nav(page, /^Mods/);
    await page.waitForTimeout(300);
    await shot(page, `list-${theme}`);
    await page.getByRole('button', { name: 'Weather Plus', exact: true }).click();
    await shot(page, `detail-budget-${theme}`);
    await page.getByRole('button', { name: 'All mods' }).click();
    await page.getByRole('button', { name: 'Fast Tools', exact: true }).click();
    await shot(page, `detail-native-${theme}`);
    await page.getByRole('button', { name: 'All mods' }).click();
    await page.getByRole('button', { name: 'Browse mod list' }).click();
    await page.waitForTimeout(300);
    await shot(page, `browse-${theme}`);
    await page.getByRole('button', { name: 'Install Auction House' }).click();
    await shot(page, `install-plan-${theme}`);
    await page.getByRole('dialog').getByRole('button', { name: 'Continue' }).click();
    await shot(page, `install-perms-${theme}`);
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Install Turbo Lighting' }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Install and turn on' }).click();
    await shot(page, `install-native-${theme}`);
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Install from file or link' }).click();
    await page.getByLabel('Or paste a link').fill('https://github.com/blockmonk/fast-tools/releases/fast-tools-0.4.0.bhmod');
    await shot(page, `install-file-${theme}`);
    await page.getByRole('dialog').getByRole('button', { name: 'Continue' }).click();
    await shot(page, `install-failed-${theme}`);
  }
});

test('phone', async ({ page }, info) => {
  test.skip(info.project.name !== 'phone');
  await open(page, 'light', 390, 844);
  await page.getByRole('button', { name: 'Start server' }).click();
  await nav(page, /^Mods/);
  await page.waitForTimeout(300);
  await shot(page, 'phone-list');
  await page.getByRole('button', { name: 'Weather Plus', exact: true }).click();
  await shot(page, 'phone-detail');
  await page.getByRole('button', { name: 'All mods' }).click();
  await page.getByRole('button', { name: 'Add mod' }).click();
  await page.waitForTimeout(300);
  await shot(page, 'phone-browse');
  await page.getByRole('button', { name: 'Install Pet Companions' }).click();
  await shot(page, 'phone-install-sheet');
});
