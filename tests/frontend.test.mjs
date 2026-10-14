import test from 'node:test';
import assert from 'node:assert/strict';
import { bounded, headline, statusText, title, remaining, resetLabel, demoView } from '../frontend/model.js';
const w = (label, used) => ({ label, used });
test('headline selects maximum short window, with weekly fallback', () => {
  assert.equal(headline({ windows: [w('5시간', 25), w('5시간', 90), w('주간', 99)] }), 90);
  assert.equal(headline({ windows: [w('주간', 73)] }), 73);
  assert.equal(headline({ windows: [] }), null);
});
test('values are bounded and nonfinite values are not displayed', () => {
  assert.equal(bounded(150), 100); assert.equal(bounded(-2), 0);
  assert.equal(bounded(NaN), 0); assert.equal(bounded(Infinity), 0);
});
test('unqueried, error and stale indicators remain distinct', () => {
  assert.equal(statusText({ windows: [] }), '—');
  assert.equal(statusText({ windows: [], error: 'failure' }), '!');
  assert.equal(statusText({ windows: [w('5시간', 42.5)], error: 'failure' }), '43%*');
});
test('service selection and provider order match native tray', () => {
  const snapshots = { claude: { windows: [w('5시간', 21)] }, codex: { windows: [w('5시간', 42)], error: 'failure' }, antigravity: { windows: [w('5시간', 63)] } };
  assert.equal(title(snapshots), 'C 21%  X 42%*  A 63%');
  assert.equal(title(snapshots, 'codex'), 'X 42%*');
});
test('reset countdown handles unknown, elapsed and fractional minutes', () => {
  const now = Date.parse('2026-10-14T08:00:00Z');
  const at = minutes => new Date(now + minutes * 60000).toISOString();
  assert.equal(remaining('invalid', now), '리셋 -');
  assert.equal(remaining(at(-1), now), '리셋 시각 지남');
  assert.equal(remaining(at(.1), now), '1분 남음');
  assert.equal(remaining(at(121), now), '2시간 1분 남음');
  assert.equal(remaining(at(1500), now), '1일 1시간 남음');
  assert.equal(resetLabel(null, now), '리셋 -');
});
test('preview explicitly marks sample data and uses group maximum', () => {
  const sample = demoView(0);
  assert.equal(sample.demo, true); assert.equal(sample.launchAtLogin, false);
  assert.equal(sample.title, 'C 25%  X 37%  A 72%');
  assert.equal(sample.snapshots.antigravity.windows.length, 3);
});
