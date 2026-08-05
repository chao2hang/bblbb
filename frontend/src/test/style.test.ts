import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import LoadingState from '$lib/components/ui/LoadingState.svelte';
describe('style location', () => {
  it('where is the style', () => {
    const { container } = render(LoadingState);
    const headStyles = Array.from(document.head.querySelectorAll('style')).map(s => s.textContent ?? '');
    const anyHead = headStyles.some(t => t.includes('prefers-reduced-motion'));
    const containerText = container.querySelector('style')?.textContent ?? '';
    console.log('head styles:', headStyles.length, 'found in head:', anyHead, 'container:', JSON.stringify(containerText.slice(0, 80)));
    expect(true).toBe(true);
  });
});
