/**
 * A stand-in for Cherry's JWKS endpoint, for the `/inline` blink tests.
 *
 * Generates a fresh RS256 keypair on every run, serves the public half at
 * `/jwks.json`, and writes the private half to `e2e/.tmp/keys.json` for the
 * test to sign with. Nothing is committed: no key in this repo is ever a real
 * one.
 */
import { createServer } from 'node:http';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as jose from 'jose';

const here = dirname(fileURLToPath(import.meta.url));
const PORT = Number(process.env.JWKS_PORT ?? 3519);

const { privateKey, publicKey } = await jose.generateKeyPair('RS256', {
  extractable: true,
});
const publicJwk = { ...(await jose.exportJWK(publicKey)), kid: 'e2e-1', alg: 'RS256', use: 'sig' };
const privateJwk = { ...(await jose.exportJWK(privateKey)), kid: 'e2e-1', alg: 'RS256' };

mkdirSync(join(here, '.tmp'), { recursive: true });
writeFileSync(join(here, '.tmp', 'keys.json'), JSON.stringify({ privateJwk, publicJwk }));

const body = JSON.stringify({ keys: [publicJwk] });
createServer((req, res) => {
  if (req.url?.startsWith('/jwks.json')) {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(body);
    return;
  }
  res.writeHead(404).end();
}).listen(PORT, '127.0.0.1', () => {
  console.log(`jwks stub listening on ${PORT}`);
});
