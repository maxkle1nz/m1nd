import { strict as assert } from 'node:assert';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import {
  LIVE_BROWSER_EXECUTABLE_ENV,
  LIVE_EXPECTED_UI_DIGEST_ENV,
  LIVE_OWNER_TOKEN_FILE_ENV,
  LIVE_OWNER_URL_ENV,
  loadLiveAuthorizationHeader,
  loadLiveOwnerConfig,
} from './live-config';

const HERE = dirname(fileURLToPath(import.meta.url));
const UI_ROOT = resolve(HERE, '..');
const ISOLATED_URL = 'http://127.0.0.1:18444';
const TOKEN = 'a'.repeat(64);
const DIGEST = 'b'.repeat(64);
const BROWSER_EXECUTABLE = process.execPath;

test('missing URL refuses synchronously instead of skipping or choosing an ambient owner', () => {
  assert.throws(
    () => loadLiveOwnerConfig({ [LIVE_EXPECTED_UI_DIGEST_ENV]: DIGEST }),
    /M1ND_LIVE_OWNER_URL is required.*no request was made/,
  );
});

test('missing promoted UI digest refuses synchronously before a browser or request exists', () => {
  assert.throws(
    () => loadLiveOwnerConfig({ [LIVE_OWNER_URL_ENV]: ISOLATED_URL }),
    /M1ND_LIVE_EXPECTED_UI_BUNDLE_SHA256 is required.*no request was made/,
  );
});

test('only an explicit loopback origin on a non-installed port is accepted', () => {
  const config = loadLiveOwnerConfig({
    [LIVE_OWNER_URL_ENV]: ISOLATED_URL,
    [LIVE_EXPECTED_UI_DIGEST_ENV]: DIGEST,
    [LIVE_BROWSER_EXECUTABLE_ENV]: BROWSER_EXECUTABLE,
  });
  assert.equal(config.ownerOrigin, ISOLATED_URL);
  assert.equal(config.ownerPort, 18_444);

  for (const url of [
    'http://127.0.0.1:1338',
    'http://localhost:18444',
    'http://example.test:18444',
    'http://127.0.0.1:18444/path',
    'http://user:secret@127.0.0.1:18444',
  ]) {
    assert.throws(
      () =>
        loadLiveOwnerConfig({
          [LIVE_OWNER_URL_ENV]: url,
          [LIVE_EXPECTED_UI_DIGEST_ENV]: DIGEST,
          [LIVE_BROWSER_EXECUTABLE_ENV]: BROWSER_EXECUTABLE,
        }),
      /no request was made/,
    );
  }
});

test('missing exact browser executable refuses before Playwright can launch', () => {
  assert.throws(
    () =>
      loadLiveOwnerConfig({
        [LIVE_OWNER_URL_ENV]: ISOLATED_URL,
        [LIVE_EXPECTED_UI_DIGEST_ENV]: DIGEST,
      }),
    /M1ND_LIVE_BROWSER_EXECUTABLE is required.*no request was made/,
  );
});

test('only the worker fixture reads a private canonical token file', () => {
  const root = mkdtempSync(join(tmpdir(), 'm1nd-g7-live-contract-'));
  const tokenPath = join(root, 'owner-token');
  try {
    writeFileSync(tokenPath, `${TOKEN}\n`, { mode: 0o600 });
    const config = loadLiveOwnerConfig({
      [LIVE_OWNER_URL_ENV]: ISOLATED_URL,
      [LIVE_OWNER_TOKEN_FILE_ENV]: tokenPath,
      [LIVE_EXPECTED_UI_DIGEST_ENV]: DIGEST,
      [LIVE_BROWSER_EXECUTABLE_ENV]: BROWSER_EXECUTABLE,
    });
    const authorizationHeader = loadLiveAuthorizationHeader({
      [LIVE_OWNER_TOKEN_FILE_ENV]: tokenPath,
    });
    assert.equal(authorizationHeader, `Bearer ${TOKEN}`);
    assert.equal(config.expectedUiBundleSha256, `sha256:${DIGEST}`);
    assert.equal('authorizationHeader' in config, false, 'public config must not retain a bearer');
    assert.equal(JSON.stringify(config).includes(TOKEN), false, 'config JSON must not contain a bearer');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('inline bearer transport is refused without reading or echoing the secret', () => {
  const rejected = 'do-not-print-this-secret';
  let message = '';
  try {
    loadLiveOwnerConfig({
      [LIVE_OWNER_URL_ENV]: ISOLATED_URL,
      [LIVE_EXPECTED_UI_DIGEST_ENV]: DIGEST,
      [LIVE_BROWSER_EXECUTABLE_ENV]: BROWSER_EXECUTABLE,
      M1ND_LIVE_OWNER_TOKEN: rejected,
    });
  } catch (error) {
    message = error instanceof Error ? error.message : String(error);
  }
  assert.match(message, /M1ND_LIVE_OWNER_TOKEN is unsupported/);
  assert.equal(message.includes(rejected), false);
});

test('live spec has no interception/HAR and live config has no web server or 1338 URL', () => {
  const spec = readFileSync(join(HERE, 'live-owner.spec.ts'), 'utf8');
  const fixture = readFileSync(join(HERE, 'live-test.ts'), 'utf8');
  const config = readFileSync(join(UI_ROOT, 'playwright.live.config.ts'), 'utf8');
  for (const source of [spec, fixture]) {
    assert.doesNotMatch(source, /\b(?:page|context)\.route\s*\(/);
    assert.doesNotMatch(source, /routeFromHAR|fulfill\s*\(/);
  }
  assert.doesNotMatch(config, /\bwebServer\s*:/);
  assert.doesNotMatch(config, /Authorization|extraHTTPHeaders/);
  assert.doesNotMatch(config, /https?:\/\/(?:localhost|127\.0\.0\.1):1338/);
  assert.match(config, /serviceWorkers:\s*'block'/);
  assert.match(config, /browserName:\s*'chromium'/);
  assert.match(config, /executablePath:\s*live\.browserExecutable/);
  assert.match(config, /trace:\s*'off'/);
  assert.match(config, /video:\s*'off'/);
  assert.match(config, /screenshot:\s*'off'/);
  assert.match(spec, /\['document', 'script', 'stylesheet'\]/);
  assert.match(spec, /response\.serverAddr\(\)/);
  assert.match(spec, /page\.context\(\)\.serviceWorkers\(\)/);
  assert.match(spec, /assert\.equal\(url\.origin, metadata\.ownerOrigin/);
});

test('default mocked Playwright command remains separate from the opt-in live gate', () => {
  const pkg = JSON.parse(readFileSync(join(UI_ROOT, 'package.json'), 'utf8')) as {
    scripts: Record<string, string>;
  };
  assert.equal(pkg.scripts['test:e2e'], 'playwright test');
  assert.match(pkg.scripts['test:e2e:live'], /playwright\.live\.config\.ts/);
});
