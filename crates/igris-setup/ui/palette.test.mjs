import test from 'node:test';
import assert from 'node:assert/strict';
import { filterActions, move, isOpenKey } from './palette.js';

const actions = [
  { title: 'Scan again', keywords: 'refresh detect' },
  { title: 'Switch theme', keywords: 'dark light mode' },
  { title: 'Open the docs', keywords: 'help guide' },
];

test('filterActions matches title and keywords, ignoring case', () => {
  assert.equal(filterActions(actions, 'scan').length, 1);
  assert.equal(filterActions(actions, 'SCAN').length, 1);
  assert.equal(filterActions(actions, 'zzz').length, 0);
  assert.equal(filterActions(actions, '').length, 3);
  assert.equal(filterActions(actions, '  ').length, 3);
  assert.equal(filterActions(actions, 'dark')[0].title, 'Switch theme');
});

test('filterActions returns a copy when the query is empty', () => {
  const out = filterActions(actions, '');
  out.pop();
  assert.equal(actions.length, 3);
});

test('move wraps in both directions and survives an empty list', () => {
  assert.equal(move(0, -1, 10), 9);
  assert.equal(move(9, 1, 10), 0);
  assert.equal(move(3, 1, 10), 4);
  assert.equal(move(0, 1, 0), 0);
});

const key = (k, mods = {}) => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });

test('isOpenKey: Ctrl or Cmd plus K opens, in any case', () => {
  assert.equal(isOpenKey(key('k', { ctrlKey: true }), false), true);
  assert.equal(isOpenKey(key('k', { metaKey: true }), false), true);
  assert.equal(isOpenKey(key('K', { ctrlKey: true }), false), true);
  assert.equal(isOpenKey(key('k', { ctrlKey: true }), true), true, 'works while typing');
});

test('isOpenKey: other chords and a bare k do not open', () => {
  assert.equal(isOpenKey(key('k', { ctrlKey: true, shiftKey: true }), false), false);
  assert.equal(isOpenKey(key('k', { ctrlKey: true, altKey: true }), false), false);
  assert.equal(isOpenKey(key('k'), false), false);
});

test('isOpenKey: slash opens only outside text fields and without Ctrl, Cmd or Alt', () => {
  assert.equal(isOpenKey(key('/'), false), true);
  assert.equal(isOpenKey(key('/'), true), false, 'typing a slash in a field');
  assert.equal(isOpenKey(key('/', { ctrlKey: true }), false), false);
  assert.equal(isOpenKey(key('/', { metaKey: true }), false), false);
  assert.equal(isOpenKey(key('/', { altKey: true }), false), false);
  assert.equal(isOpenKey(key('/', { shiftKey: true }), false), true, 'slash needs Shift on some layouts');
});
