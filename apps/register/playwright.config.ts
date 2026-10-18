import { defineConfig } from '@playwright/test';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
// Every run gets a new test-only database; never reset an existing register.
const database = join(mkdtempSync(join(tmpdir(), 'gram-e2e-')), 'register.sqlite3');
export default defineConfig({
  testDir: './e2e', workers: 1, fullyParallel: false, retries: 0,
  timeout: 90000, expect: { timeout: 15000 },
  reporter: [['list'], ['html', { open: 'never' }]],
  use: { launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}, baseURL: 'http://127.0.0.1:5173', viewport: { width: 1440, height: 1000 }, trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  webServer: [
    { command: `cargo run -p gram-register-service --features dev-server --bin register-dev-server -- --db "${database}" --port 4174`, cwd: '../..', url: 'http://127.0.0.1:4174/health', timeout: 180000, reuseExistingServer: false },
    { command: 'npm run dev', url: 'http://127.0.0.1:5173', timeout: 60000, reuseExistingServer: false },
  ],
});
