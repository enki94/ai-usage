export const providers = [
  { key: 'claude', name: 'Claude', short: 'C', color: '#b85a2a', description: 'Claude Code' },
  { key: 'codex', name: 'Codex', short: 'X', color: '#1f7a5a', description: 'Codex CLI' },
  { key: 'antigravity', name: 'Antigravity', short: 'A', color: '#2b5fd9', description: '모델 그룹별 한도' },
];
export function bounded(value) { return Number.isFinite(value) ? Math.min(100, Math.max(0, value)) : 0; }
export function headline(snapshot) {
  const windows = snapshot?.windows ?? [];
  const short = windows.filter(w => w.label === '5시간');
  const selected = short.length ? short : windows;
  return selected.length ? Math.max(...selected.map(w => bounded(w.used))) : null;
}
export function statusText(snapshot) {
  const value = headline(snapshot);
  if (value === null) return snapshot?.error ? '!' : '—';
  return Math.round(value) + '%' + (snapshot?.error ? '*' : '');
}
export function title(snapshots, selection = 'all') {
  return providers.filter(p => selection === 'all' || selection === p.key)
    .map(p => p.short + ' ' + statusText(snapshots[p.key])).join('  ');
}
export function remaining(resetAt, now = Date.now()) {
  const timestamp = Date.parse(resetAt);
  if (!Number.isFinite(timestamp)) return '리셋 -';
  const minutes = Math.ceil((timestamp - now) / 60000);
  if (minutes <= 0) return '리셋 시각 지남';
  if (minutes >= 1440) return Math.floor(minutes / 1440) + '일 ' + Math.floor((minutes % 1440) / 60) + '시간 남음';
  if (minutes >= 60) return Math.floor(minutes / 60) + '시간 ' + minutes % 60 + '분 남음';
  return minutes + '분 남음';
}
export function resetLabel(resetAt, now = Date.now()) {
  if (!resetAt || !Number.isFinite(Date.parse(resetAt))) return '리셋 -';
  const time = new Intl.DateTimeFormat('ko-KR', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false }).format(new Date(resetAt));
  return '리셋 ' + time + ' · ' + remaining(resetAt, now);
}
export function demoView(now = Date.now()) {
  const snapshots = Object.fromEntries(providers.map((p, index) => [p.key, {
    windows: [
      { id: '5h', group: index === 2 ? 'Gemini' : null, label: '5시간', used: [24.5, 37, 25][index], resetAt: new Date(now + 2 * 3600000).toISOString() },
      { id: 'weekly', group: index === 2 ? 'Gemini' : null, label: '주간', used: [63, 48, 31][index], resetAt: new Date(now + 3 * 86400000).toISOString() },
      ...(index === 2 ? [
        { id: 'claude-5h', group: 'Claude', label: '5시간', used: 72, resetAt: new Date(now + 4 * 3600000).toISOString() },
        { id: 'argon-weekly', group: 'Gemini 4 Argon', label: '주간', used: 18, resetAt: new Date(now + 5 * 86400000).toISOString() },
      ] : []),
    ], updatedAt: new Date(now).toISOString(), error: null, isLoading: false,
  }]));
  return { snapshots, settings: { interval: 0, selection: 'all' }, refreshing: false, launchAtLogin: false, settingsError: null, title: title(snapshots), demo: true };
}
