/**
 * Persistência do histórico de downloads em disco.
 * Salva download-history.json no userData do Electron.
 */

import { app } from 'electron';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';

export interface HistoryItem {
  id: string;
  url: string;
  title: string;
  thumbnail: string;
  date: string;
  formatId: string;
  outputPath?: string;
}

function getHistoryPath(): string {
  return join(app.getPath('userData'), 'download-history.json');
}

export function loadHistory(): HistoryItem[] {
  const filePath = getHistoryPath();
  if (!existsSync(filePath)) return [];
  try {
    const raw = readFileSync(filePath, 'utf-8');
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function saveHistory(items: HistoryItem[]): void {
  const filePath = getHistoryPath();
  const dir = dirname(filePath);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
  writeFileSync(filePath, JSON.stringify(items, null, 2), 'utf-8');
}
