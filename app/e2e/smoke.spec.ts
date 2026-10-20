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
  // No world yet: one was created and loaded at start.
  await expect(page.getByTestId('world')).toContainText('MY WORLD');
  await expect(page.getByRole('button', { name: 'Loaded' })).toBeDisabled();
  await page.getByRole('button', { name: 'Create world' }).first().click();
  const dlg = page.getByRole('dialog', { name: 'Create a world' });
  await dlg.getByLabel('World name').fill('alpha');
  await dlg.getByRole('radio', { name: '4×', exact: true }).click();
  await dlg.getByRole('switch', { name: 'Expert mode' }).check({ force: true });
  await dlg.getByRole('button', { name: 'Create world' }).click();
  await expect(page.getByTestId('world').filter({ hasText: 'ALPHA' })).toContainText('Loaded');
  await expect(page.getByTestId('world').filter({ hasText: 'MY WORLD' }).getByRole('button', { name: 'Load', exact: true })).toBeEnabled();
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
  await expect(page.locator('.app')).toHaveCSS('background-color', 'rgb(20, 17, 15)');
  await page.getByRole('radio', { name: 'Light' }).click();
  await expect(page.locator('.app')).toHaveCSS('background-color', 'rgb(251, 248, 244)');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  await go(page, 'Settings');
  await page.getByLabel('Language').selectOption('tr');
  await expect(page.getByRole('heading', { name: 'Ayarlar' })).toBeVisible();
  const saved = await page.evaluate(() => (window as any).__calls.filter((c: any[]) => c[0] === 'set_settings').at(-1)[1].settings.language);
  expect(saved).toBe('tr');
  await expect(page.locator('html')).toHaveAttribute('lang', 'tr');
});

/** Labels that are cut off: text wider than its box without wrapping (sidebar items, tiles, buttons, tabs). */
async function clipped(page: Page) {
  return page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>('nav button, nav a, button, [role="tab"], h1, h2, .k, .v, .t, label')]
      .filter((e) => e.offsetParent && !e.closest('.sr') && e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflow !== 'visible')
      .map((e) => e.textContent?.trim())
  );
}

for (const [lang, settings, worlds, hello] of [['ru', 'Настройки', 'Миры', 'Запустить сервер'], ['kk', 'Баптаулар', 'Әлемдер', 'Серверді іске қосу']] as const) {
  test(`switching to ${lang}: every screen translated, nothing overflows`, async ({ page }) => {
    await mockTauri(page, { worlds: true });
    await page.goto('/');
    await go(page, 'Settings');
    await page.getByLabel('Language').selectOption(lang);
    await expect(page.getByRole('heading', { name: settings })).toBeVisible();
    await expect(page.locator('html')).toHaveAttribute('lang', lang);
    const saved = await page.evaluate(() => (window as any).__calls.filter((c: any[]) => c[0] === 'set_settings').at(-1)[1].settings.language);
    expect(saved).toBe(lang);
    await expect(page.getByRole('button', { name: hello })).toHaveCount(0);
    const nav = page.locator('nav').first();
    await nav.getByRole('button', { name: new RegExp(worlds) }).click();
    await expect(page.getByRole('heading', { name: worlds, exact: true })).toBeVisible();
    for (let i = 0; i < 6; i++) {
      const items = page.locator('nav').first().getByRole('button');
      if (i >= (await items.count())) break;
      await items.nth(i).click();
      const over = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
      expect(over).toBe(false);
      expect(await clipped(page)).toEqual([]);
    }
  });
}

test('first launch follows the OS language', async ({ browser }) => {
  const ctx = await browser.newContext({ locale: 'ru-RU' });
  const page = await ctx.newPage();
  await mockTauri(page, { lang: '' });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Запустить сервер' }).first()).toBeVisible();
  const saved = await page.evaluate(() => (window as any).__calls.filter((c: any[]) => c[0] === 'set_settings').at(-1)[1].settings.language);
  expect(saved).toBe('ru');
  await ctx.close();
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

test('desktop: custom title bar, fixed shell, only the content scrolls', async ({ page }) => {
  test.skip(phone(page), 'the title bar is desktop only');
  await mockTauri(page);
  await page.goto('/');
  const bar = page.getByRole('banner', { name: 'Window title bar' });
  await expect(bar).toContainText('Blockheads Server · Dashboard');
  await expect(bar.getByRole('button', { name: 'Close window' })).toBeVisible();
  await go(page, 'Settings');
  await expect(bar).toContainText('· Settings');
  await page.setViewportSize({ width: 1000, height: 420 });
  const scrolled = await page.evaluate(() => ({
    page: document.documentElement.scrollHeight <= innerHeight,
    content: (document.querySelector('.content') as HTMLElement).scrollHeight > (document.querySelector('.content') as HTMLElement).clientHeight
  }));
  expect(scrolled).toEqual({ page: true, content: true });
  await expect(bar).toBeInViewport();
  await expect(page.getByRole('navigation', { name: 'Server' })).toBeInViewport();
});

test('phone: no title bar', async ({ page }) => {
  test.skip(!phone(page), 'phone only');
  await mockTauri(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Server' })).toBeVisible();
  await expect(page.locator('.bar')).toHaveCount(0);
});
