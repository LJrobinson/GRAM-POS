import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { fixtureCommand, reloadRegister } from './fixture-command';

test('real Rust register: mixed cart, responsive layout, lost response retry, receipt and configuration', async ({ page, request }, testInfo) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const api = async (command: string, payload: unknown = {}) => {
    const result = await fixtureCommand(request, command, payload);
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
  await reloadRegister(page);
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
    const result = await fixtureCommand(request, command, payload);
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
  page.once('dialog', dialog => dialog.accept());
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
    expect(data.format_version).toBe(label === 'Download portable backup' ? 4 : 3);
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
    const result = await fixtureCommand(request, command, payload);
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
  await reloadRegister(page);
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
  await reloadRegister(page);
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
  await reloadRegister(page);
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
    await route.fulfill({ json: { ok: true, data: { configured, config_revision: configured ? '1' : '0', config: configured ? config : null, catalog: [], draft: { lines: [] }, draft_revision: '0', active_draft: null, parked_drafts: [] } } });
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

// Keep this flow after the original fresh-database flow above. It also runs alone:
// its own catalog, session, receipts and refund are created through the real service.
test.describe('Printable receipts and sales reports', () => {
  test.use({ timezoneId: 'Pacific/Honolulu' });

  test('immutable receipt, cancel-safe printing, UTC reports and spreadsheet-safe CSV', async ({ page, request }, testInfo) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    const api = async (command: string, payload: unknown = {}) => {
      const result = await fixtureCommand(request, command, payload);
      expect(result.ok, JSON.stringify(result)).toBe(true);
      return result.data;
    };
    if (!(await api('get_state')).configured) await api('initialize_demo');
    await api('save_draft', { lines: [] });
    const initial = await api('get_state');
    expect(initial.config.currency).toEqual({ code: 'USD', minor_unit_exponent: 2 });
    initial.config.groups.push({
      id: 'report-test-group', name: 'Original snapshot schedule',
      tiers: [{ nominal_grams: '3.500', tolerance_grams: '0.150', price_minor: '3125' }],
      overage_policy: 'full_actual_at_tier_rate', rounding: 'half_up',
      below_smallest: 'proportional_at_smallest_tier', tolerance_scope: 'aggregate_pricing_group',
    });
    initial.config.catalog.push(
      { sku: 'REPORT-MEASURED', name: 'Original bulk "Summer"', kind: 'measured', declared_content_grams: null, archived: false, pricing: { kind: 'weight_group', group_id: 'report-test-group' } },
      { sku: 'REPORT-PACK', name: 'Original 3.5 g pack', kind: 'prepacked', declared_content_grams: '3.500', archived: false, pricing: { kind: 'fixed_unit', unit_price_minor: '725' } },
    );
    await api('put_config', { config: initial.config, expected_config_revision: initial.config_revision });
    let ledger = await api('get_cash_ledger');
    if (!ledger.active_session) {
      await api('open_cash_session', { idempotency_key: 'report-test-open', opening_float_minor: '2500', expected_currency: initial.config.currency });
      ledger = await api('get_cash_ledger');
    }
    const checkout = async (id: string, lines: unknown[], tender: string) => {
      const quote = await api('quote_cart', { lines });
      return api('complete_cash_sale', {
        idempotency_key: id, lines, tender_minor: tender,
        expected_total_minor: quote.total_minor, expected_config_revision: quote.config_revision,
        expected_session_id: ledger.active_session.session_id, expected_currency: initial.config.currency,
      });
    };
    const original = await checkout('report-test-mixed-sale', [
      { id: 'report-measured-line', sku: 'REPORT-MEASURED', quantity: { kind: 'measured', grams: '3.620' } },
      { id: 'report-pack-line', sku: 'REPORT-PACK', quantity: { kind: 'prepacked', count: '2' } },
    ], '5000');
    expect(original.quote.total_minor).toBe('4575');
    expect(original.change_minor).toBe('425');
    const other = await checkout('report-test-other-sale', [
      { id: 'report-other-line', sku: 'REPORT-PACK', quantity: { kind: 'prepacked', count: '1' } },
    ], '1000');
    expect(other.quote.total_minor).toBe('725');

    await page.goto('/');
    await page.getByRole('button', { name: 'Sales history', exact: true }).click();
    await page.locator('.sale-row').filter({ hasText: original.receipt_number }).click();
    const receipt = page.locator('#print-receipt');
    await expect(receipt).toHaveAttribute('data-sale-id', original.sale_id);
    await expect(receipt).toContainText('Original bulk "Summer"');
    await expect(receipt).toContainText('3.620 g measured');
    await expect(receipt).toContainText('2 × 3.500 g declared');
    await expect(receipt).toContainText('$31.25');
    await expect(receipt).toContainText('$14.50');
    await expect(receipt.locator('.receipt-total')).toContainText('$45.75');
    await expect(receipt.locator('.change-row')).toContainText('$4.25');
    await expect(receipt).toContainText('UTC');
    const originalHtml = await receipt.innerHTML();
    await page.getByRole('button', { name: 'Done', exact: true }).click();

    // Historical values must remain the saved snapshot, not today's catalog or price.
    const changed = await api('get_state');
    for (const item of changed.config.catalog) {
      if (item.sku === 'REPORT-MEASURED' || item.sku === 'REPORT-PACK') {
        item.name = `Renamed ${item.sku}`;
        item.archived = true;
        if (item.kind === 'prepacked') {
          item.declared_content_grams = '99.000';
          item.pricing.unit_price_minor = '9999';
        }
      }
    }
    const changedGroup = changed.config.groups.find((group: { id: string }) => group.id === 'report-test-group');
    changedGroup.name = 'Renamed current schedule';
    changedGroup.tiers[0].price_minor = '9900';
    await api('put_config', { config: changed.config, expected_config_revision: changed.config_revision });
    const refundReason = '=1+1, "training return", café';
    const refund = await api('refund_cash_sale', {
      idempotency_key: 'report-test-refund', sale_id: original.sale_id, reason: refundReason,
      expected_session_id: ledger.active_session.session_id, expected_currency: initial.config.currency,
    });
    expect(refund.amount_minor).toBe('4575');
    await reloadRegister(page);
    await page.getByRole('button', { name: 'Sales history', exact: true }).click();
    await page.locator('.sale-row').filter({ hasText: original.receipt_number }).click();
    await expect(page.locator('.refund-status')).toContainText('Fully refunded');
    expect(await receipt.innerHTML()).toBe(originalHtml);
    expect(await api('get_receipt', { sale_id: original.sale_id })).toEqual(original);
    await expect(receipt).not.toContainText('Renamed');
    await expect(receipt).not.toContainText('99.000');
    await page.screenshot({ path: testInfo.outputPath('receipt-preview-desktop.png'), fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath('receipt-preview-phone.png'), fullPage: true });
    await page.setViewportSize({ width: 1440, height: 1000 });

    const ledgerBeforePrint = await api('get_cash_ledger');
    const salesBeforePrint = await api('list_sales');
    const draftBeforePrint = (await api('get_state')).draft;
    await page.evaluate(() => {
      const state = window as Window & { receiptPrintCalls?: number };
      state.receiptPrintCalls = 0;
      // Returning without printing models a cancelled OS print dialog. No API is mocked.
      window.print = () => { state.receiptPrintCalls = (state.receiptPrintCalls ?? 0) + 1; };
    });
    await page.getByRole('button', { name: 'Print receipt', exact: true }).click();
    expect(await page.evaluate(() => (window as Window & { receiptPrintCalls?: number }).receiptPrintCalls)).toBe(1);
    await expect(receipt).toBeVisible();
    expect(await receipt.innerHTML()).toBe(originalHtml);
    expect(await api('get_cash_ledger')).toEqual(ledgerBeforePrint);
    expect(await api('list_sales')).toEqual(salesBeforePrint);
    expect((await api('get_state')).draft).toEqual(draftBeforePrint);
    expect(await api('get_receipt', { sale_id: original.sale_id })).toEqual(original);

    // Exercise the actual print stylesheet and Chromium PDF renderer, not just the button.
    await page.emulateMedia({ media: 'print' });
    await expect(receipt).toBeVisible();
    await expect(page.locator('.sale-row').first()).toBeHidden();
    await expect(page.locator('.receipt-actions')).toBeHidden();
    await expect(page.locator('.refund-status')).toBeHidden();
    const normalize = (value: string) => value.replace(/\s+/g, ' ').trim();
    const printedText = normalize(await page.locator('body').innerText());
    expect(printedText).toBe(normalize(await receipt.innerText()));
    expect(printedText).toContain(original.receipt_number);
    expect(printedText).not.toContain(other.receipt_number);
    expect(printedText).not.toContain('Fully refunded');
    expect(printedText).not.toContain('Sales history');
    const pdfPath = testInfo.outputPath('receipt-print.pdf');
    const pdf = await page.pdf({ path: pdfPath, format: 'A4', printBackground: true, preferCSSPageSize: true });
    expect(pdf.subarray(0, 5).toString()).toBe('%PDF-');
    expect(pdf.byteLength).toBeGreaterThan(1000);
    await testInfo.attach('Selected receipt print PDF', { path: pdfPath, contentType: 'application/pdf' });
    await page.screenshot({ path: testInfo.outputPath('receipt-print.png'), fullPage: true });
    await page.emulateMedia({ media: 'screen' });
    await page.getByRole('button', { name: 'Done', exact: true }).click();
    await page.getByRole('button', { name: 'Reports', exact: true }).click();

    const reports = page.getByRole('region', { name: 'Sales reports', exact: true });
    // Deliberately run the browser outside UTC. Report boundaries are still UTC.
    expect(await page.evaluate(() => Intl.DateTimeFormat().resolvedOptions().timeZone)).toBe('Pacific/Honolulu');
    const start = new Date(Number(original.created_at_unix) * 1000).toISOString().slice(0, 10);
    const end = new Date(Number(refund.created_at_unix) * 1000 + 86_400_000).toISOString().slice(0, 10);
    await reports.getByLabel('Start date (inclusive)', { exact: true }).fill(start);
    await reports.getByLabel('End date (exclusive)', { exact: true }).fill(end);
    await reports.getByRole('button', { name: 'Load report', exact: true }).click();
    const summary = reports.getByTestId('report-currency-USD-2');
    await expect(summary).toBeVisible();
    await expect(reports).toContainText('UTC');
    await expect(reports).toContainText(start);
    await expect(reports).toContainText(end);
    const startUnix = BigInt(Date.parse(`${start}T00:00:00Z`) / 1000);
    const endUnix = BigInt(Date.parse(`${end}T00:00:00Z`) / 1000);
    const inRange = (unix: string) => BigInt(unix) >= startUnix && BigInt(unix) < endUnix;
    const savedSales = (await api('list_sales')).sales.filter((sale: { created_at_unix: string }) => inRange(sale.created_at_unix));
    const savedRefunds = (await api('get_cash_ledger')).refunds.filter((item: { created_at_unix: string }) => inRange(item.created_at_unix));
    const gross = savedSales.reduce((total: bigint, sale: { total_minor: string }) => total + BigInt(sale.total_minor), 0n) as bigint;
    const refunds = savedRefunds.reduce((total: bigint, item: { amount_minor: string }) => total + BigInt(item.amount_minor), 0n) as bigint;
    const dollars = (minor: bigint) => `$${(minor / 100n).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',')}.${(minor % 100n).toString().padStart(2, '0')}`;
    await expect(summary.getByTestId('gross-sales')).toHaveText(dollars(gross));
    await expect(summary.getByTestId('refunds')).toHaveText(dollars(refunds));
    await expect(summary.getByTestId('net-sales')).toHaveText(dollars(gross - refunds));
    await expect(reports.getByTestId('report-event')).toHaveCount(savedSales.length + savedRefunds.length);
    await expect(reports.locator(`[data-event-kind="refund"][data-event-id="${refund.refund_id}"]`)).toContainText('training return');
    await page.screenshot({ path: testInfo.outputPath('sales-reports-desktop.png'), fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    await expect(reports.getByRole('button', { name: 'Export report CSV', exact: true })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath('sales-reports-phone.png'), fullPage: true });
    await page.setViewportSize({ width: 1440, height: 1000 });

    await page.evaluate(() => {
      const state = window as Window & { reportDownloadTypes?: string[] };
      state.reportDownloadTypes = [];
      const createObjectURL = URL.createObjectURL.bind(URL);
      URL.createObjectURL = (object: Blob | MediaSource) => {
        if (object instanceof Blob) state.reportDownloadTypes!.push(object.type);
        return createObjectURL(object);
      };
    });
    const exportResponse = page.waitForResponse(response => response.url().endsWith('/api/command') && response.request().postDataJSON()?.command === 'export_sales_report_csv');
    const downloadReady = page.waitForEvent('download');
    await reports.getByRole('button', { name: 'Export report CSV', exact: true }).click();
    const download = await downloadReady;
    expect(download.suggestedFilename()).toBe('gram-pos-sales-report.csv');
    const csvPath = testInfo.outputPath('gram-pos-sales-report.csv');
    await download.saveAs(csvPath);
    expect(await download.failure()).toBeNull();
    const csv = await readFile(csvPath, 'utf8');
    const exported = await (await exportResponse).json();
    expect(exported.ok).toBe(true);
    expect(exported.data.media_type).toBe('text/csv;charset=utf-8');
    expect(csv).toBe(exported.data.content);
    expect(await page.evaluate(() => (window as Window & { reportDownloadTypes?: string[] }).reportDownloadTypes)).toEqual(['text/csv;charset=utf-8']);
    expect(csv).toContain(`"'${refundReason.replaceAll('"', '""')}"`);
    const records = parseReportCsv(csv);
    const expectedHeader = ['record_type', 'start_date', 'end_date', 'timezone', 'start_at_unix', 'end_at_unix', 'config_revision', 'created_at_local', 'utc_offset_seconds', 'currency_code', 'minor_unit_exponent', 'gross_sales_minor', 'refunds_minor', 'net_sales_minor', 'sale_count', 'refund_count', 'event_kind', 'event_id', 'sale_id', 'created_at_unix', 'total_minor', 'session_id', 'reason'];
    expect(records[0]).toEqual(expectedHeader);
    expect(records.every(row => row.length === expectedHeader.length)).toBe(true);
    const rows = records.slice(1).map(row => Object.fromEntries(expectedHeader.map((name, index) => [name, row[index]])));
    expect(rows).toHaveLength(1 + savedSales.length + savedRefunds.length);
    expect(rows[0]).toMatchObject({ record_type: 'summary', start_date: start, end_date: end, timezone: 'UTC', currency_code: 'USD', minor_unit_exponent: '2', gross_sales_minor: String(gross), refunds_minor: String(refunds), net_sales_minor: String(gross - refunds), sale_count: String(savedSales.length), refund_count: String(savedRefunds.length) });
    expect(rows.find(row => row.event_kind === 'refund' && row.event_id === refund.refund_id)).toMatchObject({ sale_id: original.sale_id, total_minor: '4575', reason: `'${refundReason}` });
    expect(rows.find(row => row.event_kind === 'sale' && row.event_id === original.sale_id)).toMatchObject({ sale_id: original.sale_id, total_minor: '4575', reason: '' });
    await testInfo.attach('Sales report CSV', { path: csvPath, contentType: 'text/csv' });

    await reports.getByLabel('Start date (inclusive)', { exact: true }).fill('2000-01-01');
    await reports.getByLabel('End date (exclusive)', { exact: true }).fill('2000-01-02');
    await reports.getByRole('button', { name: 'Load report', exact: true }).click();
    await expect(reports.getByRole('heading', { name: 'No activity in this range', exact: true })).toBeVisible();
    await expect(reports.getByTestId('report-event')).toHaveCount(0);
    await expect(reports.getByTestId('report-currency-USD-2')).toHaveCount(0);
    await page.screenshot({ path: testInfo.outputPath('sales-reports-empty.png'), fullPage: true });
    expect(await api('get_cash_ledger')).toEqual(ledgerBeforePrint);
    expect(await api('list_sales')).toEqual(salesBeforePrint);
    expect(await api('get_receipt', { sale_id: original.sale_id })).toEqual(original);
    expect(errors).toEqual([]);

    // A real value beyond Number.MAX_SAFE_INTEGER must remain exact and fit
    // narrow receipt paper, including its total and tender rows.
    const largeState = await api('get_state');
    largeState.config.catalog.push({ sku: 'REPORT-LARGE', name: 'Large exact-value training pack', kind: 'prepacked', declared_content_grams: '1.000', archived: false, pricing: { kind: 'fixed_unit', unit_price_minor: '9007199254740993' } });
    await api('put_config', { config: largeState.config, expected_config_revision: largeState.config_revision });
    const large = await checkout('report-test-large-sale', [{ id: 'report-large-line', sku: 'REPORT-LARGE', quantity: { kind: 'prepacked', count: '1' } }], '9007199254740993');
    expect(large.quote.total_minor).toBe('9007199254740993');
    await reloadRegister(page);
    await page.getByRole('button', { name: 'Sales history', exact: true }).click();
    await page.locator('.sale-row').filter({ hasText: large.receipt_number }).click();
    await expect(receipt.locator('.receipt-total')).toContainText('$90,071,992,547,409.93');
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await receipt.evaluate(root => root.scrollWidth <= root.clientWidth + 1)).toBe(true);
    await page.emulateMedia({ media: 'print' });
    const printLayout = await receipt.evaluate(root => {
      const boundary = root.getBoundingClientRect();
      const amounts = [...root.querySelectorAll<HTMLElement>('.receipt-lines > div > strong, .receipt-totals > div > :last-child')];
      return { width: boundary.width, overflow: amounts.filter(amount => amount.scrollWidth > amount.clientWidth + 1 || amount.getBoundingClientRect().right > boundary.right + 1).map(amount => amount.textContent) };
    });
    expect(printLayout.width).toBeLessThanOrEqual(273); // 72 mm at Chromium's 96 CSS px/in.
    expect(printLayout.overflow).toEqual([]);
    await page.screenshot({ path: testInfo.outputPath('receipt-print-large.png'), fullPage: true });
    await page.emulateMedia({ media: 'screen' });
    await page.setViewportSize({ width: 1440, height: 1000 });
    expect(await api('get_receipt', { sale_id: large.sale_id })).toEqual(large);
    expect(errors).toEqual([]);
  });
});

/** Parse RFC 4180 fields, including doubled quotes and embedded CR/LF in reasons. */
function parseReportCsv(csv: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [], field = '', quoted = false;
  for (let index = 0; index < csv.length; index += 1) {
    const char = csv[index];
    if (quoted) {
      if (char !== '"') field += char;
      else if (csv[index + 1] === '"') { field += '"'; index += 1; }
      else quoted = false;
    } else if (char === '"') quoted = true;
    else if (char === ',') { row.push(field); field = ''; }
    else if (char === '\r' && csv[index + 1] === '\n') {
      row.push(field); rows.push(row); row = []; field = ''; index += 1;
    } else throw new Error(`Unexpected unquoted CSV character at ${index}`);
  }
  if (quoted || field || row.length) throw new Error('Incomplete RFC 4180 record');
  return rows;
}
