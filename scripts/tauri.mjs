import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { root, rustEnvironment } from './rust-env.mjs';
const { env } = rustEnvironment();
const cli = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
if (!existsSync(cli)) {
  console.error('Tauri CLI가 없습니다. npm ci 또는 운영체제별 setup 스크립트를 실행하세요.');
  process.exit(1);
}
const child = spawn(process.execPath, [cli, ...process.argv.slice(2)], { cwd: root, env, stdio: 'inherit', windowsHide: true });
child.on('error', e => { console.error(e.message); process.exitCode = 1; });
child.on('exit', code => { process.exitCode = code ?? 1; });
