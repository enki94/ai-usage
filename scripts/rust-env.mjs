import { existsSync } from 'node:fs';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
export const root = dirname(dirname(fileURLToPath(import.meta.url)));
export function rustEnvironment(platform = process.platform, source = process.env, hasFile = existsSync) {
  const env = { ...source };
  const bin = join(root, '.tools', 'cargo', 'bin');
  const executable = join(bin, platform === 'win32' ? 'cargo.exe' : 'cargo');
  if (!hasFile(executable)) return { env, cargo: 'cargo' };
  env.CARGO_HOME = join(root, '.tools', 'cargo');
  env.RUSTUP_HOME = join(root, '.tools', 'rustup');
  const pathKey = platform === 'win32' ? Object.keys(env).find(k => k.toLowerCase() === 'path') ?? 'Path' : 'PATH';
  const separator = platform === process.platform ? delimiter : platform === 'win32' ? ';' : ':';
  const previous = env[pathKey] ?? '';
  if (platform === 'win32') for (const key of Object.keys(env)) if (key.toLowerCase() === 'path') delete env[key];
  env[pathKey] = bin + separator + previous;
  return { env, cargo: executable };
}
