import { describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import AdminRiskPage from './+page.svelte';
import type { AdminRiskPageData } from './+page.server';

const mockData: AdminRiskPageData = {
  state: 'ok',
  version: 3,
  thresholds: {
    new_user_max_posts: 3,
    new_user_grace_secs: 604800,
    max_links: 3,
    sensitive_words: ['广告', '引流'],
    max_frequency_posts: 10,
    frequency_window_secs: 3600,
    duplicate_window_secs: 604800
  },
  error: null
};

describe('Admin Risk Policy Page - Reason Dialog Modal', () => {
  it('opens reason input modal when clicking save button', async () => {
    const { container } = render(AdminRiskPage, { props: { data: mockData, form: null } });

    // In JS mode, the card footer should only have the save button, not inline reason input
    const saveBtn = container.querySelector('.app-card__foot button');
    expect(saveBtn).not.toBeNull();
    expect(saveBtn?.textContent).toContain('保存策略');

    // Dialog should not be open initially
    expect(document.querySelector('.modal')).toBeNull();

    // Click "保存策略"
    await fireEvent.click(saveBtn!);

    // Modal dialog is now opened
    const dialog = document.querySelector('.modal');
    expect(dialog).not.toBeNull();
    expect(dialog?.textContent).toContain('保存风控策略');
    expect(dialog?.textContent).toContain('操作原因');
    expect(dialog?.textContent).toContain('确认保存');

    // Check placeholder in the dialog reason input
    const reasonInput = dialog?.querySelector('input[placeholder="如：收紧垃圾广告规则"]') as HTMLInputElement;
    expect(reasonInput).not.toBeNull();
    expect(reasonInput.value).toBe('');

    // Confirm button should be disabled when reason is empty
    const confirmBtn = Array.from(dialog?.querySelectorAll('button') ?? []).find(
      (b) => b.textContent?.trim() === '确认保存'
    ) as HTMLButtonElement;
    expect(confirmBtn).not.toBeNull();
    expect(confirmBtn.disabled).toBe(true);

    // Input reason
    await fireEvent.input(reasonInput, { target: { value: '更新防垃圾广告阈值' } });
    expect(reasonInput.value).toBe('更新防垃圾广告阈值');
    expect(confirmBtn.disabled).toBe(false);

    // Clicking cancel closes the modal
    const cancelBtn = Array.from(dialog?.querySelectorAll('button') ?? []).find(
      (b) => b.textContent?.trim() === '取消'
    );
    expect(cancelBtn).not.toBeNull();
    await fireEvent.click(cancelBtn!);

    expect(document.querySelector('.modal')).toBeNull();
  });
});
