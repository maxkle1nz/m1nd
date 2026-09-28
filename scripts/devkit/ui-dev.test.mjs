import assert from 'node:assert/strict';
import { createServer, request } from 'node:http';
import { mkdtemp, mkdir, chmod, writeFile, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { startViteBridge, readOwnerToken } from './ui-dev.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const tokenA = 'a'.repeat(64);
const tokenB = 'b'.repeat(64);

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server.address().port));
  });
}
function close(server) { return new Promise(resolve => server.close(resolve)); }
function response(port, headers = {}) {
  return new Promise((resolve, reject) => {
    const req = request({ hostname: '127.0.0.1', port, path: '/api/health', headers }, res => {
      let body = '';
      res.setEncoding('utf8');
      res.on('data', part => { body += part; });
      res.on('end', () => resolve({ status: res.statusCode, body }));
    });
    req.on('error', reject);
    req.end();
  });
}
async function unusedPort() {
  const server = createServer();
  const port = await listen(server);
  await close(server);
  return port;
}
async function privateRuntime() {
  const parent = await mkdtemp(path.join(tmpdir(), 'm1nd-devkit-proxy-'));
  const runtime = path.join(parent, 'runtime');
  await mkdir(runtime, { mode: 0o700 });
  await chmod(runtime, 0o700);
  return runtime;
}

test('real Vite bridge rejects foreign browser requests before upstream and rotates credentials per request', async () => {
  const runtime = await privateRuntime();
  const tokenPath = path.join(runtime, 'http-auth-token-v1');
  await writeFile(tokenPath, tokenA + '\n', { mode: 0o600 });
  await chmod(tokenPath, 0o600);
  const seen = [];
  const owner = createServer((req, res) => {
    seen.push(req.headers.authorization);
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end('{"ok":true}');
  });
  const ownerPort = await listen(owner);
  const uiPort = await unusedPort();
  const vite = await startViteBridge({ checkout: root, runtimeDir: runtime, uiPort, ownerPort });
  await vite.listen();
  try {
    const denied = await response(uiPort, {
      host: `foreign.example:${uiPort}`,
      origin: `http://foreign.example:${uiPort}`,
      'sec-fetch-site': 'same-origin',
    });
    assert.equal(denied.status, 403);
    assert.equal(seen.length, 0);

    // A normal same-origin GET has Host but generally no Origin or
    // Sec-Fetch-Site. The bridge must not turn read-only UI requests into 403.
    const allowed = await response(uiPort);
    assert.equal(allowed.status, 200);
    assert.equal(allowed.body, '{"ok":true}');
    assert.equal(allowed.body.includes(tokenA), false);
    assert.deepEqual(seen, [`Bearer ${tokenA}`]);

    await writeFile(tokenPath, tokenB + '\n', { mode: 0o600 });
    await chmod(tokenPath, 0o600);
    const rotated = await response(uiPort);
    assert.equal(rotated.status, 200);
    assert.deepEqual(seen, [`Bearer ${tokenA}`, `Bearer ${tokenB}`]);

    await writeFile(tokenPath, 'not-a-token\n', { mode: 0o600 });
    await chmod(tokenPath, 0o600);
    const missing = await response(uiPort);
    assert.equal(missing.status, 503);
    assert.equal(seen.length, 2);

    await writeFile(tokenPath, tokenB + '\n', { mode: 0o600 });
    await chmod(tokenPath, 0o600);
    await close(owner);
    const unavailable = await response(uiPort);
    assert.equal(unavailable.status, 503);
  } finally {
    await vite.close();
    await close(owner);
  }
});

test('token and runtime symlinks fail closed', async t => {
  const runtime = await privateRuntime();
  const tokenPath = path.join(runtime, 'http-auth-token-v1');
  const target = path.join(runtime, 'target-token');
  await writeFile(target, tokenA + '\n', { mode: 0o600 });
  await chmod(target, 0o600);
  try {
    await symlink(target, tokenPath);
  } catch (error) {
    t.skip(`symlinks unavailable: ${error.code}`);
    return;
  }
  await assert.rejects(readOwnerToken(runtime));

  const parent = path.dirname(runtime);
  const linked = path.join(parent, 'runtime-link');
  await symlink(runtime, linked);
  await assert.rejects(readOwnerToken(linked));
});

test('Vite bridge refuses a runtime under the public UI tree', async () => {
  await assert.rejects(startViteBridge({
    checkout: root,
    runtimeDir: path.join(root, 'm1nd-ui', 'public', 'runtime'),
    uiPort: await unusedPort(),
    ownerPort: await unusedPort(),
  }));
});
