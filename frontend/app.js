import { providers, bounded, headline, statusText, title, resetLabel, demoView } from './model.js';
const byId = id => document.getElementById(id);
const native = Boolean(window.__TAURI__?.core);
const preview = !native && new URLSearchParams(location.search).get('demo') === '1';
let state;
let saving = false;
let lastError = '';
const timeFormat = new Intl.DateTimeFormat('ko-KR', { hour: '2-digit', minute: '2-digit', hour12: false });
function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
async function invoke(command, args) {
  if (native) return window.__TAURI__.core.invoke(command, args);
  if (!preview) throw new Error('Tauri 앱에서 열어 주세요. 브라우저 미리보기는 npm run demo로 실행할 수 있습니다.');
  if (command === 'get_state') return state ?? demoView();
  if (command === 'refresh_usage') {
    state = { ...state, refreshing: true }; render();
    await new Promise(resolve => setTimeout(resolve, 350));
    const sample = demoView();
    state = { ...sample, settings: state.settings, title: title(sample.snapshots, state.settings.selection) };
    return state;
  }
  if (command === 'save_settings') return { ...state, settings: args.settings, title: title(state.snapshots, args.settings.selection) };
  throw new Error('브라우저 미리보기에서는 사용할 수 없습니다.');
}
function showError(error) {
  lastError = typeof error === 'string' ? error : error?.message ?? '작업에 실패했습니다.';
  render();
}
function render() {
  if (!state) { byId('notice').hidden = false; byId('notice').textContent = lastError; return; }
  const selectedTitle = title(state.snapshots, state.settings.selection);
  document.title = 'AI Usage · ' + selectedTitle;
  byId('refresh').disabled = state.refreshing;
  byId('refresh').classList.toggle('busy', state.refreshing);
  byId('status').replaceChildren(element('i'), document.createTextNode(state.demo ? '미리보기 · 예시 데이터' : state.refreshing ? '사용량 조회 중' : '5시간 · 주간 한도'));
  byId('summary').replaceChildren(...providers.map(provider => {
    const item = element('div', 'summary-item');
    item.append(element('span', '', provider.name), element('strong', provider.key + '-color', statusText(state.snapshots[provider.key])));
    return item;
  }));
  byId('left').replaceChildren(); byId('right').replaceChildren();
  for (const provider of providers) {
    const snapshot = state.snapshots[provider.key] ?? { windows: [] };
    const card = element('section', 'card');
    const header = element('div', 'card-head');
    const name = element('div'); name.append(element('h2', '', provider.name), element('div', 'provider-description', provider.description));
    header.append(element('div', 'provider-icon ' + provider.key + '-color', provider.short), name); card.append(header);
    for (const window of snapshot.windows) {
      const row = element('div', 'window');
      const used = bounded(window.used);
      const heading = element('div', 'window-heading');
      heading.append(element('span', 'window-label', [window.group, window.label].filter(Boolean).join(' · ')), element('span', 'used', used.toFixed(1) + '%'));
      const bar = element('progress', used >= 90 ? 'danger' : used >= 70 ? 'warning' : '');
      bar.max = 100; bar.value = used;
      bar.setAttribute('aria-label', provider.name + ' ' + [window.group, window.label].filter(Boolean).join(' ') + ' 사용률');
      row.append(heading, bar, element('div', 'reset', resetLabel(window.resetAt))); card.append(row);
    }
    if (snapshot.isLoading) card.append(element('div', 'empty', '조회 중…'));
    else if (!snapshot.windows.length && !snapshot.error) card.append(element('div', 'empty', '아직 조회된 값이 없습니다. 새로고침하면 CLI 로그인 정보를 확인합니다.'));
    if (snapshot.error) card.append(element('div', 'error', (headline(snapshot) !== null ? '이전 성공 값 표시 중\n' : '') + snapshot.error));
    if (snapshot.updatedAt && Number.isFinite(Date.parse(snapshot.updatedAt))) card.append(element('div', 'updated', '마지막 성공 조회 ' + timeFormat.format(new Date(snapshot.updatedAt))));
    byId(provider.key === 'antigravity' ? 'right' : 'left').append(card);
  }
  byId('selection').value = state.settings.selection;
  byId('interval').value = String(state.settings.interval);
  byId('startup').checked = state.launchAtLogin;
  byId('startup').disabled = saving || state.demo;
  byId('selection').disabled = saving;
  byId('interval').disabled = saving;
  byId('quit').disabled = !native;
  const message = lastError || state.settingsError || (state.demo ? '예시 데이터입니다. 실제 사용량은 CLI 로그인 후 일반 모드에서 확인하세요.' : '');
  byId('notice').hidden = !message;
  byId('notice').textContent = message;
}
byId('refresh').addEventListener('click', async () => {
  if (state?.refreshing) return;
  lastError = '';
  try { state = await invoke('refresh_usage'); render(); } catch (error) { showError(error); }
});
async function changeSettings() {
  if (!state || saving) return;
  const settings = { selection: byId('selection').value, interval: Number(byId('interval').value) };
  saving = true; lastError = ''; render();
  try { state = await invoke('save_settings', { settings }); } catch (error) { showError(error); }
  finally { saving = false; render(); }
}
byId('selection').addEventListener('change', changeSettings);
byId('interval').addEventListener('change', changeSettings);
byId('startup').addEventListener('change', async () => {
  if (!state || saving) return;
  const enabled = byId('startup').checked;
  saving = true; lastError = ''; render();
  try { state = await invoke('set_launch_at_login', { enabled }); }
  catch (error) { showError(error); try { state = await invoke('get_state'); } catch {} }
  finally { saving = false; render(); }
});
byId('quit').addEventListener('click', async () => { try { await invoke('quit'); } catch (error) { showError(error); } });
async function initialize() {
  try {
    if (native) await window.__TAURI__.event.listen('usage-changed', event => { state = event.payload; render(); });
    state = await invoke('get_state'); render();
  } catch (error) { showError(error); }
}
setInterval(() => { if (state && !document.hidden) render(); }, 60000);
void initialize();
