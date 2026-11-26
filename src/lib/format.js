const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function bytes(n) {
  if (n == null || Number.isNaN(n)) return '—';
  let v = Number(n);
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${v} B` : `${v.toFixed(v < 10 ? 1 : 0)} ${UNITS[i]}`;
}

export function num(n) {
  if (n == null || Number.isNaN(n)) return '—';
  return Number(n).toLocaleString('en-US');
}

export function compact(n) {
  if (n == null) return '—';
  const v = Number(n);
  if (Math.abs(v) >= 1e6) return `${(v / 1e6).toFixed(1)}M`;
  if (Math.abs(v) >= 1e4) return `${(v / 1e3).toFixed(1)}k`;
  return v.toLocaleString('en-US');
}

export function signed(n, digits = 0) {
  const v = Number(n);
  if (!v) return '0';
  const s = Math.abs(v) >= 1000 ? Math.round(v).toLocaleString('en-US') : v.toFixed(digits);
  return v > 0 ? `+${s}` : s;
}

export function date(ts) {
  if (!ts) return '—';
  return new Date(ts * 1000).toLocaleString('en-GB', { dateStyle: 'medium', timeStyle: 'short' });
}

export function relative(ts, now = Date.now() / 1000) {
  if (!ts) return 'never';
  const d = Math.max(0, now - ts);
  if (d < 60) return 'just now';
  if (d < 3600) return `${Math.floor(d / 60)} min ago`;
  if (d < 86400) return `${Math.floor(d / 3600)} h ago`;
  if (d < 86400 * 30) return `${Math.floor(d / 86400)} d ago`;
  return date(ts);
}

export function duration(ms) {
  if (ms == null) return '—';
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)} s`;
}

export function pct(part, total) {
  if (!total) return 0;
  return Math.round((part / total) * 1000) / 10;
}

export function basename(p) {
  if (!p) return '';
  const parts = p.split(/[\\/]/);
  return parts[parts.length - 1] || p;
}

export function dirname(p) {
  const i = p.lastIndexOf('/');
  return i < 0 ? '' : p.slice(0, i);
}

export const SEVERITY_ORDER = ['high', 'medium', 'low', 'info'];
export const SEVERITY_LABEL = { high: 'High', medium: 'Medium', low: 'Low', info: 'Info' };
export const ROLE_LABEL = {
  source: 'Source',
  test: 'Test',
  config: 'Config',
  documentation: 'Docs',
  build: 'Build',
  manifest: 'Manifest',
  lockfile: 'Lockfile',
  script: 'Script',
  asset: 'Asset',
  generated: 'Generated',
  data: 'Data',
  other: 'Other',
};
