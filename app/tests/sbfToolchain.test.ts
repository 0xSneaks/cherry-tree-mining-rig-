/**
 * The SBF build must use the pinned runner (cargo-build-sbf >= 4.1.0) with
 * platform-tools v1.52. An old 2.3.0 runner from PATH cannot parse
 * edition-2024 crates, so it must be refused, not silently used.
 */
import { execFileSync } from 'node:child_process';
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = join(import.meta.dirname, '..', '..');
const lib = join(root, 'scripts', 'lib', 'sbf.sh');

function fakeRunner(version: string): string {
  const dir = mkdtempSync(join(tmpdir(), 'sbf-'));
  const bin = join(dir, 'cargo-build-sbf');
  writeFileSync(bin, `#!/bin/sh\necho "solana-cargo-build-sbf ${version}"\necho "platform-tools v1.52"\n`);
  chmodSync(bin, 0o755);
  return bin;
}

function resolve(bin: string): string {
  return execFileSync(
    'bash',
    ['-c', `source "${lib}" && resolve_cargo_build_sbf && echo "$CARGO_BUILD_SBF_VERSION"`],
    { env: { ...process.env, CARGO_BUILD_SBF: bin }, encoding: 'utf8', stdio: 'pipe' },
  ).trim();
}

describe('SBF toolchain pin', () => {
  it('pins platform-tools v1.52 and a 4.1.0 minimum', () => {
    const sh = readFileSync(lib, 'utf8');
    expect(sh).toContain('SBF_TOOLS_VERSION="v1.52"');
    expect(sh).toContain('SBF_MIN_CARGO_BUILD_SBF="4.1.0"');
    expect(sh).toMatch(/--tools-version "\$SBF_TOOLS_VERSION"/);
  });

  it('refuses the old 2.3.0 runner', () => {
    expect(() => resolve(fakeRunner('2.3.0'))).toThrow();
  });

  it('accepts 4.1.0 and newer', () => {
    expect(resolve(fakeRunner('4.1.0'))).toBe('4.1.0');
    expect(resolve(fakeRunner('4.10.2'))).toBe('4.10.2');
  });

  it('build scripts never use a bare cargo build-sbf or anchor build', () => {
    for (const f of ['devnet-deploy.sh', 'build-sbf.sh']) {
      const sh = readFileSync(join(root, 'scripts', f), 'utf8')
        .split('\n')
        .filter((l) => !l.trim().startsWith('#') && !l.trim().startsWith('echo'))
        .join('\n');
      expect(sh, f).not.toMatch(/cargo build-sbf|anchor build/);
      expect(sh, f).toContain('resolve_cargo_build_sbf');
    }
  });

  it('deploy still writes the IDL anchor build used to produce', () => {
    const sh = readFileSync(join(root, 'scripts', 'devnet-deploy.sh'), 'utf8');
    expect(sh).toContain(
      'anchor idl build -p sol_rigs -o target/idl/sol_rigs.json -t target/types/sol_rigs.ts',
    );
    // IDL after the SBF build, both before deploy.
    const build = sh.indexOf('\nbuild_sbf ||');
    const idl = sh.indexOf('\nanchor idl build');
    const deploy = sh.indexOf('\nanchor deploy');
    expect(build).toBeGreaterThan(-1);
    expect(idl).toBeGreaterThan(build);
    expect(deploy).toBeGreaterThan(idl);
  });
});
