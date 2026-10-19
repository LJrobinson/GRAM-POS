import { expect, type APIRequestContext, type Page, type Dialog } from '@playwright/test';
const randomUUID = () => crypto.randomUUID();

/** Fixture writes use the same revision guard as the real UI. This adapter only
 * upgrades historical test setup; browser requests always hit Rust unchanged. */
async function raw(request: APIRequestContext, command: string, payload: unknown = {}) {
  const response = await request.post('/api/command', { data: { command, payload } });
  const result = await response.json();
  // Domain rejections are HTTP 400, not transport failures. Callers still
  // assert success or the specific expected rejection and persisted state.
  expect(typeof result.ok).toBe('boolean');
  expect(response.status(), JSON.stringify(result)).toBe(result.ok ? 200 : 400);
  return result;
}
async function state(request: APIRequestContext) {
  const result = await raw(request, 'get_state');
  expect(result.ok, JSON.stringify(result)).toBe(true);
  return result.data;
}
export async function fixtureCommand(request: APIRequestContext, command: string, payload: unknown = {}): Promise<{ ok: boolean; data: any; error?: unknown }> {
  if (command === 'save_draft') {
    const current = await state(request);
    return raw(request, 'save_active_draft', {
      ...payload as object, idempotency_key: randomUUID(), expected_draft_revision: current.draft_revision,
    });
  }
  if (command === 'complete_cash_sale' && !('expected_draft_revision' in (payload as object))) {
    const input = payload as { lines: unknown[] };
    const saved = await fixtureCommand(request, 'save_draft', { lines: input.lines });
    expect(saved.ok, JSON.stringify(saved)).toBe(true);
    return raw(request, command, {
      ...input, expected_draft_id: saved.data.active_draft.id, expected_draft_revision: saved.data.draft_revision,
    });
  }
  return raw(request, command, payload);
}

/** Reload is deliberate in recovery tests, including a real beforeunload guard. */
export async function reloadRegister(page: Page) {
  const handle = async (dialog: Dialog) => {
    expect(dialog.type()).toBe('beforeunload');
    await dialog.accept();
  };
  page.on('dialog', handle);
  try { await page.reload(); } finally { page.off('dialog', handle); }
}
