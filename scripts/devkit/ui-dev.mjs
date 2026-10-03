#!/usr/bin/env node
// Guarded loopback bridge for Vite development. It is outside m1nd-ui so a
// browser never receives an owner bearer credential.
import { constants } from 'node:fs';
import { lstat, open } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

export const TOKEN_FILE = 'http-auth-token-v1';
export const BRIDGE_TOKEN = Symbol('m1nd-devkit-owner-token');

function port(value, name) {
  if (!/^(?:[1-9][0-9]{0,4})$/.test(String(value)) || Number(value) > 65535) {
    throw new Error(`${name} must be an integer from 1 through 65535`);
  }
  return Number(value);
}
function header(request, name) {
  const values = [];
  for (let index = 0; index < request.rawHeaders.length; index += 2) {
    if (request.rawHeaders[index].toLowerCase() === name) values.push(request.rawHeaders[index + 1]);
  }
  return values.length === 1 ? { state: 'present', value: values[0] } : { state: values.length ? 'invalid' : 'absent' };
}
function apiRequest(request) {
  try {
    const requestUrl = new URL(request.url ?? '/', 'http://localhost');
    return requestUrl.pathname === '/api' || requestUrl.pathname.startsWith('/api/');
  } catch {
    return false;
  }
}
export function permittedRequest(request, uiPort) {
  const expected = `127.0.0.1:${uiPort}`;
  const host = header(request, 'host');
  const origin = header(request, 'origin');
  const site = header(request, 'sec-fetch-site');
  if (host.state !== 'present' || host.value !== expected) return false;
  if (origin.state === 'invalid' || (origin.state === 'present' && origin.value !== `http://${expected}`)) return false;
  if (site.state === 'invalid' || (site.state === 'present' && !['same-origin', 'none'].includes(site.value.toLowerCase()))) return false;
  return true;
}
function json(response, status, body) {
  if (!response.headersSent && !response.writableEnded) {
    response.writeHead(status, { 'Cache-Control': 'no-store', 'Content-Type': 'application/json; charset=utf-8' });
    response.end(JSON.stringify(body));
  }
}
export async function readOwnerToken(runtimeDir) {
  const uid = process.getuid?.();
  if (!Number.isInteger(uid)) throw new Error('owner token unavailable');
  let runtime;
  try { runtime = await lstat(runtimeDir); } catch { throw new Error('owner token unavailable'); }
  if (runtime.isSymbolicLink() || !runtime.isDirectory() || runtime.uid !== uid || (runtime.mode & 0o777) !== 0o700) {
    throw new Error('owner token unavailable');
  }
  const tokenPath = path.join(runtimeDir, TOKEN_FILE);
  let listed;
  try { listed = await lstat(tokenPath); } catch { throw new Error('owner token unavailable'); }
  if (listed.isSymbolicLink() || !Number.isInteger(constants.O_NOFOLLOW)) throw new Error('owner token unavailable');
  let handle;
  try {
    handle = await open(tokenPath, constants.O_RDONLY | constants.O_NOFOLLOW);
    const stat = await handle.stat();
    if (!stat.isFile() || stat.uid !== uid || (stat.mode & 0o777) !== 0o600) throw new Error('owner token unavailable');
    const token = (await handle.readFile({ encoding: 'utf8' })).trim();
    if (!/^[0-9a-f]{64}$/.test(token)) throw new Error('owner token unavailable');
    return token;
  } catch {
    throw new Error('owner token unavailable');
  } finally {
    await handle?.close();
  }
}
export function createBridgeMiddleware({ runtimeDir, uiPort }) {
  return async (request, response, next) => {
    if (!apiRequest(request)) return next();
    if (!permittedRequest(request, uiPort)) return json(response, 403, { error: 'm1nd_devkit_proxy_forbidden' });
    try {
      // Deliberately read per request: owner restarts and token rotation cannot
      // leave Vite holding a now-invalid credential.
      request[BRIDGE_TOKEN] = await readOwnerToken(runtimeDir);
    } catch {
      return json(response, 503, { error: 'm1nd_devkit_proxy_unavailable' });
    }
    next();
  };
}
function bridgePlugin(options) {
  return {
    name: 'm1nd-devkit-private-loopback-bridge',
    enforce: 'pre',
    configureServer(server) {
      server.middlewares.use(createBridgeMiddleware(options));
    },
  };
}
function proxyConfig(ownerPort) {
  return {
    '^/api(?:/|\\?|$)': {
      target: `http://127.0.0.1:${ownerPort}`,
      changeOrigin: true,
      configure(proxy) {
        proxy.on('proxyReq', (ownerRequest, request, response) => {
          const token = request[BRIDGE_TOKEN];
          if (typeof token !== 'string') {
            json(response, 503, { error: 'm1nd_devkit_proxy_unavailable' });
            ownerRequest.destroy();
            return;
          }
          ownerRequest.setHeader('authorization', `Bearer ${token}`);
          if (header(request, 'origin').state === 'present') {
            ownerRequest.setHeader('origin', `http://127.0.0.1:${ownerPort}`);
          }
        });
        proxy.on('error', (_error, _request, response) => {
          if (response && 'writeHead' in response) json(response, 503, { error: 'm1nd_devkit_proxy_unavailable' });
        });
      },
    },
  };
}
export async function startViteBridge({ checkout, runtimeDir, uiPort, ownerPort }) {
  const uiRoot = path.join(checkout, 'm1nd-ui');
  const resolvedRuntime = path.resolve(runtimeDir);
  const resolvedUi = path.resolve(uiRoot);
  if (resolvedRuntime === resolvedUi || resolvedRuntime.startsWith(`${resolvedUi}${path.sep}`)) {
    throw new Error('runtime must be outside m1nd-ui so Vite cannot serve private state');
  }
  process.chdir(uiRoot);
  const viteEntry = path.join(uiRoot, 'node_modules', 'vite', 'dist', 'node', 'index.js');
  const configPath = path.join(uiRoot, 'vite.config.ts');
  const vite = await import(pathToFileURL(viteEntry).href);
  const loaded = await vite.loadConfigFromFile({ command: 'serve', mode: 'development' }, configPath, uiRoot, 'error', undefined, 'runner');
  if (!loaded) throw new Error('m1nd-ui/vite.config.ts was not found');
  const root = loaded.config.root ? path.resolve(uiRoot, loaded.config.root) : uiRoot;
  const config = vite.mergeConfig(loaded.config, {
    configFile: false,
    root,
    plugins: [bridgePlugin({ runtimeDir, uiPort })],
    server: { host: '127.0.0.1', port: uiPort, strictPort: true, cors: false, allowedHosts: ['127.0.0.1'] },
  });
  // Replace the checked-in anonymous dev proxy rather than merging it.
  config.server = { ...config.server, proxy: proxyConfig(ownerPort) };
  return vite.createServer(config);
}
export async function main(environment = process.env) {
  const checkout = environment.M1ND_CHECKOUT;
  const runtimeDir = environment.M1ND_DEVKIT_RUNTIME_DIR;
  if (!checkout || !path.isAbsolute(checkout)) throw new Error('M1ND_CHECKOUT must be an absolute path');
  if (!runtimeDir || !path.isAbsolute(runtimeDir)) throw new Error('M1ND_DEVKIT_RUNTIME_DIR must be an absolute path');
  const server = await startViteBridge({
    checkout, runtimeDir,
    uiPort: port(environment.M1ND_DEV_UI_PORT ?? '5173', 'M1ND_DEV_UI_PORT'),
    ownerPort: port(environment.M1ND_DEV_HTTP_PORT ?? '14438', 'M1ND_DEV_HTTP_PORT'),
  });
  const close = async () => { await server.close(); process.exit(0); };
  process.once('SIGINT', close); process.once('SIGTERM', close);
  await server.listen();
  server.printUrls();
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(() => { process.stderr.write('m1nd-dev: unable to start guarded UI bridge\\n'); process.exit(1); });
}
