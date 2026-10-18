import { invoke } from '@tauri-apps/api/core';
export class RegisterError extends Error {
  code: string;
  constructor(message: string, code = 'connection_error') { super(message); this.code = code; }
}
export async function command<T>(name: string, payload: unknown = {}): Promise<T> {
  try {
    if ('__TAURI_INTERNALS__' in window) return await invoke<T>('register_command', { command: name, payload });
    const response = await fetch('/api/command', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ command: name, payload }) });
    const envelope = await response.json();
    if (!envelope.ok) throw new RegisterError(envelope.error?.message || 'The register could not complete this action.', envelope.error?.code);
    return envelope.data as T;
  } catch (error) {
    if (error instanceof RegisterError) throw error;
    if (error && typeof error === 'object' && 'message' in error) throw new RegisterError(String(error.message), 'code' in error ? String(error.code) : undefined);
    throw new RegisterError(typeof error === 'string' ? error : 'Cannot reach the local register. Check that the register service is running, then try again.');
  }
}
