import type { CheckoutPayload } from './types';
import type { DraftLock, DraftStorage } from './drafts';
export const CHECKOUT_PENDING_KEY = 'gram.checkout.pending.v1';
export const REGISTER_WRITER_LOCK = 'gram.register.writer.v1';
/** Chromium/WebView2 secure contexts provide real cross-window exclusivity. No fake storage lock fallback. */
export const createRegisterWriter = (locks: Pick<LockManager, 'request'> | undefined): DraftLock => async work => {
  if (!locks) throw new Error('Safe register writes require browser Web Locks. Use the supported desktop app or a secure localhost browser. Your cart has not been sent.');
  await locks.request(REGISTER_WRITER_LOCK, { ifAvailable: true }, async lock => {
    if (!lock) throw new Error('Another window is saving this register. Wait for it to finish, then retry here.');
    await work();
  });
};
export const withRegisterWriter: DraftLock = work => createRegisterWriter(navigator.locks)(work);
export function writeCheckoutJournal(storage: DraftStorage, payload: CheckoutPayload, recovering: boolean) {
  const serialized = JSON.stringify(payload);
  const existing = storage.getItem(CHECKOUT_PENDING_KEY);
  if (existing && existing !== serialized) throw new Error('Another window has an unresolved checkout. Recover it there first. This checkout was not sent.');
  if (!recovering && storage.getItem('gram.draft.pending.v1')) throw new Error('A saved cart action needs recovery before starting a checkout.');
  storage.setItem(CHECKOUT_PENDING_KEY, serialized);
}
export function clearCheckoutJournal(storage: DraftStorage, payload: CheckoutPayload) {
  if (storage.getItem(CHECKOUT_PENDING_KEY) !== JSON.stringify(payload)) throw new Error('The checkout recovery record changed in another window. It has not been removed. Reload to recover the current checkout.');
  storage.removeItem(CHECKOUT_PENDING_KEY);
}
