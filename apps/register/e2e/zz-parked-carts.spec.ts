import { test, expect, type APIRequestContext, type Page, type TestInfo } from '@playwright/test';
import type { CartLine, Config } from '../src/types';
import { reloadRegister } from './fixture-command';

const randomUUID = () => crypto.randomUUID();

// Real Rust + SQLite throughout. Interception delays or loses real responses;
// it never supplies mock application state, prices, or persistence outcomes.
const command = async (request: APIRequestContext, name: string, payload: unknown = {}) => {
  const response = await request.post('/api/command', { data: { command: name, payload } });
  expect(response.ok()).toBe(true);
  return response.json();
};
const api = async (request: APIRequestContext, name: string, payload: unknown = {}) => {
  const result = await command(request, name, payload);
  expect(result.ok, JSON.stringify(result)).toBe(true);
  return result.data;
};
async function mutate(request: APIRequestContext, name: string, payload: object = {}) {
  const state = await api(request, 'get_state');
  return api(request, name, { idempotency_key: randomUUID(), expected_draft_revision: state.draft_revision, ...payload });
}
const save = (request: APIRequestContext, lines: CartLine[]) => mutate(request, 'save_active_draft', { lines });
async function parkFixture(request: APIRequestContext, lines: CartLine[], label: string | null = 'Training pickup') {
  const saved = await save(request, lines);
  const state = await mutate(request, 'park_draft', { label });
  expect(state.active_draft).toBeNull();
  return saved.active_draft.id as string;
}
const mixedLines = (): CartLine[] => [
  { id: 'training-measured', sku: 'PARK-MEASURED', quantity: { kind: 'measured', grams: '3.621' } },
  { id: 'training-pack', sku: 'PARK-PACK', quantity: { kind: 'prepacked', count: '2' } },
  { id: 'training-unit', sku: 'PARK-UNIT', quantity: { kind: 'unit', count: '3' } },
];
const singleLine = (id = 'training-unit', count = '1'): CartLine[] => [{ id, sku: 'PARK-UNIT', quantity: { kind: 'unit', count } }];
let originalConfig: Config | undefined;
let originalParkedIds: Set<string>;
test.beforeEach(async ({ request }) => {
  originalConfig = undefined;
  originalParkedIds = new Set();
  if (!(await api(request, 'get_state')).configured) await api(request, 'initialize_demo');
  await save(request, []);
  const state = await api(request, 'get_state');
  originalConfig = structuredClone(state.config);
  originalParkedIds = new Set(state.parked_drafts.map((draft: { id: string }) => draft.id));
  const config: Config = {
    ...state.config,
    store_settings: { display_name: 'Cedar Market', receipt_header: 'Parked-cart training workspace', receipt_contact: 'example.invalid', receipt_footer: 'Training only · no live payment', timezone: 'UTC' },
    groups: [{ id: 'park-training', name: 'Training weight schedule', tiers: [{ nominal_grams: '1.000', tolerance_grams: '0.000', price_minor: '100' }], overage_policy: 'full_actual_at_tier_rate', rounding: 'half_up', below_smallest: 'proportional_at_smallest_tier', tolerance_scope: 'aggregate_pricing_group' }],
    catalog: [
      { sku: 'PARK-MEASURED', name: 'Loose tea (training)', barcode: '000PARKWEIGHT', kind: 'measured', declared_content_grams: null, archived: false, pricing: { kind: 'weight_group', group_id: 'park-training' } },
      { sku: 'PARK-PACK', name: 'Tea gift pack (training)', kind: 'prepacked', declared_content_grams: '25.000', archived: false, pricing: { kind: 'fixed_unit', unit_price_minor: '725' } },
      { sku: 'PARK-UNIT', name: 'Market tote (training)', barcode: '000PARKUNIT', kind: 'unit', declared_content_grams: null, archived: false, pricing: { kind: 'fixed_unit', unit_price_minor: '425' } },
    ],
  };
  await api(request, 'put_config', { config, expected_config_revision: state.config_revision });
});
test.afterEach(async ({ context, request }) => {
  // Stop every window and queued effect before restoring the shared test DB.
  for (const page of context.pages()) await page.close();
  await save(request, []);
  const state = await api(request, 'get_state');
  for (const draft of state.parked_drafts as { id: string }[]) {
    if (!originalParkedIds.has(draft.id)) await mutate(request, 'discard_parked_draft', { draft_id: draft.id });
  }
  if (originalConfig) {
    const current = await api(request, 'get_state');
    await api(request, 'put_config', { config: originalConfig, expected_config_revision: current.config_revision });
  }
});
const parkedDialog = (page: Page) => page.getByRole('dialog', { name: 'Parked carts', exact: true });
const preview = (page: Page) => page.getByTestId('parked-preview');
async function openPreview(page: Page, id: string) {
  await page.getByRole('button', { name: /^Parked carts/ }).click();
  await parkedDialog(page).getByTestId(`parked-draft-${id}`).click();
  await expect(preview(page)).toBeVisible();
}
async function add(page: Page, name: string, quantityLabel: string, quantity: string) {
  await page.getByRole('button', { name: new RegExp(name) }).click();
  await page.getByLabel(quantityLabel, { exact: true }).fill(quantity);
  await page.getByRole('button', { name: 'Add to cart', exact: true }).click();
}
async function park(page: Page, label?: string) {
  await page.getByRole('button', { name: 'Park cart', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Park this cart', exact: true });
  if (label !== undefined) await dialog.getByLabel('Cart label (optional)', { exact: true }).fill(label);
  await dialog.getByRole('button', { name: 'Park this cart', exact: true }).click();
}
async function capture(page: Page, info: TestInfo, name: string) {
  for (const [width, height, size] of [[1440, 1000, 'desktop'], [390, 844, 'phone']] as const) {
    await page.setViewportSize({ width, height });
    await preview(page).scrollIntoViewIfNeeded();
    await expect(preview(page)).toBeInViewport();
    if (name !== 'parked-carts-preview') {
      await preview(page).getByRole('alert').scrollIntoViewIfNeeded();
      await expect(preview(page).getByRole('alert')).toBeInViewport({ ratio: 0.6 });
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    const path = info.outputPath(`${name}-${size}.png`);
    await page.screenshot({ path }); // Full-page resizing displaces fixed dialogs.
    await info.attach(`${name} ${size} (training fixture)`, { path, contentType: 'image/png' });
  }
  await info.attach('Training fixture context', { body: 'Test-only Cedar Market register. Products, labels, prices and cash ledger are synthetic. No customer information or live payment. Parked carts are drafts, not inventory reservations or completed sales.', contentType: 'text/plain' });
  await page.setViewportSize({ width: 1440, height: 1000 });
}

test('mixed cart parks with label, previews and reloads exact quantities without creating a sale', async ({ page, request }, info) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const salesBefore = await api(request, 'list_sales');
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Park cart', exact: true })).toBeDisabled();
  await add(page, 'Loose tea', 'Measured weight', '3.621');
  await add(page, 'Tea gift pack', 'Number of packages', '2');
  await add(page, 'Market tote', 'Number of units', '3');
  await expect.poll(async () => (await api(request, 'get_state')).draft.lines.length).toBe(3);
  const active = await api(request, 'get_state');
  expect(active.draft.lines.map((line: CartLine) => line.quantity)).toEqual(mixedLines().map(line => line.quantity));
  await park(page, 'Pickup 14 · training');
  await expect(page.locator('.cart-line')).toHaveCount(0);
  await expect.poll(async () => (await api(request, 'get_state')).parked_drafts.some((draft: { id: string }) => draft.id === active.active_draft.id)).toBe(true);
  await reloadRegister(page);
  await openPreview(page, active.active_draft.id);
  await expect(parkedDialog(page)).toContainText('Pickup 14 · training');
  await expect(preview(page)).toContainText('3.621');
  const parked = await api(request, 'preview_parked_draft', { draft_id: active.active_draft.id });
  expect(parked.lines).toEqual(active.draft.lines);
  expect(parked.quote.lines.map((line: CartLine) => line.quantity)).toEqual(mixedLines().map(line => line.quantity));
  expect(parked.quote.total_minor).toBe('3087'); // 362 + 2 × 725 + 3 × 425.
  expect(parked.issues).toEqual([]);
  expect(await api(request, 'list_sales')).toEqual(salesBefore);
  await capture(page, info, 'parked-carts-preview');
  await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
  await expect(page.locator('.cart-line')).toHaveCount(3);
  await reloadRegister(page);
  const resumed = await api(request, 'get_state');
  expect(resumed.active_draft.id).toBe(active.active_draft.id);
  expect(resumed.draft.lines).toEqual(active.draft.lines);
  expect(resumed.parked_drafts.some((draft: { id: string }) => draft.id === active.active_draft.id)).toBe(false);
  expect(errors).toEqual([]);
});

test('park cancel and optional label preserve work; clear and parked discard require confirmation', async ({ page, request }) => {
  await save(request, singleLine());
  await page.goto('/');
  const before = await api(request, 'get_state');
  await page.getByRole('button', { name: 'Park cart', exact: true }).click();
  await page.getByRole('dialog', { name: 'Park this cart', exact: true }).getByRole('button', { name: 'Close dialog', exact: true }).click();
  expect((await api(request, 'get_state')).draft).toEqual(before.draft);
  page.once('dialog', dialog => dialog.dismiss());
  await page.getByRole('button', { name: 'Clear', exact: true }).click();
  await expect(page.locator('.cart-line')).toHaveCount(1);
  expect((await api(request, 'get_state')).draft).toEqual(before.draft);
  await park(page);
  await expect(page.locator('.cart-line')).toHaveCount(0);
  await openPreview(page, before.active_draft.id);
  await expect(parkedDialog(page)).toContainText('Unlabeled cart');
  const persisted = await api(request, 'get_state');
  expect(persisted.parked_drafts.find((draft: { id: string }) => draft.id === before.active_draft.id).label).toBeNull();
  page.once('dialog', dialog => dialog.dismiss());
  await preview(page).getByRole('button', { name: 'Discard parked cart', exact: true }).click();
  expect((await api(request, 'get_state')).parked_drafts).toEqual(persisted.parked_drafts);
  page.once('dialog', dialog => dialog.accept());
  await preview(page).getByRole('button', { name: 'Discard parked cart', exact: true }).click();
  await expect(parkedDialog(page).getByTestId(`parked-draft-${before.active_draft.id}`)).toHaveCount(0);
  await reloadRegister(page);
  expect((await api(request, 'get_state')).parked_drafts.some((draft: { id: string }) => draft.id === before.active_draft.id)).toBe(false);
});

test('preview uses current price with explicit warnings and does not rewrite saved quantities', async ({ page, request }, info) => {
  const lines = mixedLines();
  const id = await parkFixture(request, lines, 'Review updated prices · training');
  const changed = await api(request, 'get_state');
  const item = changed.config.catalog.find((item: { sku: string }) => item.sku === 'PARK-UNIT');
  item.name = 'Updated market tote (training)';
  item.pricing.unit_price_minor = '500';
  await api(request, 'put_config', { config: changed.config, expected_config_revision: changed.config_revision });
  await page.goto('/');
  await openPreview(page, id);
  const current = await api(request, 'preview_parked_draft', { draft_id: id });
  expect(current.lines).toEqual(lines);
  expect(current.issues.map((issue: { code: string }) => issue.code)).toContain('PRICE_CHANGED');
  expect(current.issues.every((issue: { blocking: boolean }) => !issue.blocking)).toBe(true);
  expect(current.quote.total_minor).toBe('3312');
  await expect(preview(page).getByRole('alert')).toContainText(/price/i);
  await capture(page, info, 'parked-cart-price-warning');
  await preview(page).getByLabel('I reviewed the changes. Keep the original quantities when resuming.', { exact: true }).check();
  await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
  await expect(page.locator('.total-row')).toContainText('33.12');
  await reloadRegister(page);
  const resumed = await api(request, 'get_state');
  expect(resumed.draft.lines).toEqual(lines);
  expect(resumed.active_draft.issues.some((issue: { code: string }) => issue.code === 'PRICE_CHANGED')).toBe(true);
});

test('missing archived and changed-type products stay visible and cannot be silently reinterpreted', async ({ page, request }, info) => {
  const lines = mixedLines();
  const id = await parkFixture(request, lines, 'Needs product review · training');
  const changed = await api(request, 'get_state');
  changed.config.catalog = changed.config.catalog.filter((item: { sku: string }) => item.sku !== 'PARK-MEASURED');
  changed.config.catalog.find((item: { sku: string }) => item.sku === 'PARK-PACK').archived = true;
  Object.assign(changed.config.catalog.find((item: { sku: string }) => item.sku === 'PARK-UNIT'), { kind: 'prepacked', declared_content_grams: '1.000' });
  await api(request, 'put_config', { config: changed.config, expected_config_revision: changed.config_revision });
  await page.goto('/');
  await openPreview(page, id);
  const current = await api(request, 'preview_parked_draft', { draft_id: id });
  expect(current.lines).toEqual(lines);
  expect(current.quote).toBeNull();
  expect(current.issues.filter((issue: { blocking: boolean }) => issue.blocking).map((issue: { code: string }) => issue.code)).toEqual(expect.arrayContaining(['PRODUCT_MISSING', 'PRODUCT_ARCHIVED', 'PRODUCT_TYPE_CHANGED']));
  await expect(preview(page)).toContainText('3.621');
  await expect(preview(page)).toContainText(/missing|no longer/i);
  await expect(preview(page)).toContainText(/archived/i);
  await expect(preview(page)).toContainText(/type|kind/i);
  await capture(page, info, 'parked-cart-product-review');
  await preview(page).getByLabel('I reviewed the changes. Keep the original quantities when resuming.', { exact: true }).check();
  await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
  await expect(page.locator('.cart-line')).toHaveCount(3);
  await expect(page.getByRole('button', { name: /Charge cash/ })).toBeDisabled();
  await reloadRegister(page);
  expect((await api(request, 'get_state')).draft.lines).toEqual(lines);
  await expect(page.locator('.cart-line')).toHaveCount(3);
  const unit = page.locator('.cart-line').filter({ hasText: 'Market tote' });
  await expect(unit).toContainText('3 units');
  await expect(unit).not.toContainText('3 × 1.000');
});

test('another active cart blocks resume until an explicitly confirmed clear', async ({ page, request }) => {
  const parkedLines = singleLine('parked-line', '2');
  const id = await parkFixture(request, parkedLines);
  await save(request, singleLine('different-active', '5'));
  await page.goto('/');
  await openPreview(page, id);
  await expect(preview(page).getByRole('button', { name: 'Resume cart', exact: true })).toBeDisabled();
  const before = await api(request, 'get_state');
  const rejected = await command(request, 'resume_parked_draft', { idempotency_key: randomUUID(), draft_id: id, expected_draft_revision: before.draft_revision, expected_config_revision: before.config_revision });
  expect(rejected.ok).toBe(false);
  expect((await api(request, 'get_state')).draft).toEqual(before.draft);
  expect((await api(request, 'get_state')).parked_drafts).toEqual(before.parked_drafts);
  await parkedDialog(page).getByRole('button', { name: 'Close dialog', exact: true }).click();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Clear', exact: true }).click();
  await expect.poll(async () => (await api(request, 'get_state')).active_draft).toBeNull();
  await openPreview(page, id);
  await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
  await expect.poll(async () => (await api(request, 'get_state')).draft.lines).toEqual(parkedLines);
});

for (const action of ['park', 'resume'] as const) {
  test(`lost ${action} response survives reload and retries the same saved action exactly once`, async ({ page, request }) => {
    const lines = mixedLines();
    const id = action === 'resume' ? await parkFixture(request, lines) : (await save(request, lines)).active_draft.id;
    await page.goto('/');
    if (action === 'resume') await openPreview(page, id);
    const attempts: unknown[] = [];
    let dropped = false;
    const name = action === 'park' ? 'park_draft' : 'resume_parked_draft';
    await page.route('**/api/command', async route => {
      const body = route.request().postDataJSON();
      if (body.command === name) {
        attempts.push(body.payload);
        if (!dropped) {
          dropped = true;
          const result = await (await route.fetch()).json();
          expect(result.ok, JSON.stringify(result)).toBe(true);
          await route.abort('failed');
          return;
        }
      }
      await route.continue();
    });
    if (action === 'park') await park(page, 'Recovery training cart');
    else await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
    const recovery = page.getByRole('dialog', { name: 'Recover saved cart action', exact: true });
    await expect(recovery).toBeVisible();
    const committed = await api(request, 'get_state');
    expect(committed.parked_drafts.filter((draft: { id: string }) => draft.id === id)).toHaveLength(action === 'park' ? 1 : 0);
    expect(committed.draft.lines).toEqual(action === 'park' ? [] : lines);
    expect(attempts).toHaveLength(1);
    await reloadRegister(page);
    await expect(recovery).toBeVisible();
    await recovery.getByRole('button', { name: 'Retry saved action', exact: true }).click();
    await expect(recovery).toHaveCount(0);
    expect(attempts).toHaveLength(2);
    expect(attempts[1]).toEqual(attempts[0]);
    const recovered = await api(request, 'get_state');
    expect(recovered.draft_revision).toBe(committed.draft_revision);
    expect(recovered.draft).toEqual(committed.draft);
    expect(recovered.parked_drafts).toEqual(committed.parked_drafts);
    if (action === 'resume') expect(recovered.active_draft.id).toBe(id);
  });
}

test('parking cancels delayed and queued scanner lookups instead of adding them to the next customer', async ({ page, request }) => {
  const lines = singleLine();
  const active = await save(request, lines);
  await page.goto('/');
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
    } else await route.continue();
  });
  try {
    const barcode = page.getByLabel('Barcode', { exact: true });
    await barcode.fill('000PARKUNIT');
    await barcode.press('Enter');
    await received;
    await barcode.fill('000PARKUNIT');
    await barcode.press('Enter');
    await park(page, 'Scanner cancellation training');
    await expect(page.locator('.cart-line')).toHaveCount(0);
    release();
    await fulfilled;
    // A subsequent real lookup gives a causal barrier after the ignored result.
    await barcode.fill('UNKNOWN-AFTER-PARK');
    await barcode.press('Enter');
    await expect(page.getByRole('alert')).toContainText('No product has this exact barcode');
    await expect(page.locator('.cart-line')).toHaveCount(0);
    expect((await api(request, 'get_state')).draft.lines).toEqual([]);
    expect((await api(request, 'preview_parked_draft', { draft_id: active.active_draft.id })).lines).toEqual(lines);
  } finally { release(); }
});

test('in-flight debounced save blocks park until acknowledged and cannot resurrect the parked cart', async ({ page, request }) => {
  await save(request, singleLine());
  await page.goto('/');
  let release!: () => void;
  let started!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const received = new Promise<void>(resolve => { started = resolve; });
  const events: string[] = [];
  let delayed = false;
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (body.command === 'park_draft') events.push('park');
    if (!delayed && body.command === 'save_active_draft' && body.payload.lines.length === 2) {
      delayed = true;
      const response = await route.fetch();
      events.push('save-committed');
      started();
      await gate;
      events.push('save-response');
      await route.fulfill({ response });
    } else await route.continue();
  });
  try {
    await add(page, 'Tea gift pack', 'Number of packages', '2');
    await received;
    await expect(page.getByRole('button', { name: 'Park cart', exact: true })).toBeDisabled();
    expect(events).toEqual(['save-committed']);
    release();
    await park(page, 'Wait for save · training');
    await expect(page.locator('.cart-line')).toHaveCount(0);
    expect(events).toEqual(['save-committed', 'save-response', 'park']);
    const state = await api(request, 'get_state');
    const parked = state.parked_drafts.find((draft: { label: string | null }) => draft.label === 'Wait for save · training');
    const saved = await api(request, 'preview_parked_draft', { draft_id: parked.id });
    expect(saved.lines).toHaveLength(2);
    expect(saved.lines.map((line: CartLine) => line.quantity)).toEqual([{ kind: 'unit', count: '1' }, { kind: 'prepacked', count: '2' }]);
    await reloadRegister(page);
    await expect(page.locator('.cart-line')).toHaveCount(0);
    expect((await api(request, 'get_state')).active_draft).toBeNull();
  } finally { release(); }
});

test('two windows cannot both resume the same parked cart', async ({ page, context, request }) => {
  const lines = mixedLines();
  const id = await parkFixture(request, lines);
  // Separate browser contexts also isolate recovery journals, as independent tills do.
  const secondContext = await context.browser()!.newContext();
  const second = await secondContext.newPage();
  try {
    await Promise.all([page.goto('/'), second.goto('http://127.0.0.1:5173/')]);
    await Promise.all([openPreview(page, id), openPreview(second, id)]);
    await Promise.all([
      preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click(),
      preview(second).getByRole('button', { name: 'Resume cart', exact: true }).click(),
    ]);
    await expect.poll(async () => (await api(request, 'get_state')).parked_drafts.some((draft: { id: string }) => draft.id === id)).toBe(false);
    const state = await api(request, 'get_state');
    expect(state.active_draft.id).toBe(id);
    expect(state.draft.lines).toEqual(lines);
    await expect.poll(async () => (await page.getByRole('alert').count()) + (await second.getByRole('alert').count())).toBeGreaterThan(0);
    const current = await api(request, 'get_state');
    expect(current.draft).toEqual(state.draft);
    expect(current.draft_revision).toBe(state.draft_revision);
  } finally { await secondContext.close(); }
});

test('resumed checkout consumes its draft identity once even when the committed response is lost', async ({ page, request }) => {
  const lines = singleLine('checkout-unit', '3');
  const id = await parkFixture(request, lines, 'One checkout · training');
  const initial = await api(request, 'get_state');
  const ledger = await api(request, 'get_cash_ledger');
  if (!ledger.active_session) await api(request, 'open_cash_session', { idempotency_key: randomUUID(), opening_float_minor: '0', expected_currency: initial.config.currency });
  const before = await api(request, 'list_sales');
  await page.goto('/');
  await openPreview(page, id);
  await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
  await expect(page.locator('.total-row')).toContainText('12.75');
  await page.getByRole('button', { name: /Charge cash/ }).click();
  await page.getByLabel('Cash received (USD)', { exact: true }).fill('20');
  const attempts: Record<string, unknown>[] = [];
  let dropped = false;
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (body.command === 'complete_cash_sale') {
      attempts.push(body.payload);
      if (!dropped) {
        dropped = true;
        const result = await (await route.fetch()).json();
        expect(result.ok, JSON.stringify(result)).toBe(true);
        await route.abort('failed');
        return;
      }
    }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Complete cash sale', exact: true }).click({ clickCount: 2 });
  await expect(page.getByRole('button', { name: 'Retry same checkout', exact: true })).toBeEnabled();
  expect(attempts).toHaveLength(1);
  expect(attempts[0].expected_draft_id).toBe(id);
  const committed = await api(request, 'get_state');
  expect(committed.active_draft).toBeNull();
  expect(committed.draft.lines).toEqual([]);
  expect(committed.parked_drafts.some((draft: { id: string }) => draft.id === id)).toBe(false);
  expect((await api(request, 'list_sales')).sales).toHaveLength(before.sales.length + 1);
  await page.getByRole('button', { name: 'Retry same checkout', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Sale saved.', exact: true })).toBeVisible();
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);
  expect((await api(request, 'list_sales')).sales).toHaveLength(before.sales.length + 1);
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await reloadRegister(page);
  expect((await api(request, 'get_state')).active_draft).toBeNull();
  const stale = await command(request, 'resume_parked_draft', { idempotency_key: randomUUID(), draft_id: id, expected_draft_revision: (await api(request, 'get_state')).draft_revision, expected_config_revision: initial.config_revision });
  expect(stale.ok).toBe(false);
  expect((await api(request, 'list_sales')).sales).toHaveLength(before.sales.length + 1);
});

test('catalog edit after preview requires reconciliation and fresh price review before resume', async ({ page, request }) => {
  const lines = singleLine('stale-preview', '2');
  const id = await parkFixture(request, lines, 'Fresh review required · training');
  await page.goto('/');
  await openPreview(page, id);
  await expect(preview(page)).toContainText('8.50');
  const changed = await api(request, 'get_state');
  changed.config.catalog.find((item: { sku: string }) => item.sku === 'PARK-UNIT').pricing.unit_price_minor = '500';
  await api(request, 'put_config', { config: changed.config, expected_config_revision: changed.config_revision });
  await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
  const recovery = page.getByRole('dialog', { name: 'Recover saved cart action', exact: true });
  await expect(recovery).toBeVisible();
  const rejected = await api(request, 'get_state');
  expect(rejected.active_draft).toBeNull();
  expect(rejected.parked_drafts.some((draft: { id: string }) => draft.id === id)).toBe(true);
  expect((await api(request, 'preview_parked_draft', { draft_id: id })).lines).toEqual(lines);
  const adopt = recovery.getByRole('button', { name: 'Use current saved cart', exact: true });
  page.once('dialog', dialog => dialog.dismiss());
  await adopt.click();
  await expect(recovery).toBeVisible();
  page.once('dialog', dialog => dialog.accept());
  await adopt.click();
  await expect(recovery).toHaveCount(0);
  await openPreview(page, id);
  await expect(preview(page)).toContainText('10.00');
  await expect(preview(page).getByRole('button', { name: 'Resume cart', exact: true })).toBeDisabled();
  await preview(page).getByLabel('I reviewed the changes. Keep the original quantities when resuming.', { exact: true }).check();
  await preview(page).getByRole('button', { name: 'Resume cart', exact: true }).click();
  await expect(page.locator('.total-row')).toContainText('10.00');
  expect((await api(request, 'get_state')).draft.lines).toEqual(lines);
});

test('ordinary autosave does not cancel a delayed repeated barcode scan or its queued successor', async ({ page, request }) => {
  await page.goto('/');
  let lookups = 0;
  let saves = 0;
  let release!: () => void;
  let started!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const received = new Promise<void>(resolve => { started = resolve; });
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (body.command === 'save_active_draft') saves += 1;
    if (body.command === 'lookup_barcode' && ++lookups === 2) {
      const response = await route.fetch();
      started();
      await gate;
      await route.fulfill({ response });
    } else await route.continue();
  });
  try {
    const barcode = page.getByLabel('Barcode', { exact: true });
    await barcode.fill('000PARKUNIT');
    await barcode.press('Enter');
    await expect(page.locator('.cart-line')).toHaveCount(1);
    await barcode.fill('000PARKUNIT');
    await barcode.press('Enter');
    await received;
    await barcode.fill('000PARKUNIT');
    await barcode.press('Enter');
    const beforeDebounce = saves;
    // Deliberately cross the production 300 ms debounce while a valid lookup
    // is still in flight. This wait tests a timer race, not network readiness.
    await page.waitForTimeout(450);
    expect(saves).toBe(beforeDebounce);
    await expect(barcode).toBeEnabled();
    release();
    await expect(page.locator('.cart-line')).toHaveCount(3);
    await expect.poll(async () => (await api(request, 'get_state')).draft.lines.length).toBe(3);
    const lines = (await api(request, 'get_state')).draft.lines as CartLine[];
    expect(new Set(lines.map(line => line.id)).size).toBe(3);
    expect(lines.map(line => ({ sku: line.sku, quantity: line.quantity }))).toEqual(Array.from({ length: 3 }, () => ({ sku: 'PARK-UNIT', quantity: { kind: 'unit', count: '1' } })));
    expect(lookups).toBe(3);
  } finally { release(); }
});

test('reload with both checkout and draft journals resolves payment first without resurrecting the sold cart', async ({ page, request }) => {
  const lines = singleLine('dual-journal-unit', '2');
  const initial = await api(request, 'get_state');
  const savePayload = { idempotency_key: randomUUID(), expected_draft_revision: initial.draft_revision, lines };
  const saved = await api(request, 'save_active_draft', savePayload);
  let ledger = await api(request, 'get_cash_ledger');
  if (!ledger.active_session) {
    await api(request, 'open_cash_session', { idempotency_key: randomUUID(), opening_float_minor: '0', expected_currency: saved.config.currency });
    ledger = await api(request, 'get_cash_ledger');
  }
  const quote = await api(request, 'quote_cart', { lines });
  const checkout = {
    idempotency_key: randomUUID(), lines, tender_minor: '1000', expected_total_minor: quote.total_minor,
    expected_config_revision: quote.config_revision, expected_currency: quote.currency,
    expected_session_id: ledger.active_session.session_id, expected_draft_id: saved.active_draft.id, expected_draft_revision: saved.draft_revision,
  };
  const before = await api(request, 'list_sales');
  await page.goto('/');
  await expect(page.locator('.cart-line')).toHaveCount(1);
  // These are exact real-service requests, representing a prior-version crash
  // with overlapping browser journals, not mocked service responses.
  await page.evaluate(({ checkout, savePayload, lines }) => {
    localStorage.setItem('gram.checkout.pending.v1', JSON.stringify(checkout));
    localStorage.setItem('gram.draft.pending.v1', JSON.stringify({ command: 'save_active_draft', payload: savePayload, source_lines: lines }));
  }, { checkout, savePayload, lines });
  await reloadRegister(page);
  await expect(page.getByRole('dialog', { name: 'Cash checkout', exact: true })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Recover saved cart action', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Retry same checkout', exact: true }).click();
  const recovery = page.getByRole('dialog', { name: 'Recover saved cart action', exact: true });
  await expect(recovery).toBeVisible();
  expect((await api(request, 'list_sales')).sales).toHaveLength(before.sales.length + 1);
  expect((await api(request, 'get_state')).active_draft).toBeNull();
  await recovery.getByRole('button', { name: 'Retry saved action', exact: true }).click();
  await expect(recovery.getByRole('button', { name: 'Use current saved cart', exact: true })).toBeEnabled();
  page.once('dialog', dialog => dialog.accept());
  await recovery.getByRole('button', { name: 'Use current saved cart', exact: true }).click();
  await expect(recovery).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Sale saved.', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await reloadRegister(page);
  await expect(page.locator('.cart-line')).toHaveCount(0);
  expect((await api(request, 'get_state')).active_draft).toBeNull();
  expect((await api(request, 'list_sales')).sales).toHaveLength(before.sales.length + 1);
  expect(await page.evaluate(() => [localStorage.getItem('gram.checkout.pending.v1'), localStorage.getItem('gram.draft.pending.v1')])).toEqual([null, null]);
});

test('same-browser tabs preserve the writer journal under contention and reconcile the losing cart', async ({ page, context, request }) => {
  const second = await context.newPage(); // Deliberately shares origin and localStorage.
  await Promise.all([page.goto('/'), second.goto('/')]);
  await expect(second.getByRole('button', { name: /Tea gift pack/ })).toBeVisible();
  let release!: () => void;
  let started!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const received = new Promise<void>(resolve => { started = resolve; });
  let held = false;
  let secondWrites = 0;
  await page.route('**/api/command', async route => {
    if (!held && route.request().postDataJSON().command === 'save_active_draft') {
      held = true;
      const response = await route.fetch();
      expect((await response.json()).ok).toBe(true);
      started();
      await gate;
      await route.fulfill({ response });
    } else await route.continue();
  });
  await second.route('**/api/command', async route => {
    if (route.request().postDataJSON().command === 'save_active_draft') secondWrites += 1;
    await route.continue();
  });
  try {
    await add(page, 'Market tote', 'Number of units', '1');
    await received;
    const journal = await page.evaluate(() => localStorage.getItem('gram.draft.pending.v1'));
    expect(journal).not.toBeNull();
    expect(JSON.parse(journal!).payload.lines[0].sku).toBe('PARK-UNIT');
    const committed = await api(request, 'get_state');
    await add(second, 'Tea gift pack', 'Number of packages', '2');
    const recovery = second.getByRole('dialog', { name: 'Recover saved cart action', exact: true });
    await expect(recovery).toBeVisible();
    await expect(recovery).toContainText(/another window/i);
    await expect(recovery).toContainText('PARK-PACK');
    expect(secondWrites).toBe(0); // Real navigator.locks rejected before any send.
    expect(await second.evaluate(() => localStorage.getItem('gram.draft.pending.v1'))).toBe(journal);
    expect(await page.evaluate(() => localStorage.getItem('gram.draft.pending.v1'))).toBe(journal);
    expect((await api(request, 'get_state')).draft).toEqual(committed.draft);
    release();
    await expect(page.getByRole('button', { name: 'Park cart', exact: true })).toBeEnabled();
    expect(await page.evaluate(() => localStorage.getItem('gram.draft.pending.v1'))).toBeNull();
    await recovery.getByRole('button', { name: 'Retry saved action', exact: true }).click();
    await expect(recovery.getByRole('button', { name: 'Use current saved cart', exact: true })).toBeEnabled();
    expect(secondWrites).toBe(1); // A deliberate retry now reaches Rust with its original stale CAS.
    expect((await api(request, 'get_state')).draft).toEqual(committed.draft);
    expect((await api(request, 'get_state')).draft_revision).toBe(committed.draft_revision);
    await expect(recovery).toContainText('PARK-PACK');
    await expect(recovery).toContainText('PARK-UNIT');
    // The confirmation belongs to the losing tab, not the first one.
    second.once('dialog', dialog => dialog.accept());
    await recovery.getByRole('button', { name: 'Use current saved cart', exact: true }).click();
    await expect(recovery).toHaveCount(0);
    await expect(second.locator('.cart-line')).toHaveCount(1);
    await expect(second.locator('.cart-line')).toContainText('Market tote');
    expect(await second.evaluate(() => localStorage.getItem('gram.draft.pending.v1'))).toBeNull();
    expect((await api(request, 'get_state')).draft).toEqual(committed.draft);
  } finally { release(); }
});
