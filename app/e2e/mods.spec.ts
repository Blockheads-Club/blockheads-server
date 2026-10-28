import { expect, test, type Page } from '@playwright/test';
import { mockTauri } from './mock';

const phone = (page: Page) => (page.viewportSize()?.width ?? 1000) < 600;

test('mods: list, filters, detail, browse, install with dependencies and native confirm, link errors', async ({ page }) => {
  await mockTauri(page, { worlds: true, mods: 'demo' });
  await page.goto('/');
  await page.getByRole('navigation', { name: 'Server' }).getByRole('button', { name: /^Mods/ }).click();
  await expect(page.getByRole('heading', { name: 'Mods', level: 1 })).toBeVisible();
  await expect(page.getByText('Players need the mod loader to join this server')).toBeVisible();
  await expect(page.getByTestId('mod-row')).toHaveCount(8);
  await page.getByRole('button', { name: /^Needs attention/ }).click();
  await expect(page.getByTestId('mod-row')).toHaveCount(3);
  await page.getByRole('button', { name: /^All/ }).first().click();

  // A mod's page: permissions are granted from here.
  await page.getByRole('button', { name: 'Discord Bridge', exact: true }).click();
  await expect(page.getByRole('heading', { name: /Discord Bridge/ })).toBeVisible();
  await expect(page.getByText('Waiting for your permission')).toBeVisible();
  if (phone(page)) await page.getByRole('switch', { name: 'Internet: discord.com: not granted' }).check({ force: true });
  else await page.getByRole('button', { name: 'Grant' }).click();
  await expect(page.getByText('Waiting for your permission')).toHaveCount(0);
  await page.getByRole('button', { name: 'All mods' }).click();

  // The public list: Auction House brings Economy Core along.
  if (phone(page)) await page.getByRole('button', { name: 'Add mod' }).click();
  else await page.getByRole('button', { name: 'Browse mod list' }).click();
  await expect(page.getByRole('heading', { name: 'Mod list' })).toBeVisible();
  await expect(page.getByTestId('browse-card')).toHaveCount(5);
  await page.getByRole('button', { name: 'Install Auction House' }).click();
  const dlg = page.getByRole('dialog');
  await expect(dlg.getByText('Step 1 of 2 · What gets installed')).toBeVisible();
  await expect(dlg.getByTestId('plan-item')).toHaveCount(2);
  await dlg.getByRole('button', { name: 'Continue' }).click();
  await expect(dlg.getByText('Step 2 of 2 · Permissions')).toBeVisible();
  await dlg.getByRole('button', { name: 'Install and turn on' }).click();
  await expect(dlg.getByRole('heading', { name: 'Auction House is installed' })).toBeVisible();
  await dlg.getByRole('button', { name: 'Close', exact: true }).last().click();
  await expect(page.getByTestId('browse-card').filter({ hasText: 'Auction House' })).toContainText('Installed');

  // Native code asks once more; installing stays off until the box is ticked.
  await page.getByRole('button', { name: 'Install Turbo Lighting' }).click();
  await dlg.getByRole('button', { name: 'Install and turn on' }).click();
  await expect(dlg.getByRole('group', { name: 'Danger: native code' })).toBeVisible();
  await expect(dlg.getByRole('button', { name: 'Don’t install' })).toBeFocused();
  await expect(dlg.getByRole('button', { name: 'Install native mod' })).toBeDisabled();
  await dlg.getByText(/I trust the author/).click();
  await dlg.getByRole('button', { name: 'Install native mod' }).click();
  await expect(dlg.getByRole('heading', { name: 'Turbo Lighting is installed' })).toBeVisible();
  await page.keyboard.press('Escape');

  // A link whose file does not match.
  await page.getByRole('button', { name: 'Install from file or link' }).click();
  await expect(dlg.getByText('Not from the public list')).toBeVisible();
  await dlg.getByLabel('Or paste a link').fill('https://github.com/blockmonk/fast-tools/releases/fast-tools-0.4.0.bhmod');
  await dlg.getByRole('button', { name: 'Continue' }).click();
  await expect(dlg.getByRole('heading', { name: 'The file check failed' })).toBeVisible();
  await expect(dlg.getByText('Nothing was installed.')).toBeVisible();
});

test('dashboard shows the mods card', async ({ page }) => {
  await mockTauri(page, { worlds: true, mods: 'demo' });
  await page.goto('/');
  const card = page.getByRole('region', { name: 'Mods' });
  await expect(card).toContainText('players need the mod loader');
  await card.getByRole('button', { name: 'Manage mods' }).click();
  await expect(page.getByRole('heading', { name: 'Mods', level: 1 })).toBeVisible();
});

test('mods: a mod’s settings tab edits, validates and saves', async ({ page }) => {
  await mockTauri(page, { worlds: true, mods: 'demo' });
  await page.goto('/');
  await page.getByRole('navigation', { name: 'Server' }).getByRole('button', { name: /^Mods/ }).click();
  await page.getByRole('button', { name: 'Better Chat', exact: true }).click();
  await page.getByRole('tab', { name: 'Settings' }).click();
  await expect(page.getByRole('heading', { name: 'Chat', level: 2 })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Roles', level: 2 })).toBeVisible();
  await expect(page.getByText('Mention sound')).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'Message of the day' })).toHaveValue('Welcome!');
  await expect(page.getByText('Needs restart')).toBeVisible();

  // Client-side rules first: 61 is over the maximum (the step of 5 is only a hint).
  const slow = page.getByRole('spinbutton', { name: 'Slow mode (seconds)' });
  await slow.fill('61');
  await expect(page.getByText('Must be at most 60.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Save settings' })).toBeDisabled();
  await slow.fill('10');
  await page.getByRole('combobox', { name: 'Name style' }).selectOption('tag');
  await page.getByRole('group', { name: 'Who may use colors' }).getByRole('button', { name: 'Moderators' }).click();
  await page.getByRole('checkbox', { name: 'announce: Admins' }).check();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await page.getByRole('textbox', { name: 'Name #1' }).fill('builders');
  // The backend refuses a link in the message of the day: shown next to the field, the rest is saved.
  await page.getByRole('textbox', { name: 'Message of the day' }).fill('see https://x');
  await expect(page.getByText('6 unsaved changes')).toBeVisible();
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.getByText('This value isn’t allowed.')).toBeVisible();
  await expect(page.getByText('1 unsaved change')).toBeVisible();
  await page.getByRole('button', { name: 'Reset Message of the day to default' }).click();
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.getByText('All changes saved')).toBeVisible();

  // Saved values come back after leaving the mod.
  await page.getByRole('button', { name: 'All mods' }).click();
  await page.getByRole('button', { name: 'Better Chat', exact: true }).click();
  await page.getByRole('tab', { name: 'Settings' }).click();
  await expect(page.getByRole('spinbutton', { name: 'Slow mode (seconds)' })).toHaveValue('10');
  await expect(page.getByRole('textbox', { name: 'Name #1' })).toHaveValue('builders');
});

test('mods: the admin lets players join without a client mod', async ({ page }) => {
  await mockTauri(page, { worlds: true, mods: 'demo' });
  await page.goto('/');
  await page.getByRole('navigation', { name: 'Server' }).getByRole('button', { name: /^Mods/ }).click();
  await page.getByRole('button', { name: 'Minimap', exact: true }).click();
  const sec = page.getByRole('main');
  await expect(sec.getByRole('heading', { name: 'Players must have this mod' })).toBeVisible();
  await expect(sec.getByText('The mod’s own default is required.')).toBeVisible();
  await sec.getByRole('switch', { name: 'required' }).uncheck({ force: true });
  await expect(page.getByText('Saved. It applies after a restart.')).toBeVisible();
  await expect(sec.getByRole('switch', { name: 'optional' })).not.toBeChecked();
});
