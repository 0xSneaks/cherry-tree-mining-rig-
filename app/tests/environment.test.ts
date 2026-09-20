/**
 * Spec §12.20: `isInsideCherry({ strict: true })` returns false in a plain
 * browser and in a non-Cherry wallet in-app browser.
 *
 * This matters because the embedded path renders no connect-wallet UI. A
 * false positive in a wallet's in-app browser would leave the user staring at
 * a mine they cannot operate, so the strictness is asserted directly against
 * the environments that trip the non-strict fallbacks.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { getCherryEnvironment, isInsideCherry } from '@cherrydotfun/miniapp-sdk';

type FakeWindow = {
  location: { search: string };
  parent?: unknown;
  __cherry?: boolean;
  ReactNativeWebView?: unknown;
};

function setWindow(w: FakeWindow | undefined): void {
  const g = globalThis as unknown as { window?: FakeWindow };
  if (w === undefined) delete g.window;
  else {
    w.parent ??= w;
    g.window = w;
  }
}

afterEach(() => setWindow(undefined));

describe('strict environment detection', () => {
  it('is false in a plain browser', () => {
    setWindow({ location: { search: '' } });
    expect(isInsideCherry({ strict: true })).toBe(false);
    expect(getCherryEnvironment({ strict: true }).platform).toBe('standalone');
  });

  it('is false in a non-Cherry wallet in-app browser (React Native WebView)', () => {
    // The exact shape the non-strict fallback would misread as Cherry.
    setWindow({ location: { search: '' }, ReactNativeWebView: { postMessage() {} } });
    expect(isInsideCherry({ strict: true })).toBe(false);
    expect(isInsideCherry()).toBe(true); // the fallback this strictness exists to defeat
  });

  it('is false in an unrelated iframe', () => {
    const w: FakeWindow = { location: { search: '' } };
    w.parent = { differentWindow: true };
    setWindow(w);
    expect(isInsideCherry({ strict: true })).toBe(false);
    expect(isInsideCherry()).toBe(true); // ditto
  });

  it('is true in the Cherry WebView', () => {
    setWindow({ location: { search: '' }, __cherry: true });
    expect(isInsideCherry({ strict: true })).toBe(true);
    expect(getCherryEnvironment({ strict: true }).platform).toBe('webview');
  });

  it('is true in the Cherry web iframe', () => {
    setWindow({ location: { search: '?cherry_embed=1' } });
    expect(isInsideCherry({ strict: true })).toBe(true);
    expect(getCherryEnvironment({ strict: true }).platform).toBe('iframe');
  });

  it('is false during server rendering, where there is no window at all', () => {
    setWindow(undefined);
    expect(isInsideCherry({ strict: true })).toBe(false);
  });
});
