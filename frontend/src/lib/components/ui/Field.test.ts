// M00-FRONTEND-07：Field 表单错误关联（label[for] / aria-describedby / aria-invalid / role=alert）。
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import Field from './Field.svelte';
import FieldHarness from '../../../test/FieldHarness.svelte';
import { getFormControl, expectErrorAssociation } from '$lib/testing/a11y';

describe('Field（表单错误关联）', () => {
  it('label 通过 for 关联输入框', () => {
    const { container } = render(FieldHarness, { label: '邮箱' });
    const { label, input } = getFormControl(container, '邮箱');
    expect(label).toHaveAttribute('for', 'f-email');
    expect(input).toHaveAttribute('id', 'f-email');
  });

  it('错误态：错误文案带 role=alert 且与控件正确关联', () => {
    const { container } = render(FieldHarness, { label: '邮箱', error: '邮箱格式不正确' });
    const input = container.querySelector('#f-email') as HTMLElement;
    expectErrorAssociation(input, '邮箱格式不正确');
    const err = container.querySelector('#f-email-error');
    expect(err).toHaveAttribute('role', 'alert');
    expect(err).toHaveTextContent('邮箱格式不正确');
  });

  it('提示态：hint 通过 aria-describedby 关联，控件无 aria-invalid', () => {
    const { container } = render(FieldHarness, { label: '昵称', hint: '2-20 个字符' });
    const input = container.querySelector('#f-email') as HTMLElement;
    expect(input).toHaveAttribute('aria-describedby', 'f-email-hint');
    expect(input).not.toHaveAttribute('aria-invalid');
    const hint = container.querySelector('#f-email-hint');
    expect(hint).toHaveTextContent('2-20 个字符');
  });

  it('无错误无提示时既无描述锚点也无错误通知', () => {
    const { container } = render(FieldHarness, { label: '城市' });
    const input = container.querySelector('#f-email') as HTMLElement;
    expect(input).not.toHaveAttribute('aria-describedby');
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });

  it('id 缺省时自动生成唯一 id', () => {
    const { container } = render(Field, { label: '标题' });
    const label = container.querySelector('label')!;
    const id = label.getAttribute('for');
    expect(id).toMatch(/^bblbb-/);
  });
});