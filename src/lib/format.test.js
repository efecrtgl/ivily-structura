import { describe, expect, it } from 'vitest';
import { bytes, compact, relative, signed, pct, basename, dirname, duration } from './format.js';

describe('format', () => {
  it('formats bytes', () => {
    expect(bytes(0)).toBe('0 B');
    expect(bytes(1536)).toBe('1.5 KB');
    expect(bytes(50 * 1024 * 1024)).toBe('50 MB');
    expect(bytes(null)).toBe('—');
  });
  it('formats numbers', () => {
    expect(compact(12345)).toBe('12.3k');
    expect(compact(2_500_000)).toBe('2.5M');
    expect(signed(3)).toBe('+3');
    expect(signed(-2)).toBe('-2');
    expect(signed(0)).toBe('0');
    expect(pct(1, 3)).toBe(33.3);
    expect(pct(1, 0)).toBe(0);
  });
  it('formats time', () => {
    expect(relative(100, 130)).toBe('just now');
    expect(relative(0)).toBe('never');
    expect(relative(1000, 1000 + 7200)).toBe('2 h ago');
    expect(duration(450)).toBe('450 ms');
    expect(duration(2500)).toBe('2.5 s');
  });
  it('handles paths', () => {
    expect(basename('C:\\a\\b')).toBe('b');
    expect(basename('src/x.js')).toBe('x.js');
    expect(dirname('src/lib/x.js')).toBe('src/lib');
    expect(dirname('x.js')).toBe('');
  });
});
