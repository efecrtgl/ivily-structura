// Minimal glob matching compatible with the backend's layer/group patterns:
// `*` matches within a segment, `**` across segments, `?` one character. A bare path
// (`src/ui`) matches itself and everything below it.
export function globToRegExp(glob) {
  const g = glob.trim().replace(/\\/g, '/').replace(/\/+$/, '');
  let re = '';
  for (let i = 0; i < g.length; i++) {
    const c = g[i];
    if (c === '*') {
      if (g[i + 1] === '*') {
        re += '.*';
        i++;
        if (g[i + 1] === '/') i++;
      } else re += '[^/]*';
    } else if (c === '?') re += '[^/]';
    else re += c.replace(/[.+^${}()|[\]\\]/g, '\\$&');
  }
  return new RegExp(`^${re}(?:/.*)?$`);
}

export function matchesAny(path, patterns) {
  return (patterns || []).some((p) => p.trim() && globToRegExp(p).test(path));
}
