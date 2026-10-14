import test from 'node:test';
import assert from 'node:assert/strict';
import { rustEnvironment } from '../scripts/rust-env.mjs';

test('Unix launch keeps PATH and selects local Cargo', () => {
  for (const platform of ['darwin', 'linux']) {
    let selected;
    const result = rustEnvironment(platform, { PATH: '/usr/bin:/bin', KEEP: 'yes' }, path => { selected = path; return true; });
    assert.ok(selected.endsWith('cargo'));
    assert.ok(!selected.endsWith('.exe'));
    assert.ok(result.env.PATH.endsWith(':/usr/bin:/bin'));
    assert.equal(result.env.KEEP, 'yes');
    assert.equal(result.cargo, selected);
  }
});
test('Windows launch keeps one PATH entry with semicolon separators', () => {
  const { env, cargo } = rustEnvironment('win32', { Path: 'original', PATH: 'duplicate' }, () => true);
  assert.ok(cargo.endsWith('cargo.exe'));
  assert.equal(Object.keys(env).filter(key => key.toLowerCase() === 'path').length, 1);
  assert.ok(env.Path.endsWith(';original'));
});
test('system Cargo is used when local Rust is absent', () => {
  const source = { PATH: '/usr/bin', CARGO_HOME: '/custom/cargo' };
  const result = rustEnvironment('linux', source, () => false);
  assert.equal(result.cargo, 'cargo');
  assert.deepEqual(result.env, source);
  assert.notEqual(result.env, source);
});
