import { test, expect, type APIRequestContext, type Page, type TestInfo } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Real Rust + SQLite only. Route interception loses a response after commit;
// it never substitutes mock application data. Run after blank-onboarding specs.
const api = async (request: APIRequestContext, command: string, payload: unknown = {}) => {
  const result = await (await request.post('/api/command', { data: { command, payload } })).json();
  expect(result.ok, JSON.stringify(result)).toBe(true);
  return result.data;
};
const header = 'format,sku,name,barcode,kind,archived,declared_content_grams,pricing_kind,group_id,unit_price_minor';
const quote = (value: string) => /[",\r\n]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value;
function row(sku: string, name: string, barcode: string, price = '425', extra: { kind?: string; grams?: string; group?: string; archived?: boolean } = {}) {
  return ['gram_catalog_v1', `text:${sku}`, `text:${name}`, barcode ? `text:${barcode}` : '', extra.kind ?? 'unit',
    String(extra.archived ?? false), extra.grams ?? '', extra.group ? 'weight_group' : 'fixed_unit',
    extra.group ? `text:${extra.group}` : '', extra.group ? '' : price].map(quote).join(',');
}
const csv = (...rows: string[]) => [header, ...rows, ''].join('\r\n');
const unit = (sku: string, barcode: string) => ({ sku, barcode, name: sku, kind: 'unit', archived: false,
  declared_content_grams: null, pricing: { kind: 'fixed_unit', unit_price_minor: '425' } });
let originalConfig: unknown;
let groupId: string;
test.beforeEach(async ({ request }) => {
  originalConfig = undefined;
  if (!(await api(request, 'get_state')).configured) await api(request, 'initialize_demo');
  await api(request, 'save_draft', { lines: [] });
  const state = await api(request, 'get_state');
  originalConfig = structuredClone(state.config);
  groupId = state.config.groups[0].id;
  state.config.catalog.push(unit('CSV-EXISTING', '000CSVEXIST'), unit('CSV-OMITTED', '000CSVOMIT'));
  await api(request, 'put_config', { config: state.config, expected_config_revision: state.config_revision });
});
test.afterEach(async ({ page, request }) => {
  await page.close(); // Stop draft effects before restoring the shared database.
  await api(request, 'save_draft', { lines: [] });
  if (originalConfig) {
    const state = await api(request, 'get_state');
    await api(request, 'put_config', { config: originalConfig, expected_config_revision: state.config_revision });
  }
});
async function open(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Configuration', exact: true }).click();
}
const review = (page: Page) => page.getByRole('heading', { name: 'Review import', exact: true });
const apply = (page: Page) => page.getByRole('button', { name: 'Apply catalog changes', exact: true });
async function preview(page: Page, content: string) {
  await page.getByLabel('Catalog CSV file', { exact: true }).setInputFiles({ name: 'catalog-training.csv', mimeType: 'text/csv', buffer: Buffer.from(content) });
  await page.getByRole('button', { name: 'Preview CSV', exact: true }).click();
  await expect(review(page)).toBeVisible();
}
async function capture(page: Page, info: TestInfo, name: string) {
  for (const [width, height, size] of [[1440, 1000, 'desktop'], [390, 844, 'phone']] as const) {
    await page.setViewportSize({ width, height });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    await review(page).scrollIntoViewIfNeeded();
    await expect(review(page)).toBeInViewport();
    const path = info.outputPath(`${name}-${size}.png`);
    await page.screenshot({ path });
    await info.attach(`${name} ${size} (training fixture)`, { path, contentType: 'image/png' });
  }
}

test('invalid CSV preview reports row errors without mutation', async ({ page, request }, info) => {
  const before = await api(request, 'get_state');
  await open(page);
  await preview(page, csv(row('CSV-INVALID', 'Invalid price', '000CSVBAD', '1.25'), row('CSV-DUPLICATE', 'Duplicate barcode', '000CSVEXIST')));
  await expect(page.getByRole('alert')).toContainText(/unit_price_minor|barcode/);
  await expect(apply(page)).toBeDisabled();
  const after = await api(request, 'get_state');
  expect(after.config).toEqual(before.config);
  expect(after.config_revision).toBe(before.config_revision);
  expect(after.draft).toEqual(before.draft);
  await capture(page, info, 'catalog-csv-invalid');
  await page.getByRole('button', { name: 'Cancel import', exact: true }).click();
  await expect(review(page)).toHaveCount(0);
});

test('valid preview cancels without writes then applies mixed products on mobile', async ({ page, request }, info) => {
  const before = await api(request, 'get_state');
  const content = csv(
    row('CSV-EXISTING', 'Updated existing unit', '000CSVEXIST', '550'),
    row('CSV-NEW', '=SUM(1,2) "café"\nsecond line', '0000012345', '9007199254740993'),
    row('CSV-PACK', 'Archived training pack', '', '700', { kind: 'prepacked', grams: '3.500', archived: true }),
    row('CSV-WEIGHT', 'Measured training product', '000CSVWEIGHT', '', { kind: 'measured', group: groupId }),
  );
  await open(page);
  await preview(page, content);
  await expect(apply(page)).toBeEnabled();
  await expect(page.getByRole('status').filter({ hasText: '3 created · 1 updated · 0 unchanged · 0 errors' })).toBeVisible();
  expect((await api(request, 'get_state')).config_revision).toBe(before.config_revision);
  const updatedRow = page.getByRole('list', { name: 'Catalog import rows' }).getByRole('listitem').filter({ hasText: 'CSV-EXISTING' });
  await updatedRow.locator('summary').click();
  await expect(updatedRow).toContainText('Saved product (before)');
  await expect(updatedRow).toContainText('Imported product (after)');
  await expect(updatedRow).toContainText('Updated existing unit');
  await capture(page, info, 'catalog-csv-valid');
  await page.getByRole('button', { name: 'Cancel import', exact: true }).click();
  expect((await api(request, 'get_state')).config).toEqual(before.config);
  await preview(page, content);
  await apply(page).scrollIntoViewIfNeeded();
  await expect(apply(page)).toBeInViewport();
  await apply(page).click();
  await expect.poll(async () => (await api(request, 'get_state')).config.catalog.length).toBe(before.config.catalog.length + 3);
  const saved = await api(request, 'get_state');
  const find = (sku: string) => saved.config.catalog.find((item: { sku: string }) => item.sku === sku);
  expect(find('CSV-NEW')).toMatchObject({ name: '=SUM(1,2) "café"\nsecond line', barcode: '0000012345', pricing: { unit_price_minor: '9007199254740993' } });
  expect(find('CSV-EXISTING')).toMatchObject({ name: 'Updated existing unit', pricing: { unit_price_minor: '550' } });
  expect(find('CSV-PACK')).toMatchObject({ kind: 'prepacked', archived: true, declared_content_grams: '3.500' });
  expect(find('CSV-WEIGHT')).toMatchObject({ kind: 'measured', pricing: { kind: 'weight_group', group_id: groupId } });
  expect(find('CSV-OMITTED')).toEqual(before.config.catalog.find((item: { sku: string }) => item.sku === 'CSV-OMITTED'));
  expect(saved.config.groups).toEqual(before.config.groups);
  expect(saved.draft).toEqual(before.draft);
  await page.reload();
  expect((await api(request, 'get_state')).config).toEqual(saved.config);
});

test('stale preview cannot overwrite another window and needs fresh review', async ({ page, request }) => {
  await open(page);
  await preview(page, csv(row('CSV-EXISTING', 'Import reviewed name', '000CSVEXIST', '600')));
  const changed = await api(request, 'get_state');
  changed.config.catalog.find((item: { sku: string }) => item.sku === 'CSV-EXISTING').name = 'Other window saved name';
  await api(request, 'put_config', { config: changed.config, expected_config_revision: changed.config_revision });
  const beforeApply = await api(request, 'get_state');
  await apply(page).click();
  await expect(page.getByRole('alert')).toBeVisible();
  expect((await api(request, 'get_state')).config).toEqual(beforeApply.config);
  expect((await api(request, 'get_state')).config_revision).toBe(beforeApply.config_revision);
  await expect(review(page)).toHaveCount(0);
  await page.getByRole('button', { name: 'Preview CSV', exact: true }).click();
  await expect(review(page)).toBeVisible();
  await apply(page).click();
  await expect.poll(async () => (await api(request, 'get_state')).config.catalog.find((item: { sku: string }) => item.sku === 'CSV-EXISTING').name).toBe('Import reviewed name');
});

test('lost apply response re-previews unchanged without duplicate products or revisions', async ({ page, request }) => {
  await open(page);
  await preview(page, csv(row('CSV-RECOVERY', 'Lost response training unit', '000CSVRECOVERY')));
  let dropped = false;
  let committedRevision: string | undefined;
  const attempts: unknown[] = [];
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (body.command === 'apply_catalog_csv') {
      attempts.push(body.payload);
      if (!dropped) {
        dropped = true;
        const result = await (await route.fetch()).json();
        expect(result.ok, JSON.stringify(result)).toBe(true);
        committedRevision = result.data.config_revision;
        await route.abort('failed');
        return;
      }
    }
    await route.continue();
  });
  await apply(page).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(review(page)).toHaveCount(0);
  expect(attempts).toHaveLength(1); // Never automatically replay an ambiguous write.
  await page.getByRole('button', { name: 'Preview CSV', exact: true }).click();
  await expect(review(page)).toBeVisible();
  const state = await api(request, 'get_state');
  expect(state.config.catalog.filter((item: { sku: string }) => item.sku === 'CSV-RECOVERY')).toHaveLength(1);
  expect(state.config_revision).toBe(committedRevision);
  await apply(page).click(); // Explicitly confirm the new no-op review.
  await expect.poll(() => attempts.length).toBe(2);
  await expect(review(page)).toHaveCount(0);
  await expect(page.getByRole('status').filter({ hasText: 'Catalog applied:' })).toBeVisible();
  const after = await api(request, 'get_state');
  expect(after.config).toEqual(state.config);
  expect(after.config_revision).toBe(committedRevision);
});

test('export download round-trips exactly as no-op and template is header-only', async ({ page, request }, info) => {
  const initial = await api(request, 'get_state');
  const item = initial.config.catalog.find((item: { sku: string }) => item.sku === 'CSV-EXISTING');
  Object.assign(item, { name: '=SUM(1,2) "café"\nsecond line', barcode: '000000000123', archived: true });
  item.pricing.unit_price_minor = '9007199254740993';
  await api(request, 'put_config', { config: initial.config, expected_config_revision: initial.config_revision });
  const before = await api(request, 'get_state');
  await open(page);
  const pending = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export catalog CSV', exact: true }).click();
  const download = await pending;
  expect(download.suggestedFilename()).toMatch(/\.csv$/);
  const path = info.outputPath('catalog-export.csv');
  await download.saveAs(path);
  expect(await download.failure()).toBeNull();
  const exported = await readFile(path, 'utf8');
  expect(exported).toContain('text:000000000123');
  expect(exported).toContain('9007199254740993');
  expect(exported).toContain('text:=SUM(1,2)');
  await preview(page, exported);
  await apply(page).click();
  await expect(review(page)).toHaveCount(0);
  await expect(page.getByRole('status').filter({ hasText: 'Catalog applied:' })).toBeVisible();
  const after = await api(request, 'get_state');
  expect(after.config).toEqual(before.config);
  expect(after.config_revision).toBe(before.config_revision);
  const templatePending = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download CSV template', exact: true }).click();
  const template = await templatePending;
  const templatePath = info.outputPath('catalog-template.csv');
  await template.saveAs(templatePath);
  expect((await readFile(templatePath, 'utf8')).trim()).toBe(header);
});


test('unsaved editor changes block import and cancelling a pending preview discards its response', async ({ page, request }) => {
  await open(page);
  const before = await api(request, 'get_state');
  const name = page.getByLabel('Product name 1', { exact: true });
  await name.fill('Unsaved local edit');
  await expect(page.getByLabel('Catalog CSV file', { exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Preview CSV', exact: true })).toBeDisabled();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Reload saved configuration', exact: true }).click();
  await expect(page.getByLabel('Catalog CSV file', { exact: true })).toBeEnabled();
  let release!: () => void;
  let started!: () => void;
  let finished!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const received = new Promise<void>(resolve => { started = resolve; });
  const fulfilled = new Promise<void>(resolve => { finished = resolve; });
  let delayed = false;
  await page.route('**/api/command', async route => {
    if (!delayed && route.request().postDataJSON().command === 'preview_catalog_csv') {
      delayed = true;
      const response = await route.fetch();
      started();
      await gate;
      await route.fulfill({ response });
      finished();
    } else await route.continue();
  });
  const content = csv(row('CSV-CANCELLED', 'Cancelled preview', '000CSVCANCEL'));
  await page.getByLabel('Catalog CSV file', { exact: true }).setInputFiles({ name: 'cancel.csv', mimeType: 'text/csv', buffer: Buffer.from(content) });
  await page.getByRole('button', { name: 'Preview CSV', exact: true }).click();
  await received;
  await page.getByRole('button', { name: 'Cancel import', exact: true }).click();
  release();
  await fulfilled;
  // A fresh successful preview provides a causal barrier after the late result.
  await preview(page, csv(row('CSV-FRESH', 'Fresh preview', '000CSVFRESH')));
  await expect(page.getByRole('list', { name: 'Catalog import rows' })).toContainText('CSV-FRESH');
  await expect(page.getByRole('list', { name: 'Catalog import rows' })).not.toContainText('CSV-CANCELLED');
  expect((await api(request, 'get_state')).config).toEqual(before.config);
  expect((await api(request, 'get_state')).config_revision).toBe(before.config_revision);
});

test('uncertain apply and failed refresh cannot silently discard later local edits', async ({ page, request }) => {
  await open(page);
  await preview(page, csv(row('CSV-UNCERTAIN', 'Committed before connection loss', '000CSVUNCERTAIN')));
  let committed = false;
  let refreshFailed = false;
  let refreshRequests = 0;
  await page.route('**/api/command', async route => {
    const body = route.request().postDataJSON();
    if (!committed && body.command === 'apply_catalog_csv') {
      const result = await (await route.fetch()).json();
      expect(result.ok, JSON.stringify(result)).toBe(true);
      committed = true;
      await route.abort('failed');
      return;
    }
    if (committed && body.command === 'get_state') {
      refreshRequests += 1;
      if (!refreshFailed) {
        refreshFailed = true;
        await route.abort('failed');
        return;
      }
    }
    await route.continue();
  });
  await apply(page).click();
  const refresh = page.getByRole('button', { name: 'Refresh saved catalog', exact: true });
  await expect(refresh).toBeEnabled();
  expect(refreshFailed).toBe(true);
  expect(refreshRequests).toBe(1);
  const name = page.getByLabel('Product name 1', { exact: true });
  await name.fill('Keep this unsaved local name');
  await expect(refresh).toBeDisabled();
  await expect(name).toHaveValue('Keep this unsaved local name');
  const saved = await api(request, 'get_state');
  expect(saved.config.catalog.filter((item: { sku: string }) => item.sku === 'CSV-UNCERTAIN')).toHaveLength(1);
  expect(saved.config.catalog[0].name).not.toBe('Keep this unsaved local name');
  expect(refreshRequests).toBe(1); // No silent second refresh after editing.
  // Only an explicit discard/reload may replace this local edit.
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Reload saved configuration', exact: true }).click();
  await expect(name).toHaveValue(saved.config.catalog[0].name);
});
