import type { StoreSettings } from './types.ts';

export const DEFAULT_STORE_SETTINGS: StoreSettings = { display_name: 'GRAM POS', receipt_header: '', receipt_contact: '', receipt_footer: '', timezone: 'UTC' };
export const STORE_LIMITS = { display_name: 120, receipt_header: 500, receipt_contact: 240, receipt_footer: 500, timezone: 100 } as const;
export function validTimezone(value: string): boolean {
  // The service's IANA database is authoritative; browser ICU may be older.
  return /^[A-Za-z][A-Za-z0-9_+.-]*(?:\/[A-Za-z0-9_+.-]+)*$/.test(value) && value.length <= 100;
}
export function storeSettingsError(settings: StoreSettings): string {
  for (const field of Object.keys(STORE_LIMITS) as (keyof StoreSettings)[]) {
    if (Array.from(settings[field]).length > STORE_LIMITS[field]) return `${field.replaceAll('_', ' ')} is too long (maximum ${STORE_LIMITS[field]} characters).`;
    if (/[\u0000-\u001f\u007f-\u009f\u2028\u2029\u202a-\u202e\u2066-\u2069]/.test(settings[field])) return 'Use single-line plain text without control characters.';
  }
  if (!settings.display_name.trim()) return 'Enter a store display name.';
  if (!validTimezone(settings.timezone)) return 'Enter a valid IANA timezone, such as America/New_York or UTC.';
  return '';
}
