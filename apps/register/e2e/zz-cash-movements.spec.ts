import { test, expect, type APIRequestContext, type Locator, type Page, type TestInfo } from '@playwright/test';
import type { CartLine, Config, Currency } from '../src/types';
import { fixtureCommand, reloadRegister } from './fixture-command';

// Synthetic training data, backed by real Rust commands and SQLite. Interception
// only holds or loses actual service responses, never fabricates financial data.
// Browser and store zones deliberately disagree to expose host-timezone leakage.
test.use({ timezoneId: 'Pacific/Honolulu', locale: 'en-US' });
const currency: Currency = { code: 'USD', minor_unit_exponent: 2 };
const storeTimezone = 'Asia/Tokyo';
type MovementKind = 'paid_in' | 'paid_out';
type Movement = { movement_id: string; created_at_unix: string; session_id: string; kind: MovementKind; amount_minor: string; reason: string; currency: Currency };
type Session = { session_id: string; currency: Currency; expected_cash_minor: string; movement_count: string; closed_at_unix: string | null };
const api = async (request: APIRequestContext, name: string, payload: unknown = {}) => {
  const result = await fixtureCommand(request, name, payload);
  expect(result.ok, JSON.stringify(result)).toBe(true);
  return result.data;
};
const ledger = (request: APIRequestContext) => api(request, 'get_cash_ledger');
const report = (request: APIRequestContext) => api(request, 'sales_report', { start_date: '2000-01-01', end_date: '2099-01-01', timezone: storeTimezone });
const sessionMovements = (records: { cash_movements: Movement[] }, session: Session): Movement[] =>
  records.cash_movements.filter(movement => movement.session_id === session.session_id);
const nextCommandResponse = (page: Page, command: string) => page.waitForResponse(response =>
  response.url().endsWith('/api/command') && response.request().postDataJSON()?.command === command);
const movementPayload = (session: Session, kind: MovementKind, amount = '1250', reason = 'Training change replenishment') => ({
  idempotency_key: crypto.randomUUID(), kind, amount_minor: amount, reason,
  expected_session_id: session.session_id, expected_currency: session.currency,
});
async function closeSession(request: APIRequestContext, session: Session) {
  return api(request, 'close_cash_session', { session_id: session.session_id,
    counted_minor: BigInt(session.expected_cash_minor) < 0n ? '0' : session.expected_cash_minor,
    expected_movement_count: session.movement_count });
}
let originalConfig: Config | undefined;
let session: Session;
test.beforeEach(async ({ request }) => {
  originalConfig = undefined;
  if (!(await api(request, 'get_state')).configured) await api(request, 'initialize_demo');
  await api(request, 'save_draft', { lines: [] });
  const oldSession = (await ledger(request)).active_session;
  if (oldSession) await closeSession(request, oldSession);
  const state = await api(request, 'get_state');
  originalConfig = structuredClone(state.config);
  const config: Config = { ...state.config, currency,
    store_settings: { display_name: 'Cedar Market', receipt_header: 'Cash drawer training workspace', receipt_contact: 'example.invalid', receipt_footer: 'Training only · no live payment', timezone: storeTimezone },
    catalog: [{ sku: 'CASH-TRAINING-UNIT', name: 'Market tote (training)', kind: 'unit', archived: false,
      declared_content_grams: null, pricing: { kind: 'fixed_unit', unit_price_minor: '500' } }],
  };
  await api(request, 'put_config', { config, expected_config_revision: state.config_revision });
  session = await api(request, 'open_cash_session', { idempotency_key: crypto.randomUUID(), opening_float_minor: '5000', expected_currency: currency });
});
test.afterEach(async ({ context, request }) => {
  // Stop every window's effects before restoring the serial test database.
  for (const page of context.pages()) await page.close();
  await api(request, 'save_draft', { lines: [] });
  const active = (await ledger(request)).active_session;
  if (active) await closeSession(request, active);
  if (originalConfig) {
    const state = await api(request, 'get_state');
    await api(request, 'put_config', { config: originalConfig, expected_config_revision: state.config_revision });
  }
});
async function openCash(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Cash sessions', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Open drawer session', exact: true })).toBeVisible();
}
const label = (kind: MovementKind) => kind === 'paid_in' ? 'paid in' : 'paid out';
const dialog = (page: Page, kind: MovementKind) => page.getByRole('dialog', { name: `Record ${label(kind)}`, exact: true });
async function reviewMovement(page: Page, kind: MovementKind, amount: string, reason: string) {
  await page.getByRole('button', { name: kind === 'paid_in' ? 'Paid in' : 'Paid out', exact: true }).click();
  const form = dialog(page, kind);
  await form.getByLabel('Amount (USD)', { exact: true }).fill(amount);
  await form.getByLabel('Reason', { exact: true }).fill(reason);
  await form.getByRole('button', { name: `Review ${label(kind)}`, exact: true }).click();
  await expect(form.getByRole('button', { name: `Confirm ${label(kind)}`, exact: true })).toBeEnabled();
  return form;
}
async function confirmMovement(page: Page, kind: MovementKind) {
  await dialog(page, kind).getByRole('button', { name: `Confirm ${label(kind)}`, exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
}
async function sale(request: APIRequestContext, count: string) {
  const lines: CartLine[] = [{ id: crypto.randomUUID(), sku: 'CASH-TRAINING-UNIT', quantity: { kind: 'unit', count } }];
  const quote = await api(request, 'quote_cart', { lines });
  return api(request, 'complete_cash_sale', { idempotency_key: crypto.randomUUID(), lines,
    tender_minor: quote.total_minor, expected_total_minor: quote.total_minor,
    expected_config_revision: quote.config_revision, expected_session_id: session.session_id, expected_currency: currency });
}
async function capture(page: Page, info: TestInfo, name: string, focus: Locator) {
  for (const [width, height, size] of [[1440, 1000, 'desktop'], [390, 844, 'phone']] as const) {
    await page.setViewportSize({ width, height });
    await focus.scrollIntoViewIfNeeded();
    await expect(focus).toBeInViewport();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    const path = info.outputPath(`${name}-${size}.png`);
    await page.screenshot({ path }); // True viewport capture, not full-page dialog resizing.
    await info.attach(`${name} ${size} (synthetic training fixture)`, { path, contentType: 'image/png' });
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
}

test('paid-in requires amount and reason, review and Cancel never write', async ({ page, request }, info) => {
  const before = await ledger(request);
  const salesBefore = await api(request, 'list_sales');
  const reportBefore = await report(request);
  const writes: unknown[] = [];
  page.on('request', request => {
    if (request.url().endsWith('/api/command') && request.postDataJSON()?.command === 'record_cash_movement') writes.push(request.postDataJSON());
  });
  await openCash(page);
  await page.getByRole('button', { name: 'Paid in', exact: true }).click();
  const form = dialog(page, 'paid_in');
  const review = form.getByRole('button', { name: 'Review paid in', exact: true });
  await expect(review).toBeDisabled();
  await form.getByLabel('Amount (USD)', { exact: true }).fill('12.50');
  await expect(review).toBeDisabled();
  await form.getByLabel('Reason', { exact: true }).fill('   ');
  await expect(review).toBeDisabled();
  await form.getByLabel('Reason', { exact: true }).fill('Training change replenishment');
  await review.click();
  await expect(form.getByLabel('Amount (USD)', { exact: true })).toBeDisabled();
  await expect(form.getByLabel('Reason', { exact: true })).toBeDisabled();
  await expect(form).toContainText('$12.50');
  await expect(form).toContainText('Training change replenishment');
  expect(await ledger(request)).toEqual(before);
  await capture(page, info, 'cash-paid-in-review', form.getByRole('button', { name: 'Confirm paid in', exact: true }));
  await form.getByRole('button', { name: 'Edit details', exact: true }).click();
  await form.getByLabel('Amount (USD)', { exact: true }).fill('15.00');
  await expect(form.getByRole('button', { name: 'Confirm paid in', exact: true })).toHaveCount(0);
  await review.click();
  await form.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await reloadRegister(page);
  expect(await ledger(request)).toEqual(before);
  expect(await api(request, 'list_sales')).toEqual(salesBefore);
  expect(await report(request)).toEqual(reportBefore);
  expect(await page.evaluate(() => localStorage.getItem('gram.ledger.pending.v1'))).toBeNull();
  expect(writes).toEqual([]);
});

test('drawer conserves float plus sales minus refunds plus paid-in minus paid-out, separate from revenue', async ({ page, request }, info) => {
  const retained = await sale(request, '4');
  const returned = await sale(request, '1');
  const refund = await api(request, 'refund_cash_sale', { idempotency_key: crypto.randomUUID(), sale_id: returned.sale_id,
    reason: 'Training customer return', expected_session_id: session.session_id, expected_currency: currency });
  expect(refund.amount_minor).toBe('500');
  const reportBefore = await report(request);
  const salesBefore = await api(request, 'list_sales');
  const stateBefore = await api(request, 'get_state');
  await openCash(page);
  await reviewMovement(page, 'paid_in', '12.50', 'Training change replenishment');
  await confirmMovement(page, 'paid_in');
  const paidOut = await reviewMovement(page, 'paid_out', '10.00', 'Training petty cash transfer');
  await capture(page, info, 'cash-paid-out-review', paidOut.getByRole('button', { name: 'Confirm paid out', exact: true }));
  await confirmMovement(page, 'paid_out');
  const saved = await ledger(request);
  expect(saved.active_session).toMatchObject({ session_id: session.session_id, opening_float_minor: '5000', sales_total_minor: '2500',
    refunds_total_minor: '500', paid_in_total_minor: '1250', paid_out_total_minor: '1000', expected_cash_minor: '7250', movement_count: '5' });
  const movements = sessionMovements(saved, session);
  expect(movements).toHaveLength(2);
  expect(movements.map(movement => movement.kind)).toEqual(['paid_out', 'paid_in']);
  expect(await report(request)).toEqual(reportBefore);
  expect(await api(request, 'list_sales')).toEqual(salesBefore);
  expect(await api(request, 'get_receipt', { sale_id: retained.sale_id })).toEqual(retained);
  expect(await api(request, 'get_receipt', { sale_id: returned.sale_id })).toEqual(returned);
  const stateAfter = await api(request, 'get_state');
  expect(stateAfter.config).toEqual(stateBefore.config);
  expect(stateAfter.draft).toEqual(stateBefore.draft);
  expect(stateAfter.draft_revision).toBe(stateBefore.draft_revision);
  await page.getByRole('button', { name: 'Count and close session', exact: true }).click();
  const closing = page.getByRole('dialog', { name: 'Close cash session', exact: true });
  await closing.getByLabel('Counted cash (USD)', { exact: true }).fill('72.50');
  await closing.getByRole('button', { name: 'Review closing count', exact: true }).click();
  await expect(closing).toContainText('$72.50');
  await expect(closing).toContainText('$12.50');
  await expect(closing).toContainText('$10.00');
  await expect(closing.locator('.closing-variance')).toContainText('$0.00');
  await capture(page, info, 'cash-movement-closing', closing.getByRole('button', { name: 'Confirm and close session', exact: true }));
  await closing.getByRole('button', { name: 'Confirm and close session', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'No open cash session', exact: true })).toBeVisible();
  const closed = (await ledger(request)).sessions.find((item: Session) => item.session_id === session.session_id);
  expect(closed).toMatchObject({ expected_cash_minor: '7250', counted_minor: '7250', variance_minor: '0', paid_in_total_minor: '1250', paid_out_total_minor: '1000' });
  await reloadRegister(page);
  await page.getByRole('button', { name: 'Cash sessions', exact: true }).click();
  for (const movement of movements) {
    const row = page.getByTestId('cash-movement').filter({ hasText: movement.reason });
    await expect(row).toContainText(storeTimezone);
    await expect(row.locator('dl > div').filter({ has: page.getByText('Movement ID', { exact: true }) }).locator('dd')).toHaveText(movement.movement_id);
    await expect(row.locator('dl > div').filter({ has: page.getByText('Session ID', { exact: true }) }).locator('dd')).toHaveText(movement.session_id);
    await expect(row.locator('time')).toHaveAttribute('datetime', new Date(Number(movement.created_at_unix) * 1000).toISOString());
    // Tokyo is UTC+09:00 year-round. Calculate independently of the app's formatter
    // so a regression to the browser's Honolulu timezone cannot validate itself.
    const tokyoTime = new Date(Number(movement.created_at_unix) * 1000 + 9 * 60 * 60 * 1000).toISOString().slice(0, 19).replace('T', ' ');
    await expect(row.locator('time')).toHaveText(`${tokyoTime} GMT+09:00 [${storeTimezone}]`);
  }
  await capture(page, info, 'cash-movement-history', page.getByRole('heading', { name: 'Cash movements', exact: true }));
  await info.attach('Cash conservation training fixture', { body: 'Synthetic Cedar Market data. USD 50.00 opening + 25.00 sales − 5.00 refund + 12.50 paid in − 10.00 paid out = 72.50 counted, zero variance. Cash movements do not change revenue, receipt snapshots, catalog, stock, or taxes. Browser zone Pacific/Honolulu; configured store zone Asia/Tokyo.', contentType: 'text/plain' });
});

test('repeated confirm clicks produce one immutable movement while the real response is delayed', async ({ page, request }) => {
  await openCash(page);
  await reviewMovement(page, 'paid_out', '8.00', 'Training safe transfer');
  let release!: () => void;
  let fetched!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; });
  const started = new Promise<void>(resolve => { fetched = resolve; });
  const attempts: Record<string, unknown>[] = [];
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (body.command === 'record_cash_movement') {
      attempts.push(body.payload);
      const response = await route.fetch();
      expect((await response.json()).ok).toBe(true);
      fetched();
      await held;
      await route.fulfill({ response });
    } else await route.continue();
  });
  try {
    await dialog(page, 'paid_out').getByRole('button', { name: 'Confirm paid out', exact: true }).click({ clickCount: 3 });
    await started;
    await expect(page.getByRole('button', { name: 'Saving cash record…', exact: true })).toBeDisabled();
    expect(attempts).toHaveLength(1);
    const saved = await ledger(request);
    expect(sessionMovements(saved, session)).toHaveLength(1);
    expect(saved.active_session).toMatchObject({ expected_cash_minor: '4200', paid_out_total_minor: '800', movement_count: '1' });
  } finally { release(); }
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const original = sessionMovements(await ledger(request), session)[0];
  expect(await api(request, 'record_cash_movement', attempts[0])).toEqual(original);
  expect(await api(request, 'get_cash_movement_by_key', { idempotency_key: attempts[0].idempotency_key })).toEqual({ cash_movement: original });
  const conflict = await fixtureCommand(request, 'record_cash_movement', { ...attempts[0], amount_minor: '801' });
  expect(conflict).toMatchObject({ ok: false, error: { code: 'IDEMPOTENCY_CONFLICT' } });
  expect(sessionMovements(await ledger(request), session)).toEqual([original]);
});

for (const kind of ['paid_in', 'paid_out'] as const) {
  test(`${label(kind)} lost response and reload retry the identical durable request and key`, async ({ page, request }) => {
    await openCash(page);
    await reviewMovement(page, kind, '7.25', `Training ${label(kind)} recovery`);
    let dropped = false;
    const attempts: Record<string, unknown>[] = [];
    await page.route('**/api/command', async route => {
      const body = route.request().postDataJSON();
      if (body.command === 'record_cash_movement') {
        attempts.push(body.payload);
        if (!dropped) {
          dropped = true;
          const response = await route.fetch();
          expect((await response.json()).ok).toBe(true);
          await route.abort('failed');
          return;
        }
      }
      await route.continue();
    });
    await dialog(page, kind).getByRole('button', { name: `Confirm ${label(kind)}`, exact: true }).click({ clickCount: 2 });
    await expect(page.getByRole('button', { name: 'Retry same cash action', exact: true })).toBeEnabled();
    expect(attempts).toHaveLength(1);
    const committed = sessionMovements(await ledger(request), session);
    expect(committed).toHaveLength(1);
    const pending = await page.evaluate(() => JSON.parse(localStorage.getItem('gram.ledger.pending.v1')!));
    expect(pending).toMatchObject({ command: 'record_cash_movement', payload: attempts[0] });
    await expect(page.getByRole('button', { name: 'Discard rejected cash action', exact: true })).toHaveCount(0);
    await reloadRegister(page);
    const recovery = page.getByRole('dialog', { name: 'Recover cash action', exact: true });
    await expect(recovery).toContainText(`Training ${label(kind)} recovery`);
    await expect(recovery).toContainText('$7.25');
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem('gram.ledger.pending.v1')!))).toEqual(pending);
    await recovery.getByRole('button', { name: 'Retry same cash action', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    expect(attempts).toHaveLength(2);
    expect(attempts[1]).toEqual(attempts[0]);
    const saved = await ledger(request);
    expect(sessionMovements(saved, session)).toEqual(committed);
    expect(saved.active_session).toMatchObject({ expected_cash_minor: kind === 'paid_in' ? '5725' : '4275', movement_count: '1' });
    expect(await page.evaluate(() => localStorage.getItem('gram.ledger.pending.v1'))).toBeNull();
  });
}

test('movement reviewed before the session closes is rejected without changing its immutable close', async ({ page, request }) => {
  await openCash(page);
  await reviewMovement(page, 'paid_out', '10.00', 'Training stale session rejection');
  const closed = await closeSession(request, (await ledger(request)).active_session);
  const before = await ledger(request);
  const rejected = nextCommandResponse(page, 'record_cash_movement');
  await dialog(page, 'paid_out').getByRole('button', { name: 'Confirm paid out', exact: true }).click();
  expect(await (await rejected).json()).toMatchObject({ ok: false, error: { code: 'NO_OPEN_SESSION' } });
  await expect(page.getByRole('dialog').getByRole('alert')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Discard rejected cash action', exact: true })).toBeEnabled();
  expect(await ledger(request)).toEqual(before);
  expect(sessionMovements(before, session)).toEqual([]);
  expect(before.sessions.find((item: Session) => item.session_id === session.session_id)).toEqual(closed);
  await page.getByRole('button', { name: 'Discard rejected cash action', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'No open cash session', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Paid out', exact: true })).toHaveCount(0);
});

test('other-window movement invalidates a close review until refresh and recount', async ({ page, context, request }) => {
  await openCash(page);
  await page.getByRole('button', { name: 'Count and close session', exact: true }).click();
  let closing = page.getByRole('dialog', { name: 'Close cash session', exact: true });
  await closing.getByLabel('Counted cash (USD)', { exact: true }).fill('50.00');
  await closing.getByRole('button', { name: 'Review closing count', exact: true }).click();
  const other = await context.newPage();
  await openCash(other);
  await reviewMovement(other, 'paid_in', '5.00', 'Training other-window float top-up');
  await confirmMovement(other, 'paid_in');
  const rejected = nextCommandResponse(page, 'close_cash_session');
  await closing.getByRole('button', { name: 'Confirm and close session', exact: true }).click();
  expect(await (await rejected).json()).toMatchObject({ ok: false, error: { code: 'STALE_SESSION' } });
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText(/changed|refresh|recount/i);
  let saved = await ledger(request);
  expect(saved.active_session).toMatchObject({ session_id: session.session_id, expected_cash_minor: '5500', movement_count: '1', closed_at_unix: null });
  await page.getByRole('button', { name: 'Discard rejected cash action', exact: true }).click();
  await page.getByRole('button', { name: 'Refresh cash records', exact: true }).click();
  await expect(page.locator('.ledger-totals').first()).toContainText('$55.00');
  await page.getByRole('button', { name: 'Count and close session', exact: true }).click();
  closing = page.getByRole('dialog', { name: 'Close cash session', exact: true });
  await closing.getByLabel('Counted cash (USD)', { exact: true }).fill('55.00');
  await closing.getByRole('button', { name: 'Review closing count', exact: true }).click();
  await expect(closing.locator('.closing-variance')).toContainText('$0.00');
  await closing.getByRole('button', { name: 'Confirm and close session', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'No open cash session', exact: true })).toBeVisible();
  saved = await ledger(request);
  expect(saved.active_session).toBeNull();
  expect(saved.sessions.find((item: Session) => item.session_id === session.session_id)).toMatchObject({ expected_cash_minor: '5500', counted_minor: '5500', variance_minor: '0' });
});

test('precision, currency, reasons, and overdraw reject without writes; paid-out may exactly empty the drawer', async ({ page, request }) => {
  await openCash(page);
  await page.getByRole('button', { name: 'Paid out', exact: true }).click();
  const form = dialog(page, 'paid_out');
  const amount = form.getByLabel('Amount (USD)', { exact: true });
  const review = form.getByRole('button', { name: 'Review paid out', exact: true });
  await form.getByLabel('Reason', { exact: true }).fill('Training validation boundary');
  for (const value of ['0', '-1', '1.001', '1e2', '1,000.00', '184467440737095516.16']) {
    await amount.fill(value);
    await expect(review, `Reject invalid USD amount: ${value}`).toBeDisabled();
  }
  await amount.fill('50.01');
  await expect(review).toBeDisabled();
  await expect(form.getByRole('alert')).toContainText(/expected cash/i);
  await form.getByRole('button', { name: 'Cancel', exact: true }).click();
  const before = await ledger(request);
  const base = movementPayload(session, 'paid_out', '100', 'Training validation boundary');
  const invalid: { patch: object; code: string }[] = [
    ...['0', '-1', '1.01', '18446744073709551616'].map(amount_minor => ({ patch: { amount_minor }, code: 'INVALID_INPUT' })),
    ...['', '   ', 'Training\ncontrol', 'é'.repeat(251)].map(reason => ({ patch: { reason }, code: 'INVALID_INPUT' })),
    { patch: { expected_currency: { code: 'EUR', minor_unit_exponent: 2 } }, code: 'SESSION_CURRENCY' },
    { patch: { expected_currency: { code: 'USD', minor_unit_exponent: 3 } }, code: 'SESSION_CURRENCY' },
    { patch: { expected_session_id: '999999999' }, code: 'SESSION_CHANGED' },
    { patch: { amount_minor: '5001' }, code: 'INSUFFICIENT_CASH' },
  ];
  for (const { patch, code } of invalid) {
    const input = { ...base, ...patch, idempotency_key: crypto.randomUUID() };
    const result = await fixtureCommand(request, 'record_cash_movement', input);
    expect(result, JSON.stringify(input)).toMatchObject({ ok: false, error: { code } });
    expect(await api(request, 'get_cash_movement_by_key', { idempotency_key: input.idempotency_key })).toEqual({ cash_movement: null });
    expect(await ledger(request)).toEqual(before);
  }
  // Another window can consume cash after review. The service must recheck the
  // live total in its transaction, rather than trust the browser's old balance.
  await reviewMovement(page, 'paid_out', '40.00', 'Training stale balance rejection');
  await api(request, 'record_cash_movement', movementPayload(session, 'paid_out', '2000', 'Training concurrent transfer'));
  const rejected = nextCommandResponse(page, 'record_cash_movement');
  await dialog(page, 'paid_out').getByRole('button', { name: 'Confirm paid out', exact: true }).click();
  expect(await (await rejected).json()).toMatchObject({ ok: false, error: { code: 'INSUFFICIENT_CASH' } });
  await expect(page.getByRole('dialog').getByRole('alert')).toBeVisible();
  await page.getByRole('button', { name: 'Discard rejected cash action', exact: true }).click();
  expect(sessionMovements(await ledger(request), session)).toHaveLength(1);
  await api(request, 'record_cash_movement', movementPayload(session, 'paid_out', '3000', 'Training exact empty drawer'));
  const emptied = await ledger(request);
  expect(emptied.active_session).toMatchObject({ expected_cash_minor: '0', paid_out_total_minor: '5000', movement_count: '2' });
  const overdraw = await fixtureCommand(request, 'record_cash_movement', movementPayload(session, 'paid_out', '1', 'Training one-cent overdraw'));
  expect(overdraw).toMatchObject({ ok: false, error: { code: 'INSUFFICIENT_CASH' } });
  expect(await ledger(request)).toEqual(emptied);
});

test('malformed cash recovery data blocks new writes without crashing or silently clearing it', async ({ page, request }) => {
  const pageErrors: string[] = [];
  const movementRequests: unknown[] = [];
  page.on('pageerror', error => pageErrors.push(error.message));
  page.on('request', req => { if (req.url().endsWith('/api/command') && req.postDataJSON()?.command === 'record_cash_movement') movementRequests.push(req.postDataJSON()); });
  await page.addInitScript(() => localStorage.setItem('gram.ledger.pending.v1', '{broken-cash-recovery'));
  await page.goto('/');
  const recovery = page.getByRole('dialog', { name: 'Cash recovery required', exact: true });
  await expect(recovery.getByRole('alert')).toContainText('cannot be read');
  await expect(recovery).toContainText('Keep browser data intact');
  await expect(recovery.getByRole('button')).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Charge cash/ })).toBeDisabled();
  expect(await page.evaluate(() => localStorage.getItem('gram.ledger.pending.v1'))).toBe('{broken-cash-recovery');
  expect(sessionMovements(await ledger(request), session)).toEqual([]);
  expect(movementRequests).toEqual([]);
  expect(pageErrors).toEqual([]);
});

test('another window cannot replace an uncertain movement, and stale recovery cannot resurrect it', async ({ page, context, request }) => {
  const other = await context.newPage();
  await openCash(page);
  await openCash(other);
  await reviewMovement(page, 'paid_in', '4.25', 'Training shared recovery identity');
  await reviewMovement(other, 'paid_in', '9.00', 'Training unsent competing action');
  const attempts: Record<string, unknown>[] = [];
  context.on('request', req => { if (req.url().endsWith('/api/command') && req.postDataJSON()?.command === 'record_cash_movement') attempts.push(req.postDataJSON().payload); });
  let dropped = false;
  await page.route('**/api/command', async route => {
    if (route.request().postDataJSON()?.command === 'record_cash_movement' && !dropped) {
      dropped = true;
      const response = await route.fetch();
      expect((await response.json()).ok).toBe(true);
      await route.abort('failed');
    } else await route.continue();
  });
  await dialog(page, 'paid_in').getByRole('button', { name: 'Confirm paid in', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry same cash action', exact: true })).toBeEnabled();
  const originalJournal = await page.evaluate(() => localStorage.getItem('gram.ledger.pending.v1'));
  expect(originalJournal).not.toBeNull();
  const original = sessionMovements(await ledger(request), session)[0];
  expect(original.amount_minor).toBe('425');
  await dialog(other, 'paid_in').getByRole('button', { name: 'Confirm paid in', exact: true }).click();
  await expect(dialog(other, 'paid_in').getByRole('alert')).toContainText('Another window has an unresolved cash action');
  expect(attempts).toHaveLength(1);
  expect(await other.evaluate(() => localStorage.getItem('gram.ledger.pending.v1'))).toBe(originalJournal);
  await dialog(other, 'paid_in').getByRole('button', { name: 'Cancel', exact: true }).click();
  await reloadRegister(other);
  await other.getByRole('dialog', { name: 'Recover cash action', exact: true }).getByRole('button', { name: 'Retry same cash action', exact: true }).click();
  await expect(other.getByRole('dialog')).toHaveCount(0);
  expect(await other.evaluate(() => localStorage.getItem('gram.ledger.pending.v1'))).toBeNull();
  // An old window must not resurrect a request whose journal another window
  // has completed or deliberately discarded. Reload adopts authoritative state.
  await page.getByRole('button', { name: 'Retry same cash action', exact: true }).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('resolved in another window');
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);
  await reloadRegister(page);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const saved = await ledger(request);
  expect(sessionMovements(saved, session)).toEqual([original]);
  expect(saved.active_session).toMatchObject({ expected_cash_minor: '5425', paid_in_total_minor: '425', movement_count: '1' });
  expect(await page.evaluate(() => localStorage.getItem('gram.ledger.pending.v1'))).toBeNull();
});
