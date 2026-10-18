import { test, expect } from '@playwright/test';

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
