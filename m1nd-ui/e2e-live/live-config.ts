import {
  accessSync,
  closeSync,
  constants as fsConstants,
  fstatSync,
  lstatSync,
  openSync,
  readFileSync,
} from 'node:fs';
import { isAbsolute } from 'node:path';

export const LIVE_OWNER_URL_ENV = 'M1ND_LIVE_OWNER_URL';
export const LIVE_OWNER_TOKEN_FILE_ENV = 'M1ND_LIVE_OWNER_TOKEN_FILE';
export const LIVE_EXPECTED_UI_DIGEST_ENV = 'M1ND_LIVE_EXPECTED_UI_BUNDLE_SHA256';
export const LIVE_BROWSER_EXECUTABLE_ENV = 'M1ND_LIVE_BROWSER_EXECUTABLE';
const REFUSED_INLINE_TOKEN_ENV = 'M1ND_LIVE_OWNER_TOKEN';

type Environment = Readonly<Record<string, string | undefined>>;

export interface LiveOwnerConfig {
  /** Normalized origin only. Paths, credentials, query strings, and fragments are refused. */
  readonly ownerOrigin: string;
  readonly ownerPort: number;
  /** External promoted-bundle expectation, normalized to the manifest's `sha256:` form. */
  readonly expectedUiBundleSha256: string;
  /** Exact executable pre-attested by the isolated launcher. */
  readonly browserExecutable: string;
}

function refuse(detail: string): never {
  throw new Error(`G7 LIVE configuration refused: ${detail}; no request was made`);
}

function requiredOwnerOrigin(raw: string | undefined): { origin: string; port: number } {
  if (!raw?.trim()) {
    return refuse(
      `${LIVE_OWNER_URL_ENV} is required (an isolated owner URL; there is no localhost:1338 default)`,
    );
  }

  let url: URL;
  try {
    url = new URL(raw.trim());
  } catch {
    return refuse(`${LIVE_OWNER_URL_ENV} must be an absolute http(s) URL`);
  }

  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    return refuse(`${LIVE_OWNER_URL_ENV} must use http or https`);
  }
  if (url.username || url.password) {
    return refuse(`${LIVE_OWNER_URL_ENV} must not carry credentials`);
  }
  if (url.pathname !== '/' || url.search || url.hash) {
    return refuse(`${LIVE_OWNER_URL_ENV} must be an origin with no path, query, or fragment`);
  }
  if (!url.port) {
    return refuse(`${LIVE_OWNER_URL_ENV} must name an explicit isolated-owner port`);
  }

  const hostname = url.hostname.replace(/^\[|\]$/g, '').toLowerCase();
  if (!['127.0.0.1', '::1'].includes(hostname)) {
    return refuse(
      `${LIVE_OWNER_URL_ENV} must name numeric loopback 127.0.0.1 or [::1] (DNS names are refused)`,
    );
  }

  const port = Number(url.port);
  if (!Number.isSafeInteger(port) || port < 1 || port > 65_535) {
    return refuse(`${LIVE_OWNER_URL_ENV} has an invalid port`);
  }
  if (port === 1338) {
    return refuse('port 1338 is the installed owner and is outside the isolated G7 LIVE gate');
  }

  return { origin: url.origin, port };
}

function canonicalBearer(raw: string, source: string): string {
  const token = raw.trim();
  if (!/^[0-9a-f]{64}$/.test(token)) {
    return refuse(`${source} must contain one canonical 32-byte lowercase hex bearer`);
  }
  return token;
}

function bearerFromFile(path: string): string {
  let descriptor: number | undefined;
  try {
    const pathStat = lstatSync(path);
    if (pathStat.isSymbolicLink() || !pathStat.isFile()) {
      return refuse(`${LIVE_OWNER_TOKEN_FILE_ENV} must name a regular non-symlink file`);
    }
    descriptor = openSync(path, fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0));
    const openedStat = fstatSync(descriptor);
    if (
      !openedStat.isFile() ||
      openedStat.dev !== pathStat.dev ||
      openedStat.ino !== pathStat.ino
    ) {
      return refuse(`${LIVE_OWNER_TOKEN_FILE_ENV} changed identity while it was opened`);
    }
    if (openedStat.size > 1_024) {
      return refuse(`${LIVE_OWNER_TOKEN_FILE_ENV} is unexpectedly large`);
    }
    if (process.platform !== 'win32' && (openedStat.mode & 0o077) !== 0) {
      return refuse(`${LIVE_OWNER_TOKEN_FILE_ENV} permissions must exclude group and other access`);
    }
    return canonicalBearer(readFileSync(descriptor, 'utf8'), LIVE_OWNER_TOKEN_FILE_ENV);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith('G7 LIVE configuration refused:')) {
      throw error;
    }
    return refuse(`${LIVE_OWNER_TOKEN_FILE_ENV} could not be read safely`);
  } finally {
    if (descriptor !== undefined) closeSync(descriptor);
  }
}

function requiredUiDigest(raw: string | undefined): string {
  if (!raw?.trim()) {
    return refuse(`${LIVE_EXPECTED_UI_DIGEST_ENV} is required for the promoted bundle`);
  }
  const digest = raw.trim().replace(/^sha256:/, '');
  if (!/^[0-9a-f]{64}$/.test(digest)) {
    return refuse(`${LIVE_EXPECTED_UI_DIGEST_ENV} must be a lowercase SHA-256 digest`);
  }
  return `sha256:${digest}`;
}

function requiredBrowserExecutable(raw: string | undefined): string {
  const path = raw?.trim();
  if (!path) return refuse(`${LIVE_BROWSER_EXECUTABLE_ENV} is required`);
  if (!isAbsolute(path)) return refuse(`${LIVE_BROWSER_EXECUTABLE_ENV} must be absolute`);
  try {
    const pathStat = lstatSync(path);
    if (pathStat.isSymbolicLink() || !pathStat.isFile()) {
      return refuse(`${LIVE_BROWSER_EXECUTABLE_ENV} must name a regular non-symlink file`);
    }
    accessSync(path, fsConstants.X_OK);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith('G7 LIVE configuration refused:')) {
      throw error;
    }
    return refuse(`${LIVE_BROWSER_EXECUTABLE_ENV} is not an executable file`);
  }
  return path;
}

/**
 * Read optional owner authentication only inside the Playwright worker fixture.
 * The launcher-facing public config never reads or retains bearer material, so
 * no secret can enter Playwright metadata or the browser process environment.
 */
export function loadLiveAuthorizationHeader(
  env: Environment = process.env,
): string | undefined {
  if (Object.prototype.hasOwnProperty.call(env, REFUSED_INLINE_TOKEN_ENV)) {
    return refuse(
      `${REFUSED_INLINE_TOKEN_ENV} is unsupported; use ${LIVE_OWNER_TOKEN_FILE_ENV} only`,
    );
  }
  const tokenFile = env[LIVE_OWNER_TOKEN_FILE_ENV]?.trim();
  if (!tokenFile) return undefined;
  return `Bearer ${bearerFromFile(tokenFile)}`;
}

/**
 * Resolve the opt-in gate before Playwright creates a browser context. Missing
 * or unsafe configuration throws synchronously, so the harness cannot silently
 * skip, pass, start a dev server, or contact an ambient owner.
 */
export function loadLiveOwnerConfig(env: Environment = process.env): LiveOwnerConfig {
  const { origin, port } = requiredOwnerOrigin(env[LIVE_OWNER_URL_ENV]);
  if (Object.prototype.hasOwnProperty.call(env, REFUSED_INLINE_TOKEN_ENV)) {
    return refuse(
      `${REFUSED_INLINE_TOKEN_ENV} is unsupported; use ${LIVE_OWNER_TOKEN_FILE_ENV} only`,
    );
  }
  return Object.freeze({
    ownerOrigin: origin,
    ownerPort: port,
    expectedUiBundleSha256: requiredUiDigest(env[LIVE_EXPECTED_UI_DIGEST_ENV]),
    browserExecutable: requiredBrowserExecutable(env[LIVE_BROWSER_EXECUTABLE_ENV]),
  });
}
