import { describe, expect, it } from 'vitest';
import { globToRegExp, matchesAny } from './glob.js';

describe('glob', () => {
  it('matches bare paths and their children', () => {
    expect(globToRegExp('src/ui').test('src/ui')).toBe(true);
    expect(globToRegExp('src/ui').test('src/ui/button.js')).toBe(true);
    expect(globToRegExp('src/ui').test('src/uix')).toBe(false);
  });
  it('supports * and **', () => {
    expect(globToRegExp('src/*/api').test('src/users/api')).toBe(true);
    expect(globToRegExp('src/*/api').test('src/a/b/api')).toBe(false);
    expect(globToRegExp('**/core').test('packages/x/core')).toBe(true);
    expect(globToRegExp('src/**').test('src/a/b')).toBe(true);
  });
  it('escapes regex characters', () => {
    expect(globToRegExp('a.b').test('axb')).toBe(false);
    expect(matchesAny('lib/x', ['', 'lib'])).toBe(true);
    expect(matchesAny('lib/x', [])).toBe(false);
  });
});
