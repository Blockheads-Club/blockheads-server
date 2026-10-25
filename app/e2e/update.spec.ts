import { expect, test, type Page } from '@playwright/test';
import { mockTauri } from './mock';

const phone = (page: Page) => (page.viewportSize()?.width ?? 1000) < 600;
const calls = (page: Page) => page.evaluate(() => ((window as any).__calls as [string, unknown][]).map((c) => c[0]));

test('desktop: the update popup downloads and restarts; the title bar keeps the state', async ({ page }) => {
  test.skip(phone(page), 'desktop flow');
  await mockTauri(page, { update: true });
  await page.goto('/');
  const dlg = page.getByRole('dialog', { name: 'Server 2.0.0 is here' });
  await expect(dlg).toBeVisible();
  await expect(dlg.getByRole('region', { name: 'What’s new' })).toContainText('Faster saves');
  await dlg.getByRole('button', { name: 'Later' }).click();
  const chip = page.getByRole('button', { name: 'Update available: open the update window' });
  await expect(chip).toBeVisible();
  await expect(page.getByTestId('update-button')).toHaveAccessibleName('Update available: 2.0.0');
  await chip.click();
  await page.getByRole('button', { name: 'Download update' }).click();
  await expect(page.getByRole('dialog', { name: 'Update downloaded' })).toBeVisible();
  await expect(page.getByText('The app must restart to use it.')).toBeVisible();
  await page.getByRole('button', { name: 'Restart now' }).click();
  await expect(page.getByText('Opening the installer for 2.0.0')).toBeVisible();
  await expect.poll(() => calls(page)).toContain('app_quit');
  expect(await calls(page)).toContain('update_install');
});

test('settings: check now and the automatic options', async ({ page }) => {
  await mockTauri(page);
  await page.goto('/');
  const nav = page.getByRole('navigation', { name: 'Server' });
  if (phone(page)) {
    await nav.getByRole('button', { name: 'More' }).click();
    await expect(page.getByTestId('update-button')).toHaveAccessibleName('Check for updates');
    await page.getByRole('button', { name: 'Settings' }).click();
  } else {
    await nav.getByRole('button', { name: /^Settings/ }).click();
  }
  await page.getByRole('button', { name: 'Check now' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Up to date' })).toBeVisible();
  await expect(page.getByText('Last checked: just now')).toBeVisible();
  await page.getByRole('combobox', { name: 'Versions' }).selectOption('beta');
  expect(await calls(page)).toContain('set_settings');
});

test('phone: the update downloads in the app and hands the APK to the installer', async ({ page }) => {
  test.skip(!phone(page), 'phone only');
  await mockTauri(page, { update: true });
  await page.goto('/');
  await page.getByRole('dialog', { name: 'Server 2.0.0 is here' }).getByRole('button', { name: 'Download and install' }).click();
  await expect(page.getByRole('dialog', { name: 'Update downloaded' })).toBeVisible();
  await page.getByRole('button', { name: 'Restart now' }).click();
  await expect.poll(() => calls(page)).toContain('app_quit');
  const names = await calls(page);
  expect(names).toContain('update_install');
  expect(names).not.toContain('open_url');
});
