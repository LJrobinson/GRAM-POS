import { test, expect, type APIRequestContext, type Page, type TestInfo, type Locator } from '@playwright/test';

// All pricing, persistence, lookups and refunds go to the real Rust service.
// Routes below only delay or lose a real response to exercise UI race recovery.
const command = async (request: APIRequestContext, name: string, payload: unknown = {}) =>
  (await request.post('/api/command', { data: { command: name, payload } })).json();
const api = async (request: APIRequestContext, name: string, payload: unknown = {}) => {
  const result = await command(request, name, payload);
  expect(result.ok, JSON.stringify(result)).toBe(true);
  return result.data;
};
const unit = (sku: string, barcode: string, archived = false) => ({
  sku, barcode, name: `Scanner ${sku}`, kind: 'unit', declared_content_grams: null,
  archived, pricing: { kind: 'fixed_unit', unit_price_minor: '425' },
});
async function capture(page: Page, info: TestInfo, name: string, options: { receipt?: boolean; editor?: Locator } = {}) {
  for (const [width, height, size] of [[1440, 1000, 'desktop'], [390, 844, 'phone']] as const) {
    await page.setViewportSize({ width, height });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    if (options.receipt) {
      const receipt = page.locator('#print-receipt');
      await expect(receipt).toBeVisible();
      await expect(receipt.locator('.receipt-lines')).toContainText('3 units');
      // The modal scrolls independently of the tall catalog behind it. Keep
      // the actual sale lines and total in view, then capture the viewport:
      // full-page resizing can move fixed overlays outside the screenshot.
      await receipt.locator('.receipt-total').scrollIntoViewIfNeeded();
      await expect(receipt.locator('.receipt-lines')).toBeInViewport({ ratio: 1 });
      await expect(receipt.locator('.receipt-total')).toBeInViewport({ ratio: 1 });
      const visible = await receipt.evaluate(element => {
        const rect = element.getBoundingClientRect();
        const modal = element.closest('.modal')!.getBoundingClientRect();
        return {
          width: Math.max(0, Math.min(rect.right, modal.right, innerWidth) - Math.max(rect.left, modal.left, 0)),
          height: Math.max(0, Math.min(rect.bottom, modal.bottom, innerHeight) - Math.max(rect.top, modal.top, 0)),
          overflow: element.scrollWidth > element.clientWidth + 1,
        };
      });
      expect(visible.width).toBeGreaterThan(200);
      expect(visible.height).toBeGreaterThan(200);
      expect(visible.overflow).toBe(false);
    }
    const path = info.outputPath(`${name}-${size}.png`);
    await page.screenshot({ path, fullPage: !options.receipt });
    await info.attach(`${name} ${size} (training fixtures)`, { path, contentType: 'image/png' });
    if (options.editor) {
      await options.editor.scrollIntoViewIfNeeded();
      await expect(options.editor).toBeVisible();
      const editorPath = info.outputPath(`unit-product-editor-${size}.png`);
      await options.editor.screenshot({ path: editorPath });
      await info.attach(`Unit product editor ${size} (training fixture)`, { path: editorPath, contentType: 'image/png' });
    }
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
}

// Existing specs share the one test-only database. Restore catalog configuration
// and clear our draft even on failure; do not depend on earlier test fixtures.
let originalConfig: unknown;
test.beforeEach(async ({ request }) => {
  originalConfig = undefined;
  if (!(await api(request, 'get_state')).configured) await api(request, 'initialize_demo');
  await api(request, 'save_draft', { lines: [] });
  const state = await api(request, 'get_state');
  originalConfig = structuredClone(state.config);
  state.config.catalog.push(unit('SCAN-UNIT', '001234567890'), unit('SCAN-OTHER', '1234567890'), unit('SCAN-ARCHIVED', '009999999999', true));
  const measured = state.config.catalog.find((item: { kind: string; archived?: boolean }) => item.kind === 'measured' && !item.archived);
  expect(measured).toBeTruthy();
  measured.barcode = '000MEASURED';
  await api(request, 'put_config', { config: state.config, expected_config_revision: state.config_revision });
});
test.afterEach(async ({ page, request }) => {
  await page.close(); // Stop draft effects before restoring shared state.
  await api(request, 'save_draft', { lines: [] });
  if (originalConfig) {
    const state = await api(request, 'get_state');
    await api(request, 'put_config', { config: originalConfig, expected_config_revision: state.config_revision });
  }
});

test('ordinary unit catalog and exact barcode identity reject duplicates atomically', async ({ page, request }, info) => {
  const lookup = await api(request, 'lookup_barcode', { barcode: '001234567890' });
  expect(lookup.item.sku).toBe('SCAN-UNIT');
  expect((await api(request, 'lookup_barcode', { barcode: '1234567890' })).item.sku).toBe('SCAN-OTHER');
  const before = await api(request, 'get_state');
  for (const barcode of ['001234567890', '009999999999']) {
    const invalid = structuredClone(before.config);
    invalid.catalog.push(unit('DUPLICATE-CANDIDATE', barcode));
    const rejected = await command(request, 'put_config', { config: invalid, expected_config_revision: before.config_revision });
    expect(rejected.ok).toBe(false);
    const after = await api(request, 'get_state');
    expect(after.config_revision).toBe(before.config_revision);
    expect(after.config).toEqual(before.config);
  }
  await page.goto('/');
  await page.getByRole('button', { name: 'Configuration', exact: true }).click();
  const index = before.config.catalog.findIndex((item: { sku: string }) => item.sku === 'SCAN-UNIT') + 1;
  await expect(page.getByLabel(`Product type ${index}`, { exact: true })).toHaveValue('unit');
  await expect(page.getByLabel(`Barcode ${index}`, { exact: true })).toHaveValue('001234567890');
  await expect(page.getByLabel(`Declared grams per package ${index}`, { exact: true })).toHaveCount(0);
  await page.getByLabel(`Unit price ${index}`, { exact: true }).fill('450');
  await page.getByRole('button', { name: 'Save configuration', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  const saved = (await api(request, 'get_state')).config.catalog[index - 1];
  expect(saved).toMatchObject({ kind: 'unit', barcode: '001234567890', declared_content_grams: null, pricing: { kind: 'fixed_unit', unit_price_minor: '450' } });
  await page.getByRole('button', { name: 'Add product', exact: true }).click();
  const addedIndex = before.config.catalog.length + 1;
  await page.getByLabel(`Product name ${addedIndex}`, { exact: true }).fill('Ordinary notebook');
  await page.getByLabel(`SKU ${addedIndex}`, { exact: true }).fill('SCAN-UI-UNIT');
  await page.getByLabel(`Product type ${addedIndex}`, { exact: true }).selectOption('unit');
  await page.getByLabel(`Barcode ${addedIndex}`, { exact: true }).fill('000UIUNIT');
  await page.getByLabel(`Unit price ${addedIndex}`, { exact: true }).fill('250');
  await expect(page.getByLabel(`Declared grams per package ${addedIndex}`, { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Save configuration', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Configuration saved and validated' })).toBeVisible();
  expect((await api(request, 'lookup_barcode', { barcode: '000UIUNIT' })).item).toMatchObject({
    sku: 'SCAN-UI-UNIT', name: 'Ordinary notebook', kind: 'unit', declared_content_grams: null,
    pricing: { kind: 'fixed_unit', unit_price_minor: '250' },
  });
  await capture(page, info, 'unit-catalog', { editor: page.locator('.product-editor').filter({ has: page.getByLabel(`SKU ${index}`, { exact: true }) }) });
});

async function delayNextLookup(page: Page) {
  let release!: () => void;
  let started!: () => void;
  let completed!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const received = new Promise<void>(resolve => { started = resolve; });
  const fulfilled = new Promise<void>(resolve => { completed = resolve; });
  let delayed = false;
  await page.route('**/api/command', async route => {
    if (!delayed && route.request().postDataJSON().command === 'lookup_barcode') {
      delayed = true;
      const response = await route.fetch();
      started();
      await gate;
      await route.fulfill({ response });
      completed();
    } else await route.fallback();
  });
  return { received, release, fulfilled };
}

test('focused keyboard scanner preserves two scans, reports errors and prompts measured weight', async ({ page, request }, info) => {
  let lookups = 0;
  page.on('request', request => {
    if (request.url().endsWith('/api/command') && request.postDataJSON()?.command === 'lookup_barcode') lookups += 1;
  });
  await page.goto('/');
  const barcode = page.getByLabel('Barcode', { exact: true });
  // A scanner is an ordinary keyboard outside the deliberately focused field.
  const search = page.getByLabel('Search products', { exact: true });
  await search.fill('001234567890');
  await search.press('Enter');
  await expect(page.locator('.cart-line')).toHaveCount(0);
  expect((await api(request, 'get_state')).draft.lines).toHaveLength(0);
  expect(lookups).toBe(0);
  await search.fill('');
  await barcode.fill('NOT-A-BARCODE');
  await barcode.press('Enter');
  await expect(page.getByRole('alert')).toContainText('No product has this exact barcode');
  await barcode.fill('009999999999');
  await barcode.press('Enter');
  await expect(page.getByRole('alert')).toContainText('archived product');
  expect((await api(request, 'get_state')).draft.lines).toHaveLength(0);

  // Lose one real lookup response, then deliberately retry. No cart item may be
  // added before a successful lookup, and retry is not an extra sale or line.
  let dropped = false;
  await page.route('**/api/command', async route => {
    if (!dropped && route.request().postDataJSON().command === 'lookup_barcode') {
      dropped = true;
      await route.fetch();
      await route.abort('failed');
    } else await route.fallback();
  });
  await barcode.fill('001234567890');
  await barcode.press('Enter');
  await expect(page.getByRole('alert')).toContainText(/fetch|network|connection/i);
  expect((await api(request, 'get_state')).draft.lines).toHaveLength(0);
  await page.unrouteAll({ behavior: 'wait' });

  const pending = await delayNextLookup(page);
  await barcode.fill('001234567890');
  await barcode.press('Enter');
  await pending.received;
  await expect(barcode).toBeEnabled();
  await barcode.fill('001234567890');
  await barcode.press('Enter');
  // OS repeat events are not new deliberate scans.
  await barcode.dispatchEvent('keydown', { key: 'Enter', code: 'Enter', repeat: true });
  pending.release();
  await pending.fulfilled;
  await expect(page.locator('.cart-line')).toHaveCount(2);
  await expect.poll(async () => (await api(request, 'get_state')).draft.lines.length).toBe(2);
  const lines = (await api(request, 'get_state')).draft.lines;
  expect(new Set(lines.map((line: { id: string }) => line.id)).size).toBe(2);
  for (const line of lines) expect(line).toMatchObject({ sku: 'SCAN-UNIT', quantity: { kind: 'unit', count: '1' } });
  await expect(page.locator('.total-row')).toContainText('8.50');
  await page.getByRole('button', { name: 'Units / each', exact: true }).click();
  await capture(page, info, 'unit-scanner');
  await info.attach('Screenshot training fixture context', {
    body: 'These screenshots show a test-only training register. The large drawer balance comes from an earlier exact-integer stress-price fixture, not actual business cash. The Units / each filter focuses this scanner screenshot on ordinary unit products.',
    contentType: 'text/plain',
  });
  await page.getByRole('button', { name: /^All products/ }).click();

  await barcode.fill('000MEASURED');
  await barcode.press('Enter');
  await expect(page.getByLabel('Measured weight', { exact: true })).toBeVisible();
  await expect(page.locator('.cart-line')).toHaveCount(2);
  await page.getByRole('button', { name: 'Close dialog', exact: true }).click();
  expect((await api(request, 'get_state')).draft.lines).toHaveLength(2);
  await page.getByRole('button', { name: 'Clear', exact: true }).click();
  await expect(page.locator('.cart-line')).toHaveCount(0);
  await expect.poll(async () => (await api(request, 'get_state')).draft.lines.length).toBe(0);

  // Another register window changes prices. A lookup at the new revision must
  // not silently append to this window's older catalog snapshot.
  const changed = await api(request, 'get_state');
  changed.config.catalog.find((item: { sku: string }) => item.sku === 'SCAN-UNIT').pricing.unit_price_minor = '500';
  await api(request, 'put_config', { config: changed.config, expected_config_revision: changed.config_revision });
  await barcode.fill('001234567890');
  await barcode.press('Enter');
  await expect(page.getByRole('alert')).toContainText('Catalog changed. Reload the register before scanning again.');
  await expect(page.locator('.cart-line')).toHaveCount(0);
  expect((await api(request, 'get_state')).draft.lines).toHaveLength(0);
  await page.reload();
  await barcode.fill('001234567890');
  await barcode.press('Enter');
  await expect(page.locator('.cart-line')).toHaveCount(1);
  await expect(page.locator('.total-row')).toContainText('5.00');
});

test('cancel, navigation and clearing the cart discard pending barcode lookup results', async ({ page, request }) => {
  await page.goto('/');
  for (const action of ['cancel', 'navigate', 'clear'] as const) {
    if (action === 'clear') {
      await page.getByRole('button', { name: /Scanner SCAN-UNIT/ }).click();
      await page.getByRole('button', { name: 'Add to cart', exact: true }).click();
      await expect(page.locator('.cart-line')).toHaveCount(1);
      await expect.poll(async () => (await api(request, 'get_state')).draft.lines.length).toBe(1);
    }
    const pending = await delayNextLookup(page);
    await page.getByLabel('Barcode', { exact: true }).fill('001234567890');
    await page.getByLabel('Barcode', { exact: true }).press('Enter');
    await pending.received;
    // Cancelling also invalidates a deliberate scan queued behind this lookup.
    await page.getByLabel('Barcode', { exact: true }).fill('001234567890');
    await page.getByLabel('Barcode', { exact: true }).press('Enter');
    if (action === 'cancel') await page.getByRole('button', { name: 'Cancel scan', exact: true }).click();
    else if (action === 'clear') await page.getByRole('button', { name: 'Clear', exact: true }).click();
    else {
      await page.getByRole('button', { name: 'Configuration', exact: true }).click();
      await expect(page.getByRole('heading', { name: 'Configuration.', exact: true })).toBeVisible();
    }
    pending.release();
    await pending.fulfilled;
    if (action === 'navigate') await page.getByRole('button', { name: 'Register', exact: true }).click();
    // A second lookup is a causal barrier after the late response. It must not
    // resurrect the cancelled product or leave the scanner permanently busy.
    await page.getByLabel('Barcode', { exact: true }).fill('STILL-UNKNOWN');
    await page.getByLabel('Barcode', { exact: true }).press('Enter');
    await expect(page.getByRole('alert')).toContainText('No product has this exact barcode');
    await expect(page.locator('.cart-line')).toHaveCount(0);
    await expect.poll(async () => (await api(request, 'get_state')).draft.lines.length).toBe(0);
    await page.unrouteAll({ behavior: 'wait' });
  }
});

test('ordinary count checkout has no mass and receipt snapshot survives catalog edits and refund', async ({ page, request }, info) => {
  const initial = await api(request, 'get_state');
  let ledger = await api(request, 'get_cash_ledger');
  if (!ledger.active_session) {
    await api(request, 'open_cash_session', { idempotency_key: 'unit-e2e-open', opening_float_minor: '10000', expected_currency: initial.config.currency });
    ledger = await api(request, 'get_cash_ledger');
  }
  await page.goto('/');
  await page.getByRole('button', { name: /Scanner SCAN-UNIT/ }).click();
  await page.getByLabel('Number of units', { exact: true }).fill('3');
  await page.getByRole('button', { name: 'Add to cart', exact: true }).click();
  await expect(page.locator('.total-row')).toContainText('12.75');
  await expect.poll(async () => (await api(request, 'get_state')).draft.lines.length).toBe(1);
  const draft = (await api(request, 'get_state')).draft;
  const quote = await api(request, 'quote_cart', { lines: draft.lines });
  expect(quote.groups).toEqual([]);
  expect(quote.lines[0]).toMatchObject({ quantity: { kind: 'unit', count: '3' }, declared_content_grams: null, amount_minor: '1275', explanation: { kind: 'fixed_unit', count: '3', unit_price_minor: '425' } });
  await page.getByRole('button', { name: /Charge cash/ }).click();
  await page.getByLabel('Cash received (USD)', { exact: true }).fill('20');
  await page.getByRole('button', { name: 'Complete cash sale', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Sale saved.', exact: true })).toBeVisible();
  const receipt = page.locator('#print-receipt');
  await expect(receipt).toContainText('3 units');
  await expect(receipt).not.toContainText(/\bg\b|grams|declared|measured/);
  await expect(receipt.locator('.receipt-total')).toContainText('12.75');
  await expect(receipt.locator('.change-row')).toContainText('7.25');
  const saleId = await receipt.getAttribute('data-sale-id');
  expect(saleId).toBeTruthy();
  const original = await api(request, 'get_receipt', { sale_id: saleId });
  const originalHtml = await receipt.innerHTML();
  await capture(page, info, 'unit-receipt', { receipt: true });
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  const changed = await api(request, 'get_state');
  const item = changed.config.catalog.find((item: { sku: string }) => item.sku === 'SCAN-UNIT');
  Object.assign(item, { name: 'Changed current unit', barcode: 'NEW-UNIT-BARCODE', archived: true });
  item.pricing.unit_price_minor = '9900';
  await api(request, 'put_config', { config: changed.config, expected_config_revision: changed.config_revision });
  const refund = await api(request, 'refund_cash_sale', { idempotency_key: 'unit-e2e-refund', sale_id: saleId, reason: 'Unit product training return', expected_session_id: ledger.active_session.session_id, expected_currency: initial.config.currency });
  expect(refund.amount_minor).toBe('1275');
  await page.reload();
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await page.locator('.sale-row').filter({ hasText: original.receipt_number }).click();
  await expect(page.locator('.refund-status')).toContainText('Fully refunded');
  expect(await receipt.innerHTML()).toBe(originalHtml);
  expect(await api(request, 'get_receipt', { sale_id: saleId })).toEqual(original);
});
