import test from 'node:test';
import assert from 'node:assert/strict';
import { DRAFT_PENDING_KEY, DraftWorkspace, readDraftPending, sameLines, validDraftLines, type DraftStorage, type PendingDraft } from '../src/drafts.ts';
import type { CartLine, RegisterState } from '../src/types.ts';
const measured: CartLine = { id: 'm', sku: 'OLIVES', quantity: { kind: 'measured', grams: '125.250' } };
const pack: CartLine = { id: 'p', sku: 'PACK', quantity: { kind: 'prepacked', count: '2' } };
const unit: CartLine = { id: 'u', sku: 'SOAP', quantity: { kind: 'unit', count: '3' } };
const state = (revision = '0', lines: CartLine[] = [], id: string | null = null): RegisterState => ({ configured: true, config_revision: '4', config: null, catalog: [], draft: { lines }, draft_revision: revision, active_draft: id ? { id, revision, issues: [] } : null, parked_drafts: [] });
function storage() { const data = new Map<string, string>(); return { data, getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); }, removeItem: (key: string) => { data.delete(key); } }; }
function error(code: string) { return Object.assign(new Error(code), { code }); }
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: Error) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
const pending: PendingDraft = { command: 'save_active_draft', payload: { idempotency_key: 'op-1', expected_draft_revision: '0', lines: [measured, pack, unit] }, source_lines: [measured, pack, unit] };
test('pending requests retain exact keys, revisions and original measured/count semantics', () => {
  const local = storage(); local.setItem(DRAFT_PENDING_KEY, JSON.stringify(pending));
  assert.deepEqual(readDraftPending(local), pending);
  assert.equal(validDraftLines(pending.source_lines), true);
  assert.equal(sameLines([measured], [{ ...measured, quantity: { kind: 'measured', grams: '125.25' } }]), false);
});
test('corrupt or unknown recovery data fails closed without deleting it', () => {
  for (const raw of ['{bad', '{}', JSON.stringify({ ...pending, command: 'unknown' }), JSON.stringify({ ...pending, payload: { ...pending.payload, lines: [{ ...measured, quantity: { kind: 'unit', grams: '1' } }] } })]) {
    const local = storage(); local.setItem(DRAFT_PENDING_KEY, raw);
    assert.throws(() => readDraftPending(local));
    const workspace = new DraftWorkspace(async <T>() => state() as T, local, () => 'key'); workspace.initialize(state());
    assert.equal(workspace.blocked, true); assert.equal(workspace.change([unit]), false); assert.equal(local.getItem(DRAFT_PENDING_KEY), raw);
  }
});
test('the exact journal is durable before send, and mutations lock synchronously', async () => {
  const local = storage(); const response = deferred<RegisterState>(); let calls = 0;
  const workspace = new DraftWorkspace(async <T>(name: string, payload?: unknown) => { calls++; assert.equal(name, 'save_active_draft'); assert.deepEqual(readDraftPending(local)?.payload, payload); return await response.promise as T; }, local, () => 'op-1');
  workspace.initialize(state()); workspace.change([measured]); const task = workspace.flush();
  assert.equal(calls, 1); assert.equal(workspace.blocked, true); assert.equal(workspace.change([unit]), false);
  response.resolve(state('1', [measured], 'draft-1')); await task;
  assert.equal(workspace.blocked, false); assert.equal(local.getItem(DRAFT_PENDING_KEY), null); assert.equal(workspace.view.state?.active_draft?.id, 'draft-1');
});
test('a storage failure never sends and never removes local lines', async () => {
  let calls = 0; const local: DraftStorage = { getItem: () => null, setItem: () => { throw new Error('quota'); }, removeItem: () => {} };
  const workspace = new DraftWorkspace(async <T>() => { calls++; return state() as T; }, local, () => 'key'); workspace.initialize(state()); workspace.change([measured]);
  await assert.rejects(workspace.flush(), /No cart request was sent/); assert.equal(calls, 0); assert.deepEqual(workspace.view.lines, [measured]); assert.equal(workspace.blocked, true);
});
test('unknown save failure survives reload and retries the exact request without rebasing', async () => {
  const local = storage(); const sent: unknown[] = [];
  const original = new DraftWorkspace(async <T>(_name: string, payload?: unknown) => { sent.push(payload); throw new Error('lost response'); }, local, () => 'original-key');
  original.initialize(state()); original.change([measured, pack]); await assert.rejects(original.flush());
  assert.equal(original.view.conflict, null); assert.equal(original.blocked, true);
  const recovered = new DraftWorkspace(async <T>(_name: string, payload?: unknown) => { sent.push(payload); return state('1', [measured, pack], 'retained-id') as T; }, local, () => 'must-not-be-used');
  recovered.initialize(state('1', [measured, pack], 'retained-id')); await recovered.retry();
  assert.deepEqual(sent[1], sent[0]); assert.equal(recovered.blocked, false); assert.equal(recovered.view.state?.active_draft?.id, 'retained-id');
});
test('park flushes the latest cart first, and captures the resulting CAS revision', async () => {
  const local = storage(); const calls: { name: string; payload: any }[] = []; let keys = 0;
  const workspace = new DraftWorkspace(async <T>(name: string, payload?: unknown) => { calls.push({ name, payload }); return (name === 'save_active_draft' ? state('10', [measured, pack], 'draft-1') : { ...state('11'), parked_drafts: [{ id: 'draft-1', label: 'Counter 2', parked_at_unix: '123', line_count: 2 }] }) as T; }, local, () => `key-${++keys}`);
  workspace.initialize(state('9')); workspace.change([measured, pack]); await workspace.mutate({ command: 'park_draft', label: 'Counter 2' });
  assert.deepEqual(calls.map(call => call.name), ['save_active_draft', 'park_draft']); assert.equal(calls[0].payload.expected_draft_revision, '9'); assert.equal(calls[1].payload.expected_draft_revision, '10');
  assert.deepEqual(workspace.view.lines, []); assert.equal(workspace.view.state?.parked_drafts[0].id, 'draft-1');
});
test('park waits for an already running save rather than overtaking it', async () => {
  const local = storage(); const response = deferred<RegisterState>(); const calls: string[] = [];
  const workspace = new DraftWorkspace(async <T>(name: string) => { calls.push(name); return (name === 'save_active_draft' ? await response.promise : state('2')) as T; }, local, () => `key-${calls.length}`);
  workspace.initialize(state()); workspace.change([unit]); const saving = workspace.flush(); const parking = workspace.mutate({ command: 'park_draft', label: null });
  await Promise.resolve(); assert.deepEqual(calls, ['save_active_draft']); response.resolve(state('1', [unit], 'draft-1')); await Promise.all([saving, parking]); assert.deepEqual(calls, ['save_active_draft', 'park_draft']);
});
test('a lost save blocks a queued park instead of silently proceeding', async () => {
  const local = storage(); const response = deferred<RegisterState>(); const calls: string[] = [];
  const workspace = new DraftWorkspace(async <T>(name: string) => { calls.push(name); return await response.promise as T; }, local, () => 'key');
  workspace.initialize(state()); workspace.change([unit]); const saving = workspace.flush(); const parking = workspace.mutate({ command: 'park_draft', label: null }); response.reject(new Error('offline'));
  await Promise.allSettled([saving, parking]); assert.deepEqual(calls, ['save_active_draft']); assert.equal(workspace.view.pending?.command, 'save_active_draft'); assert.deepEqual(workspace.view.lines, [unit]);
});
test('legacy nonempty drafts upgrade to a guarded durable identity without a pointless edit', async () => {
  let calls = 0; const workspace = new DraftWorkspace(async <T>(name: string) => { assert.equal(name, 'save_active_draft'); calls++; return state('1', [measured], 'migrated-id') as T; }, storage(), () => 'key');
  workspace.initialize(state('0', [measured])); assert.equal(workspace.dirty, false); await workspace.flush(); assert.equal(calls, 1); assert.equal(workspace.view.state?.active_draft?.id, 'migrated-id');
});
test('stale guards preserve local lines until explicit authoritative-state review', async () => {
  const local = storage(); const remote = state('6', [unit], 'remote');
  const workspace = new DraftWorkspace(async <T>(name: string) => { if (name === 'get_state') return remote as T; throw error('STALE_DRAFT'); }, local, () => 'key'); workspace.initialize(state('3')); workspace.change([measured]);
  await assert.rejects(workspace.flush()); assert.deepEqual(workspace.view.lines, [measured]); assert.deepEqual(workspace.view.conflict, remote); assert.notEqual(local.getItem(DRAFT_PENDING_KEY), null);
  await workspace.acceptCurrent(); assert.deepEqual(workspace.view.lines, [unit]); assert.equal(workspace.view.state?.active_draft?.id, 'remote'); assert.equal(local.getItem(DRAFT_PENDING_KEY), null);
});
test('an idempotent save reply with newer authoritative work is reviewed, not silently applied', async () => {
  const local = storage(); local.setItem(DRAFT_PENDING_KEY, JSON.stringify(pending));
  const workspace = new DraftWorkspace(async <T>() => state('9', [unit], 'other-cart') as T, local, () => 'unused'); workspace.initialize(state('9', [unit], 'other-cart'));
  await assert.rejects(workspace.retry(), /since changed/); assert.deepEqual(workspace.view.lines, pending.source_lines); assert.equal(workspace.view.conflict?.active_draft?.id, 'other-cart'); assert.equal(workspace.blocked, true);
});
test('known rejected resume can reconcile while an uncertain request cannot be abandoned', async () => {
  for (const code of ['ACTIVE_DRAFT_NOT_EMPTY', 'EMPTY_DRAFT', 'STALE_QUOTE']) {
    const workspace = new DraftWorkspace(async <T>(name: string) => { if (name === 'get_state') return state('4', [unit], 'other') as T; throw error(code); }, storage(), () => 'key'); workspace.initialize(state());
    await assert.rejects(workspace.mutate({ command: 'resume_parked_draft', draft_id: 'parked', expected_config_revision: '4' })); assert.equal(workspace.view.conflict?.draft_revision, '4');
  }
  const workspace = new DraftWorkspace(async <T>() => { throw new Error('unknown'); }, storage(), () => 'key'); workspace.initialize(state()); await assert.rejects(workspace.mutate({ command: 'discard_parked_draft', draft_id: 'parked' })); await workspace.acceptCurrent(); assert.equal(workspace.blocked, true);
});
test('resume retains invalid original quantity kinds and current issues', async () => {
  const resumed = state('8', [measured, pack, unit], 'same-id'); resumed.active_draft!.issues = [{ line_id: 'm', code: 'PRODUCT_TYPE_CHANGED', message: 'Product type changed', blocking: true }];
  const workspace = new DraftWorkspace(async <T>() => resumed as T, storage(), () => 'key'); workspace.initialize(state('7'));
  await workspace.mutate({ command: 'resume_parked_draft', draft_id: 'same-id', expected_config_revision: '4' }); assert.deepEqual(workspace.view.lines, [measured, pack, unit]); assert.equal(workspace.view.state?.active_draft?.id, 'same-id'); assert.equal(workspace.view.state?.active_draft?.issues[0].blocking, true);
});
test('a configuration refresh preserves unsaved lines and original draft CAS guard', () => {
  const workspace = new DraftWorkspace(async <T>() => state() as T, storage(), () => 'key'); workspace.initialize(state('2', [unit], 'old-id')); workspace.change([unit, measured]); workspace.refresh({ ...state('7', [pack], 'new-id'), config_revision: '5' });
  assert.deepEqual(workspace.view.lines, [unit, measured]); assert.equal(workspace.view.state?.draft_revision, '2'); assert.equal(workspace.view.state?.config_revision, '5'); assert.equal(workspace.dirty, true);
});

test('incomplete legacy quantity text can be journaled and recovered without coercion', () => {
  const incomplete: CartLine[] = [{ ...measured, quantity: { kind: 'measured', grams: '' } }, { ...unit, quantity: { kind: 'unit', count: '' } }, { ...pack, quantity: { kind: 'prepacked', count: '0.5' } }];
  const local = storage(); const request: PendingDraft = { ...pending, payload: { ...pending.payload, lines: incomplete }, source_lines: incomplete };
  local.setItem(DRAFT_PENDING_KEY, JSON.stringify(request)); assert.deepEqual(readDraftPending(local), request);
  const workspace = new DraftWorkspace(async <T>() => state() as T, local, () => 'key'); workspace.initialize(state()); assert.deepEqual(workspace.view.lines, incomplete); assert.equal(workspace.view.pending?.command, 'save_active_draft');
});

test('storage can be repaired and the exact unsent request retried without reloading local lines', async () => {
  const local = storage(); let unavailable = true; let calls = 0;
  const flaky: DraftStorage = { ...local, setItem: (key, value) => { if (unavailable) throw new Error('storage full'); local.setItem(key, value); } };
  const workspace = new DraftWorkspace(async <T>(_name: string, payload?: unknown) => { calls++; assert.equal((payload as { idempotency_key: string }).idempotency_key, 'original'); return state('1', [measured], 'active') as T; }, flaky, () => 'original'); workspace.initialize(state()); workspace.change([measured]);
  await assert.rejects(workspace.flush()); assert.equal(calls, 0); assert.deepEqual(workspace.view.pending?.source_lines, [measured]); unavailable = false; await workspace.retry(); assert.equal(calls, 1); assert.equal(workspace.blocked, false);
});
test('a different window journal is never overwritten or deleted by a new mutation', async () => {
  const local = storage(); let calls = 0;
  const workspace = new DraftWorkspace(async <T>() => { calls++; return state('1', [unit], 'active') as T; }, local, () => 'own'); workspace.initialize(state()); workspace.change([unit]);
  const foreign = JSON.stringify(pending); local.setItem(DRAFT_PENDING_KEY, foreign); await assert.rejects(workspace.flush(), /Another window/);
  assert.equal(calls, 0); assert.equal(local.getItem(DRAFT_PENDING_KEY), foreign); assert.deepEqual(workspace.view.lines, [unit]);
  local.removeItem(DRAFT_PENDING_KEY); await workspace.retry(); assert.equal(calls, 1);
});
test('a changed journal cannot be removed even after a successful server response', async () => {
  const local = storage(); const response = deferred<RegisterState>();
  const workspace = new DraftWorkspace(async <T>() => await response.promise as T, local, () => 'own'); workspace.initialize(state()); workspace.change([unit]); const task = workspace.flush();
  const foreign = JSON.stringify(pending); local.setItem(DRAFT_PENDING_KEY, foreign); response.resolve(state('1', [unit], 'own-draft')); await assert.rejects(task, /record changed/); assert.equal(local.getItem(DRAFT_PENDING_KEY), foreign); assert.equal(workspace.blocked, true);
});
test('an unresolved checkout journal blocks draft dispatch before writing a draft record', async () => {
  const local = storage(); let calls = 0; const workspace = new DraftWorkspace(async <T>() => { calls++; return state() as T; }, local, () => 'own'); workspace.initialize(state()); workspace.change([unit]); local.setItem('gram.checkout.pending.v1', '{}');
  await assert.rejects(workspace.flush(), /checkout needs recovery/); assert.equal(calls, 0); assert.equal(local.getItem(DRAFT_PENDING_KEY), null); assert.deepEqual(workspace.view.lines, [unit]);
});
test('an exclusive writer gate is acquired before reading or dispatching the mutation', async () => {
  const local = storage(); let locked = false;
  const workspace = new DraftWorkspace(async <T>() => { assert.equal(locked, true); assert.notEqual(local.getItem(DRAFT_PENDING_KEY), null); return state('1', [unit], 'active') as T; }, local, () => 'own', async work => { locked = true; try { await work(); } finally { locked = false; } });
  workspace.initialize(state()); workspace.change([unit]); await workspace.flush(); assert.equal(locked, false);
});

test('configuration refresh does not borrow another active identity when this cart has none', () => {
  const workspace = new DraftWorkspace(async <T>() => state() as T, storage(), () => 'key'); workspace.initialize(state('2')); workspace.refresh(state('3', [unit], 'other')); assert.equal(workspace.view.state?.active_draft, null); assert.equal(workspace.view.state?.draft_revision, '2'); assert.deepEqual(workspace.view.lines, []);
});
test('authoritative review is refreshed under the writer lock and refuses newly changed lines', async () => {
  const local = storage(); let current = state('2', [unit], 'remote');
  const workspace = new DraftWorkspace(async <T>(name: string) => { if (name === 'get_state') return current as T; throw error('STALE_DRAFT'); }, local, () => 'key'); workspace.initialize(state()); workspace.change([measured]); await assert.rejects(workspace.flush());
  current = state('3', [pack], 'newer'); await assert.rejects(workspace.acceptCurrent(), /changed again/); assert.deepEqual(workspace.view.lines, [measured]); assert.deepEqual(workspace.view.conflict?.draft.lines, [pack]); assert.notEqual(local.getItem(DRAFT_PENDING_KEY), null);
  await workspace.acceptCurrent(); assert.deepEqual(workspace.view.lines, [pack]); assert.equal(workspace.blocked, false);
});

test('configuration refresh updates warnings only when the durable active identity still matches', () => {
  const workspace = new DraftWorkspace(async <T>() => state() as T, storage(), () => 'key'); workspace.initialize(state('2', [unit], 'same'));
  const refreshed = state('2', [unit], 'same'); refreshed.active_draft!.issues = [{ line_id: 'u', code: 'PRODUCT_ARCHIVED', message: 'Archived', blocking: true }]; workspace.refresh(refreshed); assert.equal(workspace.view.state?.active_draft?.issues[0].code, 'PRODUCT_ARCHIVED');
});

test('authoritative review never clears a journal replaced during its state read', async () => {
  const local = storage(); const remote = state('2', [unit], 'remote'); let replaceDuringRead = false;
  const workspace = new DraftWorkspace(async <T>(name: string) => { if (name === 'get_state') { if (replaceDuringRead) local.setItem(DRAFT_PENDING_KEY, JSON.stringify(pending)); return remote as T; } throw error('STALE_DRAFT'); }, local, () => 'own'); workspace.initialize(state()); workspace.change([measured]); await assert.rejects(workspace.flush());
  replaceDuringRead = true; await assert.rejects(workspace.acceptCurrent(), /record changed while loading/); assert.equal(local.getItem(DRAFT_PENDING_KEY), JSON.stringify(pending)); assert.deepEqual(workspace.view.lines, [measured]); assert.equal(workspace.blocked, true);
});

// serde_json::Value sorts object keys on the real Rust command bridge. Object key order is
// not cart content; array order and every stored quantity character still are.
const rustWireLines: CartLine[] = JSON.parse('[{"id":"m","quantity":{"grams":"125.250","kind":"measured"},"sku":"OLIVES"},{"id":"p","quantity":{"count":"2","kind":"prepacked"},"sku":"PACK"},{"id":"u","quantity":{"count":"3","kind":"unit"},"sku":"SOAP"}]');
test('cart equality ignores Rust JSON property order but preserves exact fields and line order', () => {
  const localLines = [measured, pack, unit];
  assert.notEqual(JSON.stringify(localLines), JSON.stringify(rustWireLines));
  assert.equal(sameLines(localLines, rustWireLines), true);
  assert.equal(sameLines(localLines, [...rustWireLines].reverse()), false);
  assert.equal(sameLines([measured], [{ ...measured, id: 'different' }]), false);
  assert.equal(sameLines([measured], [{ ...measured, sku: 'olives' }]), false);
  assert.equal(sameLines([unit], [{ ...unit, quantity: { kind: 'prepacked', count: '3' } }]), false);
  assert.equal(sameLines([measured], [{ ...measured, quantity: { kind: 'measured', grams: '125.25' } }]), false);
  assert.equal(sameLines([unit], [{ ...unit, quantity: { kind: 'unit', count: '03' } }]), false);
  assert.equal(sameLines([{ ...unit, quantity: { kind: 'unit', count: '' } }], [{ quantity: { count: '', kind: 'unit' }, sku: unit.sku, id: unit.id }]), true);
});
test('normal autosave accepts Rust wire field order without false recovery or a second save', async () => {
  const local = storage(); let calls = 0; const errors: string[] = [];
  const workspace = new DraftWorkspace(async <T>(name: string) => { calls++; assert.equal(name, 'save_active_draft'); return state('1', rustWireLines, 'active') as T; }, local, () => 'key');
  workspace.subscribe(() => { if (workspace.view.error) errors.push(workspace.view.error); });
  workspace.initialize(state()); workspace.change([measured, pack, unit]); await workspace.flush();
  assert.equal(workspace.blocked, false); assert.equal(workspace.dirty, false); assert.equal(workspace.view.conflict, null); assert.deepEqual(errors, []); assert.equal(local.getItem(DRAFT_PENDING_KEY), null);
  await workspace.flush(); assert.equal(calls, 1);
  assert.equal(workspace.change([...rustWireLines, { ...unit, id: 'next' }]), true);
});

test('recovery stays visible throughout an exact retry, unlike an ordinary in-flight autosave', async () => {
  const local = storage(); const response = deferred<RegisterState>(); let calls = 0;
  const workspace = new DraftWorkspace(async <T>() => { if (++calls === 1) throw new Error('lost response'); return await response.promise as T; }, local, () => 'key');
  workspace.initialize(state()); workspace.change([measured]); const first = workspace.flush();
  assert.equal(workspace.view.busy, true); assert.equal(workspace.view.error, '');
  await assert.rejects(first); assert.notEqual(workspace.view.error, '');
  const recoveryMessage = workspace.view.error; const retry = workspace.retry(); assert.equal(workspace.view.error, recoveryMessage); assert.equal(workspace.view.busy, true); assert.notEqual(workspace.view.error, ''); assert.equal(workspace.blocked, true);
  response.resolve(state('1', [rustWireLines[0]], 'active')); await retry;
  assert.equal(workspace.view.error, ''); assert.equal(workspace.view.pending, null); assert.equal(workspace.blocked, false);
});
test('startup pending journal keeps its recovery presentation before any retry starts', () => {
  const local = storage(); local.setItem(DRAFT_PENDING_KEY, JSON.stringify(pending));
  const workspace = new DraftWorkspace(async <T>() => state() as T, local, () => 'unused'); workspace.initialize(state()); assert.notEqual(workspace.view.error, ''); assert.equal(workspace.view.busy, false);
});

test('pending cash recovery blocks cart writes and preserves malformed cash evidence', async () => {
  for (const cashRecord of ['pending', '{}', '']) {
    const local = storage(); let calls = 0;
    const workspace = new DraftWorkspace(async <T>() => { calls++; return state() as T; }, local, () => 'own'); workspace.initialize(state()); workspace.change([unit]); local.setItem('gram.ledger.pending.v1', cashRecord);
    await assert.rejects(workspace.flush(), /cash action needs recovery/); assert.equal(calls, 0); assert.equal(local.getItem(DRAFT_PENDING_KEY), null); assert.equal(local.getItem('gram.ledger.pending.v1'), cashRecord); assert.deepEqual(workspace.view.lines, [unit]);
  }
});
test('saved-cart reconciliation cannot clear work while a cash action needs recovery', async () => {
  const local = storage(); const remote = state('2', [unit], 'remote'); const workspace = new DraftWorkspace(async <T>(name: string) => { if (name === 'get_state') return remote as T; throw error('STALE_DRAFT'); }, local, () => 'own');
  workspace.initialize(state()); workspace.change([measured]); await assert.rejects(workspace.flush()); const own = local.getItem(DRAFT_PENDING_KEY); local.setItem('gram.ledger.pending.v1', 'pending');
  await assert.rejects(workspace.acceptCurrent(), /pending cash action/); assert.equal(local.getItem(DRAFT_PENDING_KEY), own); assert.deepEqual(workspace.view.lines, [measured]);
});
