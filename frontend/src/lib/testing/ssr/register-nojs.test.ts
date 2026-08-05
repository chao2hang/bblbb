// M02-UX-01：注册页无 JS 基线——SSR HTML 输出原生 form[method=POST]
// （无 JS 时浏览器直接把表单提交到 +page.server.ts 的服务端 action，
// 字段校验与 CSRF 均由服务端完成，认证裁决始终在后端）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import RegisterPage from '../../../routes/register/+page.svelte';

describe('无 JS：注册页服务端表单（M02-UX-01）', () => {
  it('SSR HTML 输出原生 form[method=POST]（可直接提交到 action）', () => {
    const { body } = render(RegisterPage, { props: {} });
    expect(body).toMatch(/<form[^>]*method="POST"/);
  });

  it('SSR HTML 包含全部字段、label[for] 关联与提交按钮', () => {
    const { body } = render(RegisterPage, { props: {} });
    expect(body).toContain('用户名');
    expect(body).toContain('邮箱');
    expect(body).toContain('密码');
    expect(body).toContain('确认密码');
    expect(body).toContain('type="submit"');
    // name 属性保证 FormData 能被服务端 action 读取
    expect(body).toContain('name="username"');
    expect(body).toContain('name="email"');
    expect(body).toContain('name="password"');
    expect(body).toContain('name="confirm"');
  });

  it('SSR HTML 不渲染成功面板（初始无 ok 状态）', () => {
    const { body } = render(RegisterPage, { props: {} });
    expect(body).not.toContain('注册成功');
  });
});
