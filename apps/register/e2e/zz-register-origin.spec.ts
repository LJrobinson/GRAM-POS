import { test, expect } from '@playwright/test';

// A Chromium origin-capability probe, not a Tauri/WebView2 runtime test. The
// minimal HTML is served by routing so no external host or register DB is used.
test('Windows default Tauri origin is secure and serializes same-origin Web Locks in Chromium', async ({ browser }, info) => {
  const context = await browser.newContext();
  await context.route('http://tauri.localhost/**', route => route.fulfill({
    status: 200, contentType: 'text/html', body: '<!doctype html><html><head><title>Local origin capability fixture</title></head><body>Training-only browser capability probe</body></html>',
  }));
  const first = await context.newPage();
  const second = await context.newPage();
  try {
    await Promise.all([first.goto('http://tauri.localhost/'), second.goto('http://tauri.localhost/')]);
    const capability = await first.evaluate(() => ({
      origin: location.origin, secure: isSecureContext,
      hasWebLocks: typeof navigator.locks?.request === 'function', userAgent: navigator.userAgent,
    }));
    expect(capability.origin).toBe('http://tauri.localhost');
    expect(capability.secure).toBe(true);
    expect(capability.hasWebLocks).toBe(true);
    const lockName = 'gram-origin-capability-regression';
    await first.evaluate(name => {
      const state = { acquired: false, complete: false, error: '', release: () => {} };
      (window as Window & { originLock?: typeof state }).originLock = state;
      void navigator.locks.request(name, async () => {
        state.acquired = true;
        await new Promise<void>(resolve => { state.release = resolve; });
        state.complete = true;
      }).catch(error => { state.error = String(error); });
    }, lockName);
    await expect.poll(() => first.evaluate(() => (window as Window & { originLock?: { acquired: boolean } }).originLock?.acquired)).toBe(true);
    await second.evaluate(name => {
      const state = { acquired: false, error: '' };
      (window as Window & { originLock?: typeof state }).originLock = state;
      void navigator.locks.request(name, () => { state.acquired = true; }).catch(error => { state.error = String(error); });
    }, lockName);
    await expect.poll(() => second.evaluate(async name => {
      const snapshot = await navigator.locks.query();
      return snapshot.pending?.filter(lock => lock.name === name).length;
    }, lockName)).toBe(1);
    const whileHeld = await second.evaluate(async name => ({
      acquired: (window as Window & { originLock?: { acquired: boolean } }).originLock?.acquired,
      held: (await navigator.locks.query()).held?.filter(lock => lock.name === name).length,
    }), lockName);
    expect(whileHeld).toEqual({ acquired: false, held: 1 });
    await first.evaluate(() => (window as Window & { originLock?: { release: () => void } }).originLock!.release());
    await expect.poll(() => second.evaluate(() => (window as Window & { originLock?: { acquired: boolean } }).originLock?.acquired)).toBe(true);
    expect(await first.evaluate(() => {
      const state = (window as Window & { originLock?: { complete: boolean; error: string } }).originLock!;
      return { complete: state.complete, error: state.error };
    })).toEqual({ complete: true, error: '' });
    expect(await second.evaluate(() => (window as Window & { originLock?: { error: string } }).originLock?.error)).toBe('');
    await info.attach('Chromium default Windows Tauri origin capability', {
      body: JSON.stringify({ ...capability, serialization: 'Second page acquired only after first page released', scope: 'Chromium probe only, not a packaged Tauri or WebView2 smoke test' }, null, 2),
      contentType: 'application/json',
    });
  } finally { await context.close(); }
});
