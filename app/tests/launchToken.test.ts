/**
 * Spec §12.22: the backend rejects a forged or expired launch token, accepts a
 * valid one, and resolves the same `sub` the client reports.
 *
 * This runs against a real RS256 keypair and a local JWKS endpoint, so the
 * whole verification path — signature, algorithm, audience, expiry — is
 * exercised rather than mocked.
 */
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import * as jose from 'jose';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

const APP_ID = 'sol-rigs-test-app';
const WALLET = '7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU';

let server: Server;
let jwksUrl: string;
let signingKey: jose.KeyLike;
let attackerKey: jose.KeyLike;

beforeAll(async () => {
  const honest = await jose.generateKeyPair('RS256', { extractable: true });
  const attacker = await jose.generateKeyPair('RS256', { extractable: true });
  signingKey = honest.privateKey;
  attackerKey = attacker.privateKey;

  // Only the honest key is published, so anything signed by the attacker's
  // key has no matching JWK and cannot verify.
  const jwk = await jose.exportJWK(honest.publicKey);
  jwk.kid = 'test-key-1';
  jwk.alg = 'RS256';
  jwk.use = 'sig';
  const body = JSON.stringify({ keys: [jwk] });

  server = createServer((_req, res) => {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(body);
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const { port } = server.address() as AddressInfo;
  jwksUrl = `http://127.0.0.1:${port}/jwks.json`;

  process.env.CHERRY_APP_ID = APP_ID;
  process.env.CHERRY_JWKS_URL = jwksUrl;
});

afterAll(async () => {
  await new Promise<void>((r) => server.close(() => r()));
});

type Claims = Record<string, unknown>;

/** Cherry binds a token to an app via the `app_id` claim, not `aud`. */
async function mint(
  claims: Claims,
  opts: { key?: jose.KeyLike; expiresIn?: string; appId?: string } = {},
): Promise<string> {
  return new jose.SignJWT({ app_id: opts.appId ?? APP_ID, ...claims })
    .setProtectedHeader({ alg: 'RS256', kid: 'test-key-1' })
    .setIssuedAt()
    .setIssuer('https://chat.cherry.fun')
    .setExpirationTime(opts.expiresIn ?? '5m')
    .sign(opts.key ?? signingKey);
}

// Imported lazily so the env vars above are in place first.
async function verify(token: string) {
  const { verifySession } = await import('../src/lib/server/launchToken');
  return verifySession(token);
}

describe('launch token verification', () => {
  it('accepts a valid token and resolves the wallet from the signed sub', async () => {
    const token = await mint({
      sub: WALLET,
      room_id: 'room-42',
    });
    const session = await verify(token);
    expect(session.sub).toBe(WALLET);
    expect(session.roomId).toBe('room-42');
  });

  it('rejects a token signed by a key that is not in the JWKS', async () => {
    const forged = await mint({ sub: WALLET }, { key: attackerKey });
    await expect(verify(forged)).rejects.toThrow();
  });

  it('rejects an expired token', async () => {
    const stale = await new jose.SignJWT({ sub: WALLET, app_id: APP_ID })
      .setProtectedHeader({ alg: 'RS256', kid: 'test-key-1' })
      .setIssuer('https://chat.cherry.fun')
      .setIssuedAt(Math.floor(Date.now() / 1000) - 3_600)
      .setExpirationTime(Math.floor(Date.now() / 1000) - 600)
      .sign(signingKey);
    await expect(verify(stale)).rejects.toThrow();
  });

  it('rejects a token minted for a different app', async () => {
    const other = await mint({ sub: WALLET }, { appId: 'some-other-app' });
    await expect(verify(other)).rejects.toThrow();
  });

  it('rejects structural garbage', async () => {
    await expect(verify('not.a.jwt')).rejects.toThrow();
    await expect(verify('')).rejects.toThrow();
  });

  it('rejects an unsigned "alg: none" token', async () => {
    const header = Buffer.from(JSON.stringify({ alg: 'none', typ: 'JWT' })).toString(
      'base64url',
    );
    const payload = Buffer.from(
      JSON.stringify({ sub: WALLET, app_id: APP_ID, exp: Date.now() / 1000 + 300 }),
    ).toString('base64url');
    await expect(verify(`${header}.${payload}.`)).rejects.toThrow();
  });

  it('carries the signed blink params, and only the signed ones', async () => {
    const token = await mint({
      sub: WALLET,
      room_id: 'room-7',
      message_id: 'msg-123',
      route: '/result',
      params: { kind: 'compound', rigs: '54', compounded: '12' },
    });
    const session = await verify(token);
    expect(session.messageId).toBe('msg-123');
    expect(session.params).toEqual({
      kind: 'compound',
      rigs: '54',
      compounded: '12',
    });
  });
});
