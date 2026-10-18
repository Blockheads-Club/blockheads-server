import { expect, test } from '@playwright/test';
import { mockTauri } from './mock';

const state = (over: object) => ({ boot: 'loading', error: '', version: '1.2.3', lang: 'en', ...over });

test('splash window: loading, slow and the data folder error with its actions', async ({ page }) => {
  await mockTauri(page, { splash: state({}) });
  await page.goto('/splash');
  await expect(page.getByRole('status', { name: 'Blockheads Server is starting' })).toBeVisible();
  await expect(page.getByText('Loading worlds and settings…')).toBeVisible();
  await expect(page.getByText('A Blockheads Club project · unofficial')).toBeVisible();
  await expect(page.getByText('1.2.3')).toBeVisible();
  await expect(page.getByRole('button')).toHaveCount(0);

  await mockTauri(page, { splash: state({ boot: 'slow' }) });
  await page.goto('/splash');
  await expect(page.getByText('Opening a big data folder — this can take a moment.')).toBeVisible();

  await mockTauri(page, { splash: state({ boot: 'error', error: '/data: Permission denied' }) });
  await page.goto('/splash');
  await expect(page.getByRole('alert')).toContainText('couldn’t open its data folder');
  await page.getByRole('button', { name: 'Choose folder…' }).click();
  await page.getByRole('button', { name: 'Try again' }).click();
  const calls = await page.evaluate(() => (window as any).__calls.map((c: any[]) => c[0]));
  expect(calls).toEqual(expect.arrayContaining(['choose_data_folder', 'splash_retry']));
});

test('main window tells the splash about its progress and reveals itself when ready', async ({ page }) => {
  await mockTauri(page);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: /Dashboard|Server/ }).first()).toBeVisible();
  const calls = await page.evaluate(() => (window as any).__calls.map((c: any[]) => c[0]));
  expect(calls).toEqual(expect.arrayContaining(['boot', 'splash_set', 'main_ready']));
});
