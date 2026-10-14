import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { root, rustEnvironment } from './rust-env.mjs';
const { env, cargo } = rustEnvironment();
const child = spawn(cargo, process.argv.slice(2), { cwd: join(root, 'src-tauri'), env, stdio: 'inherit', windowsHide: true });
child.on('error', () => { console.error('Cargo 실행 실패. Windows는 scripts/setup.ps1, macOS/Linux는 sh scripts/setup.sh를 실행하세요.'); process.exitCode = 1; });
child.on('exit', code => { process.exitCode = code ?? 1; });
