import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ api: {} as Record<string, any> }));
vi.mock('../api', () => ({ api: new Proxy({}, { get: (_t, k: string) => mock.api[k] }) }));

import { setLang } from '../i18n.svelte';
import { W1, makeApi } from '../mock-api';
import { store } from '../store.svelte';
import JvmSettings from './JvmSettings.svelte';

const run = { worldId: W1, port: 15151, maxPlayers: 0, password: '', saveDelay: 10 };

beforeEach(() => {
  mock.api = makeApi();
  setLang('en');
  store.info = { version: '1.0.0', dataDir: '/data', platform: 'linux', nativeControls: false, jvmBuilt: true };
  store.settings = { language: 'en', run: { ...run }, mods: { jvm: false, javaHome: '' } };
});
afterEach(cleanup);

describe('JvmSettings', () => {
  it('is off by default, shows the red warning, and saves the switch and the Java folder', async () => {
    render(JvmSettings);
    expect(screen.getByRole('alert')).toHaveTextContent('Native code, not sandboxed');
    expect(screen.getByRole('alert')).toHaveTextContent('cannot limit them');
    const sw = screen.getByRole('switch', { name: 'Run Java mods' });
    expect(sw).not.toBeChecked();
    expect(screen.queryByLabelText('Java folder (optional)')).toBeNull();
    expect(screen.queryByText(/no Java support/)).toBeNull();

    await fireEvent.click(sw);
    await waitFor(() => expect(mock.api.setSettings).toHaveBeenCalled());
    expect(store.settings.mods).toEqual({ jvm: true, javaHome: '' });
    const home = await screen.findByLabelText('Java folder (optional)');
    await fireEvent.input(home, { target: { value: '/opt/jdk' } });
    expect(store.settings.mods).toEqual({ jvm: true, javaHome: '/opt/jdk' });
    expect(mock.api.setSettings).toHaveBeenLastCalledWith(expect.objectContaining({ mods: { jvm: true, javaHome: '/opt/jdk' } }));

    await fireEvent.click(sw);
    expect(store.settings.mods?.jvm).toBe(false);
    expect(screen.queryByLabelText('Java folder (optional)')).toBeNull();
  });

  it('says so when the build has no Java support, and copes with settings without mods', async () => {
    store.info = { version: '1.0.0', dataDir: '/data', platform: 'linux', nativeControls: false };
    store.settings = { language: 'en', run: { ...run } };
    render(JvmSettings);
    expect(screen.getByText(/no Java support/)).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: 'Run Java mods' })).not.toBeChecked();
    store.info = null;
    await fireEvent.click(screen.getByRole('switch', { name: 'Run Java mods' }));
    expect(store.settings.mods).toEqual({ jvm: true, javaHome: '' });
  });
});
