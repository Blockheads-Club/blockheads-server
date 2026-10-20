import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  timeout: 30_000,
  use: { baseURL: 'http://localhost:4173', viewport: { width: 1000, height: 700 } },
  webServer: { command: 'bun run build && bun run preview --port 4173 --strictPort', url: 'http://localhost:4173', reuseExistingServer: !process.env.CI, timeout: 120_000 },
  projects: [{ name: 'desktop' }, { name: 'phone', use: { viewport: { width: 390, height: 780 } } }]
});
