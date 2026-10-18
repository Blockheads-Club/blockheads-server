import { expect, test, type Page } from '@playwright/test';
import { mockTauri } from './mock';

const phone = (page: Page) => (page.viewportSize()?.width ?? 1000) < 600;

/** Open a screen from the sidebar (desktop) or the bottom bar (phone, with "More" for the rest). */
async function go(page: Page, name: 'Dashboard' | 'Worlds' | 'Console' | 'Players' | 'Access lists' | 'Settings') {
  const nav = page.getByRole('navigation', { name: 'Server' });
  if (phone(page) && (name === 'Access lists' || name === 'Settings')) {
    await nav.getByRole('button', { name: 'More' }).click();
    await page.getByRole('button', { name }).click();
  } else {
    await nav.getByRole('button', { name: new RegExp(`^${name}`) }).click();
  }
}

test('create a world, run the server, moderate, edit lists, change settings', async ({ page }) => {
  await mockTauri(page);
  await page.goto('/');

  await go(page, 'Worlds');
  await expect(page.getByRole('heading', { name: 'No worlds yet' })).toBeVisible();
  await page.getByRole('button', { name: 'Create world' }).first().click();
  const dlg = page.getByRole('dialog', { name: 'Create a world' });
  await dlg.getByLabel('World name').fill('alpha');
  await dlg.getByRole('radio', { name: '4×', exact: true }).click();
  await dlg.getByRole('switch', { name: 'Expert mode' }).check({ force: true });
  await dlg.getByRole('button', { name: 'Create world' }).click();
  await expect(page.getByTestId('world')).toContainText('ALPHA');
  await expect(page.getByText('World “alpha” created')).toBeVisible();

  await go(page, 'Dashboard');
  await expect(page.getByTestId('state')).toHaveText('Stopped');
  await expect(page.getByText('Start the server to let players join')).toBeVisible();
  await page.getByRole('button', { name: 'Start server' }).click();
  await expect(page.getByTestId('state')).toHaveText('Running');
  await expect(page.getByTestId('addr')).toHaveText('192.168.1.42');
  await expect(page.getByTestId('qr').locator('svg')).toBeVisible();
  await expect(page.getByText('2h 14m')).toBeVisible();
  await expect(page.getByTestId('player')).toContainText('MIRA_K');

  await go(page, 'Console');
  await expect(page.getByTestId('log')).toContainText('World load complete.');
  await page.getByLabel('Search log').fill('mira');
  await expect(page.getByText('1 of 2')).toBeVisible();
  await page.getByLabel('Search log').fill('');
  await page.getByRole('button', { name: 'Errors' }).click();
  await expect(page.getByTestId('log')).toContainText('Save took 6.2 s');
  await page.getByRole('button', { name: 'All' }).click();
  await page.getByLabel('Server command').fill('/players');
  await page.getByRole('button', { name: 'Run' }).click();
  await expect(page.getByTestId('log')).toContainText('/players');
  await page.getByLabel('Server command').press('ArrowUp');
  await expect(page.getByLabel('Server command')).toHaveValue('/players');

  await go(page, 'Players');
  await expect(page.getByTestId('player')).toContainText('MIRA_K');
  await page.getByRole('button', { name: 'Actions for MIRA_K' }).click();
  await page.getByRole('menuitem', { name: 'Remove admin' }).click();
  await page.getByRole('button', { name: 'Actions for MIRA_K' }).click();
  await page.getByRole('menuitem', { name: 'Make mod' }).click();
  await expect(page.getByText('MIRA_K is now a mod')).toBeVisible();

  await go(page, 'Access lists');
  await expect(page.getByRole('heading', { name: /Admins/ })).toBeVisible();
  await page.getByLabel('Add to whitelist').fill('Notch');
  await page.getByRole('button', { name: 'Add' }).nth(2).click();
  await expect(page.getByText('Notch', { exact: true })).toBeVisible();

  await go(page, 'Settings');
  await page.getByLabel('Port').fill('16000');
  await expect(page.getByRole('region', { name: 'Unsaved changes' })).toContainText('1 unsaved change');
  await page.getByRole('button', { name: 'Save & restart' }).click();
  await expect(page.getByText('Settings saved')).toBeVisible();
  const calls = await page.evaluate(() => (window as any).__calls.filter((c: any[]) => c[0] === 'start_server').at(-1)[1].options.port);
  expect(calls).toBe(16000);

  await go(page, 'Dashboard');
  await page.getByRole('button', { name: 'Stop server' }).click();
  await expect(page.getByTestId('state')).toHaveText('Stopped');
});

test('theme switching and Turkish', async ({ page }) => {
  await mockTauri(page, { worlds: true });
  await page.goto('/');
  await go(page, 'Settings');
  await page.getByRole('radio', { name: 'Dark' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(20, 17, 15)');
  await page.getByRole('radio', { name: 'Light' }).click();
  await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(251, 248, 244)');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  await go(page, 'Settings');
  await page.getByLabel('Language').selectOption('tr');
  await expect(page.getByRole('heading', { name: 'Ayarlar' })).toBeVisible();
  const saved = await page.evaluate(() => (window as any).__calls.filter((c: any[]) => c[0] === 'set_settings').at(-1)[1].settings.language);
  expect(saved).toBe('tr');
  await expect(page.locator('html')).toHaveAttribute('lang', 'tr');
});

test('no horizontal overflow on any screen', async ({ page }) => {
  await mockTauri(page, { worlds: true });
  await page.goto('/');
  for (const name of ['Dashboard', 'Worlds', 'Console', 'Players', 'Access lists', 'Settings'] as const) {
    await go(page, name);
    const over = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
    expect(over, name).toBe(false);
  }
});
