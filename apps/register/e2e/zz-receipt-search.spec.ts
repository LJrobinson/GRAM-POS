import { test, expect, type APIRequestContext, type Locator, type Page, type TestInfo } from '@playwright/test';
import type { CartLine, Config, Currency, Receipt, SaleSummary, StoreSettings } from '../src/types';
import { fixtureCommand, reloadRegister } from './fixture-command';

// Real Rust/SQLite training fixtures only. Routes delay actual service responses.
// A different browser zone catches accidental host-timezone formatting.
test.use({ timezoneId: 'Pacific/Honolulu', locale: 'en-US' });
const currency: Currency = { code: 'USD', minor_unit_exponent: 2 };
const storeTimezone = 'Asia/Tokyo';
const branding: StoreSettings = { display_name: 'Cedar Market', receipt_header: 'Receipt lookup training workspace',
  receipt_contact: 'example.invalid', receipt_footer: 'Training only · no live payment', timezone: storeTimezone };
type Session = { session_id: string; expected_cash_minor: string; movement_count: string };
const api = async (request: APIRequestContext, name: string, payload: unknown = {}) => {
  const result = await fixtureCommand(request, name, payload);
  expect(result.ok, JSON.stringify(result)).toBe(true);
  return result.data;
};
const sales = async (request: APIRequestContext): Promise<SaleSummary[]> => (await api(request, 'list_sales')).sales;
const ledger = (request: APIRequestContext) => api(request, 'get_cash_ledger');
const search = (page: Page) => page.getByRole('searchbox', { name: 'Find a saved receipt', exact: true });
const row = (page: Page, receipt: Receipt) => page.locator('.sale-row').filter({ has: page.getByText(receipt.receipt_number, { exact: true }) });
const count = (page: Page) => page.locator('.history-count');
const paper = (page: Page) => page.locator('#print-receipt');
let originalConfig: Config | undefined;
let session: Session;
async function closeSession(request: APIRequestContext, active: Session) {
  await api(request, 'close_cash_session', { session_id: active.session_id,
    counted_minor: BigInt(active.expected_cash_minor) < 0n ? '0' : active.expected_cash_minor,
    expected_movement_count: active.movement_count });
}
test.beforeEach(async ({ request }) => {
  originalConfig = undefined;
  if (!(await api(request, 'get_state')).configured) await api(request, 'initialize_demo');
  await api(request, 'save_draft', { lines: [] });
  const active = (await ledger(request)).active_session;
  if (active) await closeSession(request, active);
  const state = await api(request, 'get_state');
  originalConfig = structuredClone(state.config);
  const config: Config = { ...state.config, currency, store_settings: branding,
    catalog: [{ sku: 'RECEIPT-SEARCH-UNIT', name: 'Market tote (receipt search training)', kind: 'unit', archived: false,
      declared_content_grams: null, pricing: { kind: 'fixed_unit', unit_price_minor: '425' } }],
  };
  await api(request, 'put_config', { config, expected_config_revision: state.config_revision });
  session = await api(request, 'open_cash_session', { idempotency_key: crypto.randomUUID(), opening_float_minor: '5000', expected_currency: currency });
});
test.afterEach(async ({ context, request }) => {
  for (const page of context.pages()) await page.close();
  await api(request, 'save_draft', { lines: [] });
  const active = (await ledger(request)).active_session;
  if (active) await closeSession(request, active);
  if (originalConfig) {
    const state = await api(request, 'get_state');
    await api(request, 'put_config', { config: originalConfig, expected_config_revision: state.config_revision });
  }
});
async function sale(request: APIRequestContext, units = '1'): Promise<Receipt> {
  const lines: CartLine[] = [{ id: crypto.randomUUID(), sku: 'RECEIPT-SEARCH-UNIT', quantity: { kind: 'unit', count: units } }];
  const quote = await api(request, 'quote_cart', { lines });
  return api(request, 'complete_cash_sale', { idempotency_key: crypto.randomUUID(), lines,
    tender_minor: quote.total_minor, expected_total_minor: quote.total_minor, expected_config_revision: quote.config_revision,
    expected_session_id: session.session_id, expected_currency: currency });
}
async function openHistory(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await expect(search(page)).toBeVisible();
  await expect(count(page)).toBeVisible();
}
async function capture(page: Page, info: TestInfo, name: string, focus: Locator) {
  for (const [width, height, size] of [[1440, 1000, 'desktop'], [390, 844, 'phone']] as const) {
    await page.setViewportSize({ width, height });
    await focus.scrollIntoViewIfNeeded();
    await expect(focus).toBeInViewport();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    const path = info.outputPath(`${name}-${size}.png`);
    await page.screenshot({ path });
    await info.attach(`${name} ${size} (synthetic training fixture)`, { path, contentType: 'image/png' });
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
}
async function expectMatches(page: Page, all: SaleSummary[], query: string) {
  // Independent expected set, not the production filtering helper.
  const needle = query.trim().toLowerCase();
  const expected = all.filter(item => [item.receipt_number, item.sale_id].some(value => value.toLowerCase().includes(needle)));
  await search(page).fill(query);
  await expect(count(page)).toHaveText(`${expected.length} of ${all.length} saved sales`);
  await expect(page.locator('.sale-row > span:first-child > strong')).toHaveText(expected.map(item => item.receipt_number));
  return expected;
}

test('literal search covers all saved sales on desktop and phone without changing records', async ({ page, request }, info) => {
  test.setTimeout(180000);
  const oldest = await sale(request);
  // A target older than 100 subsequent real sales catches recent-only shortcuts.
  for (let index = 0; index < 101; index++) await sale(request);
  const all = await sales(request);
  expect(all.findIndex(item => item.sale_id === oldest.sale_id)).toBeGreaterThanOrEqual(101);
  const stateBefore = await api(request, 'get_state');
  const ledgerBefore = await ledger(request);
  const browserCommands: string[] = [];
  page.on('request', req => { if (req.url().endsWith('/api/command')) browserCommands.push(req.postDataJSON().command); });
  await openHistory(page);
  await expect(count(page)).toHaveText(`${all.length} of ${all.length} saved sales`);
  await expect(page.locator('.sale-row')).toHaveCount(all.length);
  await expect(page.getByRole('button', { name: 'Clear search', exact: true })).toBeDisabled();
  await expectMatches(page, all, `  ${oldest.receipt_number.toLowerCase()}  `);
  await expect(row(page, oldest)).toBeVisible();
  await expect(page.locator('.history-zone')).toContainText(storeTimezone);
  const tokyoTime = new Date(Number(oldest.created_at_unix) * 1000 + 9 * 60 * 60 * 1000).toISOString().slice(0, 19).replace('T', ' ');
  await expect(row(page, oldest).locator('.history-time')).toHaveText(`${tokyoTime} GMT+09:00 [${storeTimezone}]`);
  await capture(page, info, 'receipt-search-match', search(page));
  await expectMatches(page, all, oldest.receipt_number.slice(0, -1).toLowerCase());
  await expectMatches(page, all, oldest.sale_id);
  await expect(row(page, oldest)).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  for (const literal of ['.*', '%', '_', '[', 'Market tote']) {
    expect(await expectMatches(page, all, literal)).toHaveLength(0);
    await expect(page.getByRole('heading', { name: 'No matching receipts.', exact: true })).toBeVisible();
  }
  await capture(page, info, 'receipt-search-no-match', search(page));
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole('button', { name: 'Show all saved sales', exact: true }).click();
  await expect(search(page)).toHaveValue('');
  await expect(search(page)).toBeFocused();
  await expect(count(page)).toHaveText(`${all.length} of ${all.length} saved sales`);
  await search(page).fill(oldest.receipt_number);
  await page.getByRole('button', { name: 'Clear search', exact: true }).click();
  await expect(search(page)).toHaveValue('');
  await expect(search(page)).toBeFocused();
  expect(await api(request, 'get_state')).toEqual(stateBefore);
  expect(await ledger(request)).toEqual(ledgerBefore);
  expect(await sales(request)).toEqual(all);
  expect(await api(request, 'get_receipt', { sale_id: oldest.sale_id })).toEqual(oldest);
  expect(browserCommands.filter(name => !['get_state', 'get_cash_ledger', 'list_sales', 'get_receipt', 'quote_cart'].includes(name))).toEqual([]);
});

test('filtered receipt and refund keep the original sale identity, pricing, branding, and timezone', async ({ page, request }, info) => {
  let state = await api(request, 'get_state');
  await api(request, 'put_store_settings', { store_settings: { ...branding, timezone: 'UTC' }, expected_config_revision: state.config_revision });
  const target = await sale(request, '2');
  const other = await sale(request, '3');
  state = await api(request, 'get_state');
  const changed: Config = { ...state.config, store_settings: { ...branding, display_name: 'Later store branding' },
    catalog: [{ ...state.config.catalog[0], name: 'Renamed later product', pricing: { kind: 'fixed_unit', unit_price_minor: '999' } }],
  };
  await api(request, 'put_config', { config: changed, expected_config_revision: state.config_revision });
  await openHistory(page);
  await search(page).fill(other.receipt_number);
  await row(page, other).click();
  await expect(paper(page)).toHaveAttribute('data-sale-id', other.sale_id);
  await expect(paper(page).locator('.receipt-total')).toContainText('$12.75');
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await search(page).fill(target.receipt_number.toLowerCase());
  await row(page, target).click();
  await expect(paper(page)).toHaveAttribute('data-sale-id', target.sale_id);
  await expect(paper(page)).toContainText(target.receipt_number);
  await expect(paper(page)).toContainText('Cedar Market');
  await expect(paper(page)).toContainText('Market tote (receipt search training)');
  await expect(paper(page)).not.toContainText('Later store branding');
  await expect(paper(page)).not.toContainText('Renamed later product');
  await expect(paper(page).locator('.receipt-total')).toContainText('$8.50');
  const utcTime = new Date(Number(target.created_at_unix) * 1000).toISOString().slice(0, 19).replace('T', ' ');
  await expect(paper(page).locator('.receipt-meta')).toContainText(`${utcTime} UTC`);
  await expect(paper(page)).not.toContainText(storeTimezone);
  const originalHtml = await paper(page).innerHTML();
  await capture(page, info, 'receipt-search-original-receipt', paper(page).locator('.receipt-brand'));
  await page.getByRole('button', { name: 'Refund full receipt', exact: true }).click();
  const refund = page.getByRole('dialog', { name: 'Full receipt refund', exact: true });
  await expect(refund.getByRole('heading', { level: 3 })).toHaveText(`${target.receipt_number} · $8.50`);
  await refund.getByLabel('Refund reason', { exact: true }).fill('Training search identity check');
  await refund.getByRole('button', { name: 'Review full refund', exact: true }).click();
  await capture(page, info, 'receipt-search-refund-review', refund.getByRole('heading', { level: 3 }));
  const before = await ledger(request);
  await refund.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await ledger(request)).toEqual(before);
  await expect(search(page)).toHaveValue(target.receipt_number.toLowerCase());
  await row(page, target).click();
  await page.getByRole('button', { name: 'Refund full receipt', exact: true }).click();
  await refund.getByLabel('Refund reason', { exact: true }).fill('Training search identity check');
  await refund.getByRole('button', { name: 'Review full refund', exact: true }).click();
  const requestSent = page.waitForRequest(req => req.url().endsWith('/api/command') && req.postDataJSON()?.command === 'refund_cash_sale');
  await refund.getByRole('button', { name: 'Confirm full cash refund', exact: true }).click();
  expect((await requestSent).postDataJSON().payload.sale_id).toBe(target.sale_id);
  await expect(refund).toHaveCount(0);
  const after = await ledger(request);
  expect(after.refunds).toHaveLength(before.refunds.length + 1);
  expect(after.refunds.find((item: { sale_id: string }) => item.sale_id === target.sale_id)).toMatchObject({ original_receipt_number: target.receipt_number, amount_minor: '850' });
  expect(after.refunds.some((item: { sale_id: string }) => item.sale_id === other.sale_id)).toBe(false);
  expect(await api(request, 'get_receipt', { sale_id: target.sale_id })).toEqual(target);
  expect(await api(request, 'get_receipt', { sale_id: other.sale_id })).toEqual(other);
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await search(page).fill(target.receipt_number);
  await row(page, target).click();
  await expect(page.getByLabel('Current refund status', { exact: true })).toContainText('Fully refunded');
  expect(await paper(page).innerHTML()).toBe(originalHtml);
  await expect(page.getByRole('button', { name: 'Refund full receipt', exact: true })).toHaveCount(0);
});

test('refresh retains the query and adopts new sales; navigation and reload reset transient search', async ({ page, request }) => {
  const target = await sale(request);
  await openHistory(page);
  await search(page).fill('demo-');
  const newest = await sale(request, '2');
  const all = await sales(request);
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await expect(search(page)).toHaveValue('demo-');
  await expect(count(page)).toHaveText(`${all.length} of ${all.length} saved sales`);
  await expect(row(page, newest)).toBeVisible();
  await search(page).fill(target.receipt_number);
  await page.getByRole('button', { name: 'Register', exact: true }).click();
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await expect(search(page)).toHaveValue('');
  await expect(count(page)).toHaveText(`${all.length} of ${all.length} saved sales`);
  await search(page).fill(target.receipt_number);
  await row(page, target).click();
  await expect(paper(page)).toHaveAttribute('data-sale-id', target.sale_id);
  await reloadRegister(page);
  await expect(paper(page)).toHaveCount(0);
  await page.getByRole('button', { name: 'Sales history', exact: true }).click();
  await expect(search(page)).toHaveValue('');
  await expect(count(page)).toHaveText(`${all.length} of ${all.length} saved sales`);
  await search(page).fill(target.sale_id);
  await row(page, target).click();
  await expect(paper(page)).toHaveAttribute('data-sale-id', target.sale_id);
  expect(await api(request, 'get_receipt', { sale_id: target.sale_id })).toEqual(target);
});

for (const interruption of ['search change', 'refresh', 'navigation'] as const) {
  test(`a delayed receipt cannot reopen after ${interruption}, and a later receipt opens correctly`, async ({ page, request }) => {
    const old = await sale(request);
    const newer = await sale(request, '2');
    const before = await ledger(request);
    await openHistory(page);
    await search(page).fill(old.receipt_number);
    let release!: () => void;
    let started!: () => void;
    let delivered!: () => void;
    let delayed = false;
    const held = new Promise<void>(resolve => { release = resolve; });
    const fetched = new Promise<void>(resolve => { started = resolve; });
    const finished = new Promise<void>(resolve => { delivered = resolve; });
    await page.route('**/api/command', async route => {
      const body = route.request().postDataJSON();
      if (!delayed && body.command === 'get_receipt' && body.payload.sale_id === old.sale_id) {
        delayed = true;
        const response = await route.fetch();
        expect((await response.json()).ok).toBe(true);
        started();
        await held;
        try { await route.fulfill({ response }); }
        finally { delivered(); }
      } else await route.continue();
    });
    try {
      await row(page, old).click();
      await fetched;
      await expect(row(page, old)).toBeDisabled();
      if (interruption === 'search change') await search(page).fill(newer.receipt_number);
      else if (interruption === 'refresh') {
        await page.getByRole('button', { name: 'Refresh', exact: true }).click();
        await expect(count(page)).toBeVisible();
        await expect(search(page)).toHaveValue(old.receipt_number);
      } else {
        await page.getByRole('button', { name: 'Cash sessions', exact: true }).click();
        await expect(search(page)).toHaveCount(0);
      }
      release();
      await finished;
      // Let fetch callbacks and React commit before asserting absence. A plain
      // negative assertion could succeed before the stale response is handled.
      await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
      await expect(paper(page)).toHaveCount(0);
      await expect(page.getByRole('dialog', { name: 'Sale saved.', exact: true })).toHaveCount(0);
      if (interruption === 'navigation') {
        await page.getByRole('button', { name: 'Sales history', exact: true }).click();
        await expect(search(page)).toHaveValue('');
      }
      await search(page).fill(newer.receipt_number);
      await expect(row(page, newer)).toBeEnabled();
      await row(page, newer).click();
      await expect(paper(page)).toHaveAttribute('data-sale-id', newer.sale_id);
      await expect(paper(page)).toContainText(newer.receipt_number);
      await expect(paper(page)).not.toContainText(old.receipt_number);
      expect(await ledger(request)).toEqual(before);
      expect(await api(request, 'get_receipt', { sale_id: old.sale_id })).toEqual(old);
      expect(await api(request, 'get_receipt', { sale_id: newer.sale_id })).toEqual(newer);
    } finally { release(); }
  });
}
