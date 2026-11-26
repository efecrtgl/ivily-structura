import { open, save, ask } from '@tauri-apps/plugin-dialog';
import { inTauri } from './api.js';

export async function pickFolder(title = 'Select a project folder') {
  if (!inTauri()) return null;
  const r = await open({ directory: true, multiple: false, title });
  return typeof r === 'string' ? r : null;
}

export async function pickFile(title, extensions, name = 'Files') {
  if (!inTauri()) return null;
  const r = await open({ multiple: false, title, filters: [{ name, extensions }] });
  return typeof r === 'string' ? r : null;
}

export async function pickSave(title, defaultPath, extensions, name = 'Files') {
  if (!inTauri()) return null;
  const r = await save({ title, defaultPath, filters: [{ name, extensions }] });
  return typeof r === 'string' ? r : null;
}

export async function confirm(message, title = 'Please confirm', kind = 'warning') {
  if (!inTauri()) return window.confirm(message);
  return ask(message, { title, kind });
}

export function safeFileName(s) {
  return (s || 'export').replace(/[<>:"/\\|?*\x00-\x1f]+/g, '-').slice(0, 80);
}
