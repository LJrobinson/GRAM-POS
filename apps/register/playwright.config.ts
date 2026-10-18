import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './e2e', workers: 1, fullyParallel: false, retries: 0,
  timeout: 90000, expect: { timeout: 15000 },
  reporter: [['list'], ['html', { open: 'never' }]],
  use: { baseURL: 'http://127.0.0.1:5173', viewport: { width: 1440, height: 1000 }, trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  webServer: [
    { command: 'cargo run -p gram-register-service --features dev-server --bin register-dev-server -- --db /tmp/gram-e2e.sqlite3 --port 4174', cwd: '../..', url: 'http://127.0.0.1:4174/health', timeout: 180000, reuseExistingServer: false },
    { command: 'npm run dev', url: 'http://127.0.0.1:5173', timeout: 60000, reuseExistingServer: false },
  ],
});
