// Quick-actions palette logic. No DOM here, so `node --test` can check it.

/** Actions whose title or keywords contain the query, ignoring case. An empty query lists all. */
export function filterActions(actions, query) {
  const q = String(query ?? '').trim().toLowerCase();
  if (!q) return actions.slice();
  return actions.filter((a) => `${a.title} ${a.keywords ?? ''}`.toLowerCase().includes(q));
}

/** Move the highlighted index by `delta`, wrapping around the list. */
export function move(index, delta, length) {
  if (length <= 0) return 0;
  return (((index + delta) % length) + length) % length;
}

/**
 * Should this key press open the palette? Ctrl or Cmd plus K always (never with Alt or Shift, which
 * browsers use for their own chords). A slash opens it only outside text fields. Shift is allowed
 * for the slash: some keyboard layouts need it to type one. `event.key` is used, not the physical
 * key, so other layouts work.
 */
export function isOpenKey(event, typing) {
  const mod = event.ctrlKey || event.metaKey;
  const key = String(event.key ?? '');
  if (mod && !event.altKey && !event.shiftKey && key.toLowerCase() === 'k') return true;
  return key === '/' && !typing && !mod && !event.altKey;
}
