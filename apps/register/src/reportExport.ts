import { invoke } from '@tauri-apps/api/core';
import { command } from './api';
import type { SalesReportRange } from './reports';

type ExportFile = { filename: string; media_type: string; content: string };

/** Only a fixed server-generated report kind and validated range cross native IPC. */
export async function exportSalesReport(range: SalesReportRange): Promise<string> {
  if ('__TAURI_INTERNALS__' in window) {
    try {
      const path = await invoke<string | null>('save_export', { kind: 'sales_report_csv', range });
      return path ? `Report saved to ${path}` : 'Export cancelled. No file was saved.';
    } catch (error) {
      if (error && typeof error === 'object' && 'message' in error) throw new Error(String(error.message));
      throw error;
    }
  }
  const file = await command<ExportFile>('export_sales_report_csv', range);
  const url = URL.createObjectURL(new Blob([file.content], { type: file.media_type }));
  try {
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = file.filename;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  return 'CSV download requested. Check your browser downloads. This report is not a backup.';
}
