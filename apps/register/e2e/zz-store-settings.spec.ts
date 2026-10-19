import { test, expect, type APIRequestContext, type Page, type TestInfo } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Actual Rust/SQLite commands. Interception only delays real responses to test races.
const command = async (request: APIRequestContext, name: string, payload: unknown = {}) =>
  (await request.post('/api/command', { data: { command: name, payload } })).json();
const api = async (request: APIRequestContext, name: string, payload: unknown = {}) => {
  const result = await command(request, name, payload);
  expect(result.ok, JSON.stringify(result)).toBe(true);
  return result.data;
};
const branding = { display_name: 'Cedar Market', receipt_header: 'Neighborhood provisions · Training register',
  receipt_contact: '123 Example Lane · example.invalid', receipt_footer: 'Thank you for shopping locally.', timezone: 'America/Los_Angeles' };
let originalConfig: unknown;
test.beforeEach(async ({ request }) => {
  originalConfig = undefined;
  if (!(await api(request, 'get_state')).configured) await api(request, 'initialize_demo');
  await api(request, 'save_draft', { lines: [] });
  const state = await api(request, 'get_state');
  originalConfig = structuredClone(state.config);
  delete state.config.store_settings;
  state.config.catalog.push({ sku: 'SETTINGS-UNIT', name: 'Market tote (training)', kind: 'unit', archived: false,
    declared_content_grams: null, pricing: { kind: 'fixed_unit', unit_price_minor: '425' } });
  await api(request, 'put_config', { config: state.config, expected_config_revision: state.config_revision });
});
test.afterEach(async ({ page, request }) => {
  await page.close();
  await api(request, 'save_draft', { lines: [] });
  if (originalConfig) {
    const state = await api(request, 'get_state');
    await api(request, 'put_config', { config: originalConfig, expected_config_revision: state.config_revision });
  }
});
const settings = (page: Page) => page.getByRole('region', { name: 'Store settings', exact: true });
async function openSettings(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Configuration', exact: true }).click();
  await expect(settings(page)).toBeVisible();
}
async function fillSettings(page: Page) {
  const form = settings(page);
  await form.getByLabel('Store display name', { exact: true }).fill(branding.display_name);
  await form.getByLabel('Receipt header', { exact: true }).fill(branding.receipt_header);
  await form.getByLabel('Receipt contact', { exact: true }).fill(branding.receipt_contact);
  await form.getByLabel('Receipt footer', { exact: true }).fill(branding.receipt_footer);
  await form.getByLabel('Store timezone', { exact: true }).fill(branding.timezone);
}
async function saveSettings(request: APIRequestContext, value = branding) {
  const state = await api(request, 'get_state');
  return api(request, 'put_store_settings', { store_settings: value, expected_config_revision: state.config_revision });
}
async function checkout(request: APIRequestContext, key: string) {
  const state = await api(request, 'get_state');
  let ledger = await api(request, 'get_cash_ledger');
  if (!ledger.active_session) {
    await api(request, 'open_cash_session', { idempotency_key: `${key}-open`, opening_float_minor: '0', expected_currency: state.config.currency });
    ledger = await api(request, 'get_cash_ledger');
  }
  const lines = [{ id: 'settings-unit', sku: 'SETTINGS-UNIT', quantity: { kind: 'unit', count: '2' } }];
  const quote = await api(request, 'quote_cart', { lines });
  return api(request, 'complete_cash_sale', { idempotency_key: key, lines, tender_minor: '1000',
    expected_total_minor: quote.total_minor, expected_config_revision: quote.config_revision,
    expected_session_id: ledger.active_session.session_id, expected_currency: state.config.currency });
}
async function capture(page: Page, info: TestInfo, name: string, focus?: () => Promise<void>) {
  for (const [width, height, size] of [[1440, 1000, 'desktop'], [390, 844, 'phone']] as const) {
    await page.setViewportSize({ width, height });
    if (focus) await focus();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    const path = info.outputPath(`${name}-${size}.png`);
    await page.screenshot({ path });
    await info.attach(`${name} ${size} (training fixture)`, { path, contentType: 'image/png' });
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
}

test('store settings preview and cancel are read-only, save preserves policies and original receipts', async ({ page, request }, info) => {
  const before = await api(request, 'get_state');
  const legacy = await checkout(request, 'settings-legacy-sale');
  await openSettings(page);
  await fillSettings(page);
  await settings(page).getByRole('button', { name: 'Preview receipt', exact: true }).click();
  await expect(settings(page)).toContainText('Cedar Market');
  await settings(page).getByLabel('Receipt footer', { exact: true }).fill('Changed preview');
  await expect(settings(page).getByRole('button', { name: 'Save store settings', exact: true })).toBeDisabled();
  await settings(page).getByLabel('Receipt footer', { exact: true }).fill(branding.receipt_footer);
  await settings(page).getByRole('button', { name: 'Preview receipt', exact: true }).click();
  await expect(settings(page)).toContainText('DEMO / TRAINING');
  expect((await api(request, 'get_state')).config_revision).toBe(before.config_revision);
  await capture(page, info, 'store-settings-preview', () => settings(page).getByLabel('Receipt identity preview', { exact: true }).scrollIntoViewIfNeeded());
  await settings(page).getByRole('button', { name: 'Cancel changes', exact: true }).click();
  await expect(settings(page).getByLabel('Store display name', { exact: true })).toHaveValue('GRAM POS');
  expect((await api(request, 'get_state')).config).toEqual(before.config);
  await fillSettings(page);
  await settings(page).getByRole('button', { name: 'Preview receipt', exact: true }).click();
  await settings(page).getByRole('button', { name: 'Save store settings', exact: true }).click();
  await expect.poll(async () => (await api(request, 'get_state')).config.store_settings).toEqual(branding);
  const saved = await api(request, 'get_state');
  expect(saved.config.currency).toEqual(before.config.currency);
  expect(saved.config.groups).toEqual(before.config.groups);
  expect(saved.config.catalog).toEqual(before.config.catalog);
  const receipt = await checkout(request, 'settings-branded-sale');
  expect(receipt.config_snapshot.store_settings).toEqual(branding);
  await saveSettings(request, { ...branding, display_name: 'Later branding', receipt_footer: 'Later footer', timezone: 'Asia/Tokyo' });
  expect(await api(request, 'get_receipt', { sale_id: legacy.sale_id })).toEqual(legacy);
  expect(await api(request, 'get_receipt', { sale_id: receipt.sale_id })).toEqual(receipt);
  await page.reload();
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await page.locator('.sale-row').filter({ hasText: receipt.receipt_number }).click();
  const printed = page.locator('#print-receipt');
  await expect(printed).toContainText(branding.display_name);
  await expect(printed).toContainText(branding.receipt_footer);
  await expect(printed).not.toContainText('Later branding');
  await expect(printed).toContainText('DEMO / TRAINING RECEIPT');
  await expect(printed.locator('.receipt-total')).toContainText('$8.50');
  const originalHtml = await printed.innerHTML();
  await capture(page, info, 'store-branded-receipt', () => printed.locator('.receipt-brand').scrollIntoViewIfNeeded());
  const ledger = await api(request, 'get_cash_ledger');
  await api(request, 'refund_cash_sale', { idempotency_key: 'settings-branded-refund', sale_id: receipt.sale_id, reason: 'Training return',
    expected_session_id: ledger.active_session.session_id, expected_currency: receipt.quote.currency });
  await page.reload();
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await page.locator('.sale-row').filter({ hasText: receipt.receipt_number }).click();
  expect(await printed.innerHTML()).toBe(originalHtml);
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await page.locator('.sale-row').filter({ hasText: legacy.receipt_number }).click();
  await expect(printed.locator('.receipt-brand')).toHaveText('GRAMDEMO / TRAINING RECEIPT');
  await expect(printed).not.toContainText(branding.display_name);
});

test('settings stale save cannot overwrite a newer revision', async ({ page, request }) => {
  await openSettings(page);
  await fillSettings(page);
  await settings(page).getByRole('button', { name: 'Preview receipt', exact: true }).click();
  const newer = { ...branding, display_name: 'Other window' };
  await saveSettings(request, newer);
  await settings(page).getByRole('button', { name: 'Save store settings', exact: true }).click();
  await expect(settings(page).getByRole('alert')).toBeVisible();
  expect((await api(request, 'get_state')).config.store_settings).toEqual(newer);
});

test('IANA report days resolve DST, unknown zones fail, CSV identifies exact bounds', async ({ page, request }, info) => {
  await saveSettings(request);
  await checkout(request, 'settings-report-sale');
  await page.goto('/');
  await page.getByRole('button', { name: 'Reports', exact: true }).click();
  const reports = page.getByRole('region', { name: 'Sales reports', exact: true });
  await expect(reports.getByLabel('Report timezone', { exact: true })).toHaveValue(branding.timezone);
  for (const [start_date, end_date, hours] of [['2026-03-08', '2026-03-09', 23], ['2026-11-01', '2026-11-02', 25]] as const) {
    const report = await api(request, 'sales_report', { start_date, end_date, timezone: branding.timezone });
    expect(BigInt(report.end_at_unix) - BigInt(report.start_at_unix)).toBe(BigInt(hours * 3600));
  }
  await reports.getByLabel('Start date (inclusive)', { exact: true }).fill('2026-01-01');
  await reports.getByLabel('End date (exclusive)', { exact: true }).fill('2027-01-01');
  await reports.getByRole('button', { name: 'Load report', exact: true }).click();
  await expect(reports.getByTestId('report-currency-USD-2')).toBeVisible();
  await expect(reports).toContainText(branding.timezone);
  await expect(reports.getByTestId('report-event').first()).toContainText(/[-+]\d{2}:\d{2}/);
  await capture(page, info, 'store-timezone-report', () => reports.scrollIntoViewIfNeeded());
  const downloadReady = page.waitForEvent('download');
  await reports.getByRole('button', { name: 'Export report CSV', exact: true }).click();
  const download = await downloadReady;
  const path = info.outputPath('store-timezone-report.csv');
  await download.saveAs(path);
  const csv = await readFile(path, 'utf8');
  expect(csv).toContain('America/Los_Angeles');
  expect(csv).toContain('start_at_unix');
  expect(csv).toContain('end_at_unix');
  await info.attach('Store-local report CSV', { path, contentType: 'text/csv' });
  await reports.getByLabel('Report timezone', { exact: true }).fill('Mars/Olympus');
  await reports.getByRole('button', { name: 'Load report', exact: true }).click();
  await expect(reports.getByRole('alert')).toBeVisible();
  await expect(reports.getByRole('button', { name: 'Export report CSV', exact: true })).toHaveCount(0);
});

test('late report responses cannot restore a changed timezone and export refuses changed settings', async ({ page, request }) => {
  await saveSettings(request);
  await page.goto('/');
  await page.getByRole('button', { name: 'Reports', exact: true }).click();
  const reports = page.getByRole('region', { name: 'Sales reports', exact: true });
  await reports.getByLabel('Start date (inclusive)', { exact: true }).fill('2026-03-08');
  await reports.getByLabel('End date (exclusive)', { exact: true }).fill('2026-03-09');
  let release!: () => void;
  let fetched!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  const started = new Promise<void>(resolve => { fetched = resolve; });
  let delayed = false;
  await page.route('**/api/command', async route => {
    const payload = route.request().postDataJSON();
    if (payload.command === 'sales_report' && !delayed) {
      delayed = true;
      const response = await route.fetch();
      fetched();
      await held;
      await route.fulfill({ response });
    } else await route.continue();
  });
  await reports.getByRole('button', { name: 'Load report', exact: true }).click();
  await started;
  await reports.getByLabel('Report timezone', { exact: true }).fill('UTC');
  release();
  await expect(reports.getByRole('button', { name: 'Export report CSV', exact: true })).toHaveCount(0);
  await reports.getByRole('button', { name: 'Load report', exact: true }).click();
  await expect(reports.getByRole('button', { name: 'Export report CSV', exact: true })).toBeEnabled();
  await expect(reports).toContainText('UTC');
  await saveSettings(request, { ...branding, timezone: 'Europe/London' });
  const downloads: string[] = [];
  page.on('download', download => downloads.push(download.suggestedFilename()));
  await reports.getByRole('button', { name: 'Export report CSV', exact: true }).click();
  await expect(reports.getByRole('alert')).toBeVisible();
  expect(downloads).toEqual([]);
});

test('confirmed reload resets legacy unsaved settings and preview; cancelled reload preserves them', async ({ page, request }) => {
  const before = await api(request, 'get_state');
  expect(before.config.store_settings).toBeUndefined();
  await openSettings(page);
  await fillSettings(page);
  await settings(page).getByRole('button', { name: 'Preview receipt', exact: true }).click();
  page.once('dialog', dialog => dialog.dismiss());
  await page.getByRole('button', { name: 'Reload saved configuration', exact: true }).click();
  await expect(settings(page).getByLabel('Store display name', { exact: true })).toHaveValue(branding.display_name);
  await expect(settings(page).getByRole('button', { name: 'Save store settings', exact: true })).toBeEnabled();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Reload saved configuration', exact: true }).click();
  await expect(settings(page).getByLabel('Store display name', { exact: true })).toHaveValue('GRAM POS');
  await expect(settings(page).getByRole('button', { name: 'Save store settings', exact: true })).toBeDisabled();
  await expect(settings(page).getByLabel('Receipt identity preview', { exact: true })).toHaveCount(0);
  expect((await api(request, 'get_state')).config_revision).toBe(before.config_revision);
});
