import type { CartLine, Currency, Quote, RegisterState } from './types';

export type DraftIssue = { line_id: string; code: string; message: string; blocking: boolean };
export type ActiveDraft = { id: string; revision: string; issues: DraftIssue[] };
export type ParkedDraft = { id: string; label: string | null; parked_at_unix: string; line_count: number };
export type DraftPreview = { draft_id: string; label: string | null; lines: CartLine[]; config_revision: string; currency: Currency; quote: Quote | null; issues: DraftIssue[] };
type Guard = { idempotency_key: string; expected_draft_revision: string };
export type DraftRequest =
  | { command: 'save_active_draft'; payload: Guard & { lines: CartLine[] } }
  | { command: 'park_draft'; payload: Guard & { label: string | null } }
  | { command: 'resume_parked_draft'; payload: Guard & { draft_id: string; expected_config_revision: string } }
  | { command: 'discard_parked_draft'; payload: Guard & { draft_id: string } };
export type PendingDraft = DraftRequest & { source_lines: CartLine[] };
export const DRAFT_PENDING_KEY = 'gram.draft.pending.v1';
export type DraftStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
export const sameLines = (left: CartLine[], right: CartLine[]) => JSON.stringify(left) === JSON.stringify(right);
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === 'string';
const nonempty = (value: unknown): value is string => text(value) && value.length > 0;
export function validDraftLines(value: unknown): value is CartLine[] {
  return Array.isArray(value) && value.every(line => record(line) && nonempty(line.id) && nonempty(line.sku) && record(line.quantity) && (line.quantity.kind === 'measured' ? text(line.quantity.grams) : (line.quantity.kind === 'unit' || line.quantity.kind === 'prepacked') && text(line.quantity.count)));
}
export function readDraftPending(storage: DraftStorage): PendingDraft | null {
  const raw = storage.getItem(DRAFT_PENDING_KEY);
  if (!raw) return null;
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new Error('Saved cart recovery data cannot be read. Keep this browser data intact and restore a valid recovery record before continuing.'); }
  if (!record(value) || !record(value.payload) || !validDraftLines(value.source_lines) || !nonempty(value.payload.idempotency_key) || !nonempty(value.payload.expected_draft_revision)) throw new Error('Saved cart recovery data is incomplete. Do not clear browser storage; the pending action must be recovered before continuing.');
  const payload = value.payload;
  const valid = value.command === 'save_active_draft' ? validDraftLines(payload.lines)
    : value.command === 'park_draft' ? payload.label === null || text(payload.label)
    : value.command === 'resume_parked_draft' ? nonempty(payload.draft_id) && nonempty(payload.expected_config_revision)
    : value.command === 'discard_parked_draft' && nonempty(payload.draft_id);
  if (!valid) throw new Error('Saved cart recovery data is not a recognized request. Keep it intact until the action can be recovered.');
  return value as PendingDraft;
}
export const knownDraftFailure = new Set(['INVALID_INPUT', 'NOT_CONFIGURED', 'STALE_DRAFT', 'STALE_QUOTE', 'DRAFT_NOT_FOUND', 'EMPTY_DRAFT', 'ACTIVE_DRAFT_NOT_EMPTY', 'IDEMPOTENCY_CONFLICT', 'GUARDED_DRAFT_REQUIRED']);
export type DraftView = { state: RegisterState | null; lines: CartLine[]; pending: PendingDraft | null; busy: boolean; error: string; conflict: RegisterState | null };
export type DraftLock = (work: () => Promise<void>) => Promise<void>;
type Command = <T>(name: string, payload?: unknown) => Promise<T>;
const message = (error: unknown) => error instanceof Error ? error.message : 'The saved cart action could not be confirmed.';

/** One writer with a durable, exact request journal. Never retry with new guards or a new key. */
export class DraftWorkspace {
  view: DraftView = { state: null, lines: [], pending: null, busy: false, error: '', conflict: null };
  private inFlight: Promise<void> | null = null;
  private listeners = new Set<() => void>();
  private command: Command;
  private storage: DraftStorage;
  private key: () => string;
  private exclusive: DraftLock;
  constructor(command: Command, storage: DraftStorage, key: () => string, exclusive: DraftLock = work => work()) {
    this.command = command; this.storage = storage; this.key = key; this.exclusive = exclusive;
    try { this.view.pending = readDraftPending(storage); if (this.view.pending) this.view.error = 'A previous cart action needs to be resolved before this register can change carts.'; }
    catch (error) { this.view.error = message(error); }
  }
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  snapshot = () => this.view;
  private update(value: Partial<DraftView>) { this.view = { ...this.view, ...value }; for (const listener of this.listeners) listener(); }
  get blocked() { return this.view.busy || !!this.view.pending || !!this.view.error; }
  get dirty() { return !!this.view.state && !sameLines(this.view.lines, this.view.state.draft.lines); }
  initialize(state: RegisterState) { this.update({ state, lines: this.view.pending?.source_lines ?? state.draft.lines }); }
  refresh(state: RegisterState) {
    // Configuration updates must not throw away a locally edited cart.
    this.update({ state: { ...state, draft: this.view.state?.draft ?? state.draft, draft_revision: this.view.state?.draft_revision ?? state.draft_revision, active_draft: this.view.state ? (this.view.state.active_draft && this.view.state.active_draft.id === state.active_draft?.id ? { ...this.view.state.active_draft, issues: state.active_draft.issues } : this.view.state.active_draft) : state.active_draft }, lines: this.view.lines });
  }
  change(lines: CartLine[]) { if (this.blocked) return false; this.update({ lines }); return true; }
  async flush(): Promise<void> {
    if (this.inFlight) await this.inFlight;
    if (this.blocked) throw new Error(this.view.error || 'Resolve the pending cart action first.');
    if (!this.view.state || (!this.dirty && !(this.view.lines.length > 0 && !this.view.state.active_draft))) return;
    return this.send({ command: 'save_active_draft', payload: { idempotency_key: this.key(), expected_draft_revision: this.view.state.draft_revision, lines: this.view.lines }, source_lines: this.view.lines });
  }
  async mutate(request: Omit<Extract<DraftRequest, { command: 'park_draft' }>, 'payload'> & { label: string | null } | { command: 'resume_parked_draft'; draft_id: string; expected_config_revision: string } | { command: 'discard_parked_draft'; draft_id: string }): Promise<void> {
    await this.flush();
    if (!this.view.state || this.blocked) throw new Error('Resolve the pending cart action first.');
    const guard = { idempotency_key: this.key(), expected_draft_revision: this.view.state.draft_revision };
    const operation: DraftRequest = request.command === 'park_draft' ? { command: request.command, payload: { ...guard, label: request.label } }
      : request.command === 'resume_parked_draft' ? { command: request.command, payload: { ...guard, draft_id: request.draft_id, expected_config_revision: request.expected_config_revision } }
      : { command: request.command, payload: { ...guard, draft_id: request.draft_id } };
    await this.send({ ...operation, source_lines: this.view.lines });
  }
  retry = async () => {
    if (this.view.busy || !this.view.pending) return;
    await this.send(this.view.pending);
  };
  private send(pending: PendingDraft): Promise<void> {
    if (this.inFlight) return Promise.reject(new Error('Another saved cart action is still running.'));
    this.update({ pending, busy: true, error: '', conflict: null });
    const serialized = JSON.stringify(pending);
    const task = (async () => {
      try {
        await this.exclusive(async () => {
        // Never overwrite another window's recovery record. With Web Locks this check and the whole request are exclusive.
        const existing = this.storage.getItem(DRAFT_PENDING_KEY);
        if (existing && existing !== serialized) throw new Error('Another window has an unresolved cart action. Resolve it there first, then retry here. Your local lines have not been changed.');
        if (this.storage.getItem('gram.checkout.pending.v1')) throw new Error('A checkout needs recovery before this cart action. Resolve it in its window first, then retry here.');
        // The journal write must succeed before any request can leave this browser. Keep unsent work in memory for retry.
        try { this.storage.setItem(DRAFT_PENDING_KEY, serialized); }
        catch { throw new Error('This browser could not keep a recovery record. No cart request was sent. Make browser storage available, then retry here without reloading.'); }
        const state = await this.command<RegisterState>(pending.command, pending.payload);
        if (pending.command === 'save_active_draft' && !sameLines(state.draft.lines, pending.payload.lines)) {
          this.update({ conflict: state, error: 'The action was saved, but the active cart has since changed in another window. Review the current saved cart before continuing.' });
          throw new Error(this.view.error);
        }
        if (this.storage.getItem(DRAFT_PENDING_KEY) !== serialized) throw new Error('The browser recovery record changed in another window. It has been kept intact. Resolve that action before retrying this one.');
        this.storage.removeItem(DRAFT_PENDING_KEY);
        this.update({ state, lines: state.draft.lines, pending: null, error: '', conflict: null });
        });
      } catch (error) {
        let conflict = this.view.conflict;
        if (record(error) && text(error.code) && knownDraftFailure.has(error.code)) {
          try { conflict = await this.command<RegisterState>('get_state'); } catch { /* Leave exact request available to retry. */ }
        }
        this.update({ error: message(error), conflict });
        throw error;
      } finally { this.inFlight = null; this.update({ busy: false }); }
    })();
    this.inFlight = task;
    return task;
  }
  /** Only after an explicit review/confirmation. Unknown outcomes have no conflict snapshot. */
  async acceptCurrent() {
    if (!this.view.conflict || this.view.busy) return;
    const reviewed = this.view.conflict;
    this.update({ busy: true });
    try {
      await this.exclusive(async () => {
        if (this.storage.getItem('gram.checkout.pending.v1')) throw new Error('Resolve the pending checkout before accepting a different saved cart.');
        if (this.view.pending && this.storage.getItem(DRAFT_PENDING_KEY) !== JSON.stringify(this.view.pending)) throw new Error('Another window owns the recovery record. Resolve its pending action before adopting saved state here.');
        const current = await this.command<RegisterState>('get_state');
        if (!sameLines(current.draft.lines, reviewed.draft.lines) || current.active_draft?.id !== reviewed.active_draft?.id) {
          this.update({ conflict: current, error: 'The saved active cart changed again. Review the latest lines before choosing it.' });
          throw new Error(this.view.error);
        }
        if (this.view.pending && this.storage.getItem(DRAFT_PENDING_KEY) !== JSON.stringify(this.view.pending)) throw new Error('The browser recovery record changed while loading the saved cart. It has been kept intact.');
        this.storage.removeItem(DRAFT_PENDING_KEY);
        this.update({ state: current, lines: current.draft.lines, pending: null, error: '', conflict: null });
      });
    } finally { this.update({ busy: false }); }
  }
  replaceAfterCheckout(state: RegisterState) {
    if (this.view.pending) throw new Error('Resolve the saved cart action before loading a checkout result.');
    this.update({ state, lines: state.draft.lines, error: '', conflict: null });
  }
}
