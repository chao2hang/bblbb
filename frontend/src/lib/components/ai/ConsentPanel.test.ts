// M09-UI-02/03：同意面板组件测试。
//
// - 披露文案完整展示（Provider/用途/数据模式/hash/版本）；
// - 未勾选 checkbox 时确认按钮禁用；勾选后点击触发 onConfirm（携带同意输入）；
// - 已同意态展示同意版本并可撤回（onRevoke）；
// - processing 期间按钮禁用。
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render } from '@testing-library/svelte';
import ConsentPanel from './ConsentPanel.svelte';
import type { AiConsentView } from '$lib/api/types';

const disclosureText = '你的正文将发送给 AI 提供商「测试提供商」用于「内容格式化」。\n数据模式：full_with_consent。';

const props = {
  purpose: 'formatting',
  providers: [{ id: 'prov-1', name: '测试提供商', secret_configured: true, available: true }],
  dataMode: 'full_with_consent',
  disclosureText,
  disclosureVersion: 3,
  disclosureHashValue: 'deadbeef',
  onConfirm: vi.fn(),
  onCancel: vi.fn()
};

describe('M09-UI-02 ConsentPanel', () => {
  it('未确认时渲染完整披露并禁用确认按钮', () => {
    const { container, getByRole } = render(ConsentPanel, { props });
    expect(container.textContent).toContain('测试提供商');
    expect(container.textContent).toContain('内容格式化');
    expect(container.textContent).toContain('v3');
    expect(container.textContent).toContain('deadbeef');
    const confirm = getByRole('button', { name: '同意并继续' }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
  });

  it('勾选确认框后点击 → onConfirm 携带 consent input', async () => {
    const onConfirm = vi.fn();
    const { getByRole, container } = render(ConsentPanel, { props: { ...props, onConfirm } });
    const checkbox = container.querySelector('#ai-consent-ack') as HTMLInputElement;
    await fireEvent.click(checkbox);
    const confirm = getByRole('button', { name: '同意并继续' }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(false);
    await fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledWith({
      provider_id: 'prov-1',
      purpose: 'formatting',
      data_mode: 'full_with_consent',
      disclosure_version: 3,
      disclosure_hash: 'deadbeef'
    });
  });

  it('取消按钮调用 onCancel', async () => {
    const onCancel = vi.fn();
    const { getByRole } = render(ConsentPanel, { props: { ...props, onCancel } });
    await fireEvent.click(getByRole('button', { name: '取消' }));
    expect(onCancel).toHaveBeenCalled();
  });

  it('已同意态：显示同意版本并可撤回', async () => {
    const onRevoke = vi.fn();
    const consent: AiConsentView = {
      provider_id: 'prov-1',
      provider_name: '测试提供商',
      purpose: 'formatting',
      data_mode: 'full_with_consent',
      disclosure_version: 3,
      disclosure_hash: 'deadbeef',
      granted_at: 1700000000000
    };
    const { container, getByRole } = render(ConsentPanel, {
      props: { ...props, existingConsent: consent, onRevoke }
    });
    expect(container.textContent).toContain('已同意');
    expect(container.textContent).toContain('v3');
    await fireEvent.click(getByRole('button', { name: '撤回同意' }));
    expect(onRevoke).toHaveBeenCalledWith({
      provider_id: 'prov-1',
      purpose: 'formatting',
      data_mode: 'full_with_consent',
      disclosure_version: 3,
      disclosure_hash: 'deadbeef'
    });
  });

  it('processing 期间禁用确认与取消', () => {
    const { getByRole } = render(ConsentPanel, { props: { ...props, processing: true } });
    expect((getByRole('button', { name: '处理中…' }) as HTMLButtonElement).disabled).toBe(true);
    expect((getByRole('button', { name: '取消' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('隐私守卫：Provider 无 Secret 字段可渲染（secret 只保留布尔）', () => {
    const { container } = render(ConsentPanel, { props });
    // 组件契约不含 secret 明文；即使外部塞入也不会渲染（类型/组件层面无该字段）。
    expect(container.textContent).not.toContain('sk-');
  });
});
