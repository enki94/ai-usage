import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import * as model from '../frontend/model.js';
const source = (await readFile(new URL('../frontend/app.js', import.meta.url), 'utf8')).replace(/^import[^\n]*\n/, '');
class Element {
  constructor(tag = 'div') { this.tag = tag; this.children = []; this.listeners = {}; this.attributes = {}; this.hidden = false; this.value = ''; this.checked = false; this.classList = { toggle() {} }; }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  addEventListener(name, callback) { this.listeners[name] = callback; }
  setAttribute(name, value) { this.attributes[name] = value; }
}
function texts(element) { return [element.textContent ?? '', ...element.children.flatMap(texts)].filter(Boolean); }
async function dashboard(fixture, options = {}) {
  const ids = Object.fromEntries(['refresh', 'status', 'summary', 'left', 'right', 'notice', 'selection', 'interval', 'startup', 'quit'].map(id => [id, new Element()]));
  let listener;
  const calls = [];
  let current = structuredClone(fixture);
  const invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === 'get_state') return current;
    if (command === 'save_settings') {
      if (options.rejectSave) throw '설정 저장 실패';
      current = { ...current, settings: args.settings }; return current;
    }
    if (command === 'refresh_usage') return current;
    if (command === 'quit') return;
    if (command === 'set_launch_at_login') { current = { ...current, launchAtLogin: args.enabled }; return current; }
    throw new Error('Unexpected command ' + command);
  };
  const document = { hidden: false, title: '', getElementById: id => ids[id], createElement: tag => new Element(tag), createTextNode: text => Object.assign(new Element('#text'), { textContent: text }) };
  vm.runInNewContext(source, { ...model, document, window: { __TAURI__: { core: { invoke }, event: { listen: async (_, callback) => { listener = callback; } } } }, location: { search: '' }, setInterval() {}, setTimeout, URLSearchParams, Intl, Date, Number, Boolean, String, Promise });
  await new Promise(resolve => setImmediate(resolve));
  return { ids, calls, document, event: data => listener({ payload: data }) };
}
test('dashboard renders provider identity, windows and sample notice', async () => {
  const { ids, document } = await dashboard(model.demoView());
  assert.equal(ids.left.children.length, 2); assert.equal(ids.right.children.length, 1);
  assert.match(document.title, /C 25%  X 37%  A 72%/);
  assert.ok(texts(ids.right).includes('Claude · 5시간'));
  assert.ok(texts(ids.right).includes('Gemini 4 Argon · 주간'));
  assert.equal(ids.startup.disabled, true);
  assert.match(ids.notice.textContent, /예시 데이터/);
});
test('server error and provider group text remain inert text nodes', async () => {
  const state = model.demoView(); state.demo = false;
  state.snapshots.codex.error = '<img src=x onerror=alert(1)>';
  state.snapshots.antigravity.windows[0].group = '<script>alert(1)</script>';
  const { ids } = await dashboard(state);
  assert.ok(texts(ids.left).some(s => s.includes('이전 성공 값 표시 중') && s.includes('<img')));
  assert.ok(texts(ids.right).some(s => s.includes('<script>')));
  assert.equal(ids.left.children[1].children.some(n => n.tag === 'img'), false);
});
test('selection changes send only validated-shaped settings and update title', async () => {
  const state = model.demoView(); state.demo = false;
  const { ids, calls, document } = await dashboard(state);
  ids.selection.value = 'codex'; ids.interval.value = '600';
  await ids.selection.listeners.change();
  const sent = calls.find(c => c.command === 'save_settings').args.settings;
  assert.equal(sent.selection, 'codex'); assert.equal(sent.interval, 600);
  assert.equal(document.title, 'AI Usage · X 37%');
  assert.equal(ids.selection.disabled, false);
});
test('failed settings save restores committed values and reports error', async () => {
  const { ids } = await dashboard(model.demoView(), { rejectSave: true });
  ids.selection.value = 'codex'; await ids.selection.listeners.change();
  assert.equal(ids.selection.value, 'all'); assert.equal(ids.notice.textContent, '설정 저장 실패');
});
test('backend updates clear loading controls and retain stale marker', async () => {
  const state = model.demoView();
  const { ids, document, event } = await dashboard(state);
  event({ ...state, refreshing: true }); assert.equal(ids.refresh.disabled, true);
  state.snapshots.codex.error = '만료'; event(state);
  assert.equal(ids.refresh.disabled, false); assert.match(document.title, /X 37%\*/);
});
