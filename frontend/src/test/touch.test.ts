import { describe, expect, it } from 'vitest';
import { fireEvent } from '@testing-library/svelte';
describe('jsdom touch', () => {
  it('TouchEvent exists', () => {
    expect(typeof window.TouchEvent).toBe('function');
  });
  it('touches carried', () => {
    const el = document.body;
    let seen: unknown = null;
    el.addEventListener('touchstart', (e) => { seen = (e as TouchEvent).touches; });
    fireEvent.touchStart(el, { touches: [{ clientX: 1, clientY: 2 }] });
    expect(seen).toBeTruthy();
  });
});
