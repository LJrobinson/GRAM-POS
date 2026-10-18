import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test('real Rust register: mixed cart, responsive layout, lost response retry, receipt and configuration', async ({ page, request }, testInfo) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const api = async (command: string, payload: unknown = {}) => {
    const response = await request.post('/api/command', { data: { command, payload } });
    const result = await response.json();
    expect(result.ok, JSON.stringify(result)).toBe(true);
    return result.data;
  };
  await page.goto('/');
  await page.getByRole('button', { name: 'Load demo register' }).click();
  await expect(page.getByRole('button', { name: /Demo Bulk A/ })).toBeVisible();
  await page.getByRole('button', { name: 'Manage cash', exact: true }).click();
  await page.getByRole('button', { name: 'Open cash session', exact: true }).click();
  await page.getByLabel('Opening float (USD)', { exact: true }).fill('100');
  await page.getByRole('button', { name: 'Review opening float', exact: true }).click();
  await page.getByRole('button', { name: 'Confirm opening float', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Open drawer session', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Register', exact: true }).click();
  for (const [name, weight] of [['Demo Bulk A', '3.62'], ['Demo Bulk B', '3.58']]) {
    await page.getByRole('button', { name: new RegExp(name) }).click();
    await page.getByLabel('Measured weight', { exact: true }).fill(weight);
    await page.getByRole('button', { name: 'Add to cart' }).click();
  }
  await page.getByRole('button', { name: /Demo 3.5 g Fixed Pack/ }).click();
  await page.getByLabel('Number of packages', { exact: true }).fill('2');
  await page.getByRole('button', { name: 'Add to cart' }).click();
  await expect(page.locator('.total-row')).toContainText('125.00');
  await expect.poll(async () => (await api('get_state')).draft.lines.length).toBe(3);
  const stateBefore = await api('get_state');
  const quote = await api('quote_cart', { lines: stateBefore.draft.lines });
  expect(quote.total_minor).toBe('12500');
  expect(quote.groups[0].actual_measured_grams).toBe('7.200');
  expect(quote.lines[2].quantity).toEqual({ kind: 'prepacked', count: '2' });
  await page.screenshot({ path: testInfo.outputPath('register-desktop.png'), fullPage: true });
  for (const [width, height, name] of [[1024, 768, 'tablet'], [390, 844, 'phone']] as const) {
    await page.setViewportSize({ width, height });
    await expect(page.getByRole('button', { name: /Charge cash/ })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath(`register-${name}.png`), fullPage: true });
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.getByRole('button', { name: /Charge cash/ }).click();
  await page.getByLabel('Cash received (USD)', { exact: true }).fill('100');
  await expect(page.getByRole('button', { name: 'Complete cash sale' })).toBeDisabled();
  await page.getByLabel('Cash received (USD)', { exact: true }).fill('150');
  let dropped = false;
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (!dropped && body.command === 'complete_cash_sale') {
      dropped = true;
      const response = await route.fetch();
      expect((await response.json()).ok).toBe(true);
      await route.abort('failed');
    } else await route.continue();
  });
  await page.getByRole('button', { name: 'Complete cash sale' }).click();
  await expect(page.getByRole('button', { name: 'Retry same checkout' })).toBeEnabled();
  expect((await api('list_sales')).sales).toHaveLength(1);
  await page.getByRole('button', { name: 'Retry same checkout' }).click();
  await expect(page.getByRole('heading', { name: 'Sale saved.' })).toBeVisible();
  await expect(page.locator('.change-row')).toContainText('25.00');
  expect((await api('list_sales')).sales).toHaveLength(1);
  expect((await api('get_state')).draft.lines).toHaveLength(0);
  await page.screenshot({ path: testInfo.outputPath('saved-receipt.png'), fullPage: true });
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await page.reload();
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await expect(page.locator('.sale-row')).toHaveCount(1);
  await page.locator('.sale-row').click();
  await expect(page.locator('.receipt-total')).toContainText('125.00');
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await page.getByRole('button', { name: 'Configuration', exact: true }).click();
  await page.getByLabel('tolerance grams tier 1', { exact: true }).fill('0.16');
  await page.getByRole('button', { name: 'Save configuration', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  expect((await api('get_state')).config.groups[0].tiers[0].tolerance_grams).toBe('0.16');
  await page.getByRole('button', { name: 'Register', exact: true }).click();
  await page.getByRole('button', { name: /Demo Bulk A/ }).click();
  await page.getByLabel('Measured weight', { exact: true }).fill('3.66');
  await page.getByRole('button', { name: 'Add to cart' }).click();
  await expect(page.locator('.total-row')).toContainText('30.00');
  expect(errors).toEqual([]);
});

test('catalog editing, archives, atomic validation, conflict recovery and portable downloads', async ({ page, request }, testInfo) => {
  const api = async (command: string, payload: unknown = {}) => {
    const result = await (await request.post('/api/command', { data: { command, payload } })).json();
    expect(result.ok, JSON.stringify(result)).toBe(true);
    return result.data;
  };
  if (!(await api('get_state')).configured) await api('initialize_demo');
  await api('save_draft', { lines: [] });
  const initial = await api('get_state');
  await page.goto('/');
  await page.getByRole('button', { name: 'Configuration', exact: true }).click();
  const save = page.getByRole('button', { name: 'Save configuration', exact: true });
  await page.getByRole('button', { name: 'Add pricing group', exact: true }).click();
  const groupNumber = initial.config.groups.length + 1;
  await page.getByLabel(`Group ID ${groupNumber}`, { exact: true }).fill('NEW-GROUP');
  await page.getByLabel(`Group name ${groupNumber}`, { exact: true }).fill('Test tier schedule');
  const group = page.locator('.settings-card').filter({ has: page.getByLabel(`Group ID ${groupNumber}`, { exact: true }) });
  await group.getByLabel('nominal grams tier 1', { exact: true }).fill('1.000');
  await group.getByLabel('tolerance grams tier 1', { exact: true }).fill('0.100');
  await group.getByLabel('price minor tier 1', { exact: true }).fill('200');
  await group.getByLabel('Outside tolerance', { exact: true }).selectOption('full_actual_at_tier_rate');
  await group.getByLabel('Money rounding', { exact: true }).selectOption('half_up');
  await group.getByLabel('Below the smallest tier', { exact: true }).selectOption('reject');
  await group.getByRole('button', { name: 'Add tier', exact: true }).click();
  await expect(group.getByLabel('nominal grams tier 2', { exact: true })).toBeVisible();
  await group.getByRole('button', { name: 'Remove tier 2', exact: true }).click();
  await expect(group.getByLabel('nominal grams tier 2', { exact: true })).toHaveCount(0);
  const number = initial.config.catalog.length + 1;
  await page.getByRole('button', { name: 'Add product', exact: true }).click();
  await page.getByLabel(`Product name ${number}`, { exact: true }).fill('Custom measured');
  await page.getByLabel(`SKU ${number}`, { exact: true }).fill('CUSTOM-M');
  await page.getByLabel(`Pricing group ${number}`, { exact: true }).selectOption('NEW-GROUP');
  await save.click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  await page.getByRole('button', { name: 'Add product', exact: true }).click();
  const packNumber = number + 1;
  await page.getByLabel(`Product name ${packNumber}`, { exact: true }).fill('Custom pack');
  await page.getByLabel(`SKU ${packNumber}`, { exact: true }).fill('CUSTOM-P');
  await page.getByLabel(`Product type ${packNumber}`, { exact: true }).selectOption('prepacked');
  await page.getByLabel(`Declared grams per package ${packNumber}`, { exact: true }).fill('3.500');
  await page.getByLabel(`Package price ${packNumber}`, { exact: true }).fill('9007199254740993');
  await save.click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  expect((await api('get_state')).config.catalog[packNumber - 1].pricing.unit_price_minor).toBe('9007199254740993');
  await page.getByLabel(`Package price ${packNumber}`, { exact: true }).fill('600');
  await page.getByLabel(`SKU ${packNumber}`, { exact: true }).fill('CUSTOM-M');
  const revisionBeforeInvalid = (await api('get_state')).config_revision;
  await save.click();
  await expect(page.getByRole('alert')).toBeVisible();
  expect((await api('get_state')).config_revision).toBe(revisionBeforeInvalid);
  await page.getByLabel(`SKU ${packNumber}`, { exact: true }).fill('CUSTOM-P');
  await save.click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  await page.getByRole('button', { name: `Archive product ${number}`, exact: true }).click();
  await save.click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  await page.getByRole('button', { name: 'Register', exact: true }).click();
  await expect(page.getByRole('button', { name: /Custom measured/ })).toHaveCount(0);
  await page.getByRole('button', { name: /Custom pack/ }).click();
  await page.getByRole('button', { name: 'Add to cart', exact: true }).click();
  await expect(page.locator('.total-row')).toContainText('6.00');
  await page.getByRole('button', { name: 'Clear', exact: true }).click();
  await page.getByRole('button', { name: 'Configuration', exact: true }).click();
  await page.getByRole('button', { name: `Unarchive product ${number}`, exact: true }).click();
  await save.click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  await page.getByLabel(`Product name ${number}`, { exact: true }).fill('Local unsaved edit');
  const newer = await api('get_state');
  newer.config.catalog[number - 1].name = 'Other window edit';
  await api('put_config', { config: newer.config, expected_config_revision: newer.config_revision });
  await save.click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByLabel(`Product name ${number}`, { exact: true })).toHaveValue('Local unsaved edit');
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Reload saved configuration', exact: true }).click();
  await expect(page.getByLabel(`Product name ${number}`, { exact: true })).toHaveValue('Other window edit');
  for (const label of ['Download portable backup', 'Export sales JSON']) {
    const download = page.waitForEvent('download');
    await page.getByRole('button', { name: label, exact: true }).click();
    const file = await download;
    expect(file.suggestedFilename()).toMatch(/\.json$/);
    await file.saveAs(testInfo.outputPath(file.suggestedFilename()));
    expect(await file.failure()).toBeNull();
    const data = JSON.parse(await readFile(testInfo.outputPath(file.suggestedFilename()), 'utf8'));
    expect(data.format).toBe(label === 'Download portable backup' ? 'gram-pos-backup' : 'gram-pos-sales');
    expect(data.format_version).toBe(2);
    if (label === 'Download portable backup') expect(JSON.parse(data.settings.config_json).catalog[number - 1].name).toBe('Other window edit');
    else expect(data.receipts).toHaveLength((await api('list_sales')).sales.length);
  }
  await page.screenshot({ path: testInfo.outputPath('catalog-desktop.png'), fullPage: true });
  await page.locator('.export-panel').screenshot({ path: testInfo.outputPath('backup-export-panel.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('catalog-phone.png'), fullPage: true });
});

test('full refund and immutable drawer close recover lost responses across restart', async ({ page, request }, testInfo) => {
  const api = async (command: string, payload: unknown = {}) => {
    const result = await (await request.post('/api/command', { data: { command, payload } })).json();
    expect(result.ok, JSON.stringify(result)).toBe(true);
    return result.data;
  };
  await api('save_draft', { lines: [] });
  const before = await api('get_cash_ledger');
  const sale = (await api('list_sales')).sales[0];
  const original = await api('get_receipt', { sale_id: sale.sale_id });
  await page.goto('/');
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await page.locator('.sale-row').first().click();
  await page.getByRole('button', { name: 'Refund full receipt', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Review full refund', exact: true })).toBeDisabled();
  await page.getByLabel('Refund reason', { exact: true }).fill('Training customer return');
  await page.getByRole('button', { name: 'Review full refund', exact: true }).click();
  await page.screenshot({ path: testInfo.outputPath('refund-confirmation.png'), fullPage: true });
  let dropped = false;
  const refundKeys: string[] = [];
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (body.command === 'refund_cash_sale') {
      refundKeys.push(body.payload.idempotency_key);
      if (!dropped) { dropped = true; const result = await route.fetch(); expect((await result.json()).ok).toBe(true); await route.abort('failed'); return; }
    }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Confirm full cash refund', exact: true }).click({ clickCount: 2 });
  await expect(page.getByRole('button', { name: 'Retry same cash action', exact: true })).toBeEnabled();
  await page.reload();
  await page.getByRole('button', { name: 'Retry same cash action', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Cash sessions.' })).toBeVisible();
  expect(refundKeys).toHaveLength(2); expect(refundKeys[0]).toBe(refundKeys[1]);
  let ledger = await api('get_cash_ledger');
  expect(ledger.refunds).toHaveLength(before.refunds.length + 1);
  expect(await api('get_receipt', { sale_id: sale.sale_id })).toEqual(original);
  const expected = ledger.active_session.expected_cash_minor;
  const counted = (BigInt(expected) - 50n).toString();
  await page.getByRole('button', { name: 'Count and close session', exact: true }).click();
  await page.getByLabel('Counted cash (USD)', { exact: true }).fill(`${BigInt(counted) / 100n}.${(BigInt(counted) % 100n).toString().padStart(2, '0')}`);
  await page.getByRole('button', { name: 'Review closing count', exact: true }).click();
  await page.screenshot({ path: testInfo.outputPath('cash-session-close.png'), fullPage: true });
  let closeDropped = false;
  await page.route('**/api/command', async route => {
    if (!closeDropped && route.request().postDataJSON().command === 'close_cash_session') {
      closeDropped = true; const result = await route.fetch(); expect((await result.json()).ok).toBe(true); await route.abort('failed');
    } else await route.fallback();
  });
  await page.getByRole('button', { name: 'Confirm and close session', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry same cash action', exact: true })).toBeEnabled();
  await page.reload();
  await page.getByRole('button', { name: 'Retry same cash action', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'No open cash session', exact: true })).toBeVisible();
  ledger = await api('get_cash_ledger');
  expect(ledger.active_session).toBeNull();
  const closed = ledger.sessions.find((s: { session_id: string }) => s.session_id === before.active_session.session_id);
  expect(closed.counted_minor).toBe(counted); expect(closed.variance_minor).toBe('-50');
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('cash-session-history-phone.png'), fullPage: true });
  await page.getByRole('button', { name: 'Register', exact: true }).click();
  await page.getByRole('button', { name: /Demo 3.5 g Fixed Pack/ }).click();
  await page.getByRole('button', { name: 'Add to cart', exact: true }).click();
  await expect(page.getByRole('button', { name: /Charge cash/ })).toBeDisabled();
  await page.getByRole('button', { name: 'Cash sessions', exact: true }).click();
  await page.getByRole('button', { name: 'Open cash session', exact: true }).click();
  await page.getByLabel('Opening float (USD)', { exact: true }).fill('25');
  await page.getByRole('button', { name: 'Review opening float', exact: true }).click();
  let openDropped = false;
  const openKeys: string[] = [];
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (body.command === 'open_cash_session') {
      openKeys.push(body.payload.idempotency_key);
      if (!openDropped) { openDropped = true; const result = await route.fetch(); expect((await result.json()).ok).toBe(true); await route.abort('failed'); return; }
    }
    await route.fallback();
  });
  await page.getByRole('button', { name: 'Confirm opening float', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry same cash action', exact: true })).toBeEnabled();
  await page.reload();
  await page.getByRole('button', { name: 'Retry same cash action', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Open drawer session', exact: true })).toBeVisible();
  expect(openKeys).toHaveLength(2); expect(openKeys[0]).toBe(openKeys[1]);
  expect((await api('get_cash_ledger')).sessions).toHaveLength(before.sessions.length + 1);
  await page.screenshot({ path: testInfo.outputPath('cash-session-open-phone.png'), fullPage: true });
});

test('blank onboarding requires explicit currency and opens configuration', async ({ page }) => {
  let configured = false;
  const config = { schema_version: 1, demo: true, tax_mode: 'demo_no_tax', currency: { code: 'CAD', minor_unit_exponent: 2 }, groups: [], catalog: [] };
  await page.route('**/api/command', async route => {
    const { command, payload } = route.request().postDataJSON();
    if (command === 'get_cash_ledger') { await route.fulfill({ json: { ok: true, data: { active_session: null, sessions: [], refunds: [], sale_sessions: [] } } }); return; }
    if (command === 'initialize_empty') {
      expect(payload).toEqual({ currency: { code: 'CAD', minor_unit_exponent: 2 } });
      configured = true;
    }
    await route.fulfill({ json: { ok: true, data: { configured, config_revision: configured ? '1' : '0', config: configured ? config : null, catalog: [], draft: { lines: [] } } } });
  });
  await page.goto('/');
  await page.getByText('Start with an empty catalog instead', { exact: true }).click();
  const create = page.getByRole('button', { name: 'Create empty training register' });
  await expect(create).toBeDisabled();
  await page.getByLabel('Currency code', { exact: true }).fill('CAD');
  await page.getByLabel('Currency decimal places', { exact: true }).selectOption('2');
  await create.click();
  await expect(page.getByRole('heading', { name: 'Configuration.' })).toBeVisible();
  await expect(page.getByText('No products yet.', { exact: false })).toBeVisible();
});
