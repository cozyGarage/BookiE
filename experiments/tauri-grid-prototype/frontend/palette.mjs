export function fuzzyScore(query, text) {
  const needle = query.trim().toLowerCase();
  const hay = text.toLowerCase();
  if (needle === '') return 1;
  let at = 0;
  let score = 0;
  let previous = -2;
  for (const letter of needle) {
    const found = hay.indexOf(letter, at);
    if (found === -1) return 0;
    score += found === previous + 1 ? 3 : 1;
    if (found === 0 || hay[found - 1] === ' ') score += 2;
    previous = found;
    at = found + 1;
  }
  if (hay.startsWith(needle)) score += 10;
  return score;
}

export function paletteMatches(query, actions) {
  return actions
    .map((action, order) => ({ action, order, score: fuzzyScore(query, action.label) }))
    .filter((entry) => entry.score > 0 && entry.action.enabled !== false)
    .sort((a, b) => b.score - a.score || a.order - b.order)
    .map((entry) => entry.action);
}

export function paletteSelection(index, count, key) {
  if (count === 0) return 0;
  if (key === 'ArrowDown') return (index + 1) % count;
  if (key === 'ArrowUp') return (index - 1 + count) % count;
  return Math.max(0, Math.min(index, count - 1));
}
