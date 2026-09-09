// M06-UI-06/07：存储管理页 SSR——脱敏状态（Secret 不进 DOM）、env 只读、
// TTL 说明与迁移（预演/hash/回滚）提示、测试连接按钮。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminStorage from '../../../routes/admin/storage/+page.svelte';
import type { StorageConfig } from '$lib/api/types';

const config: StorageConfig = {
  backend: 's3',
  source: 'db',
  s3_endpoint: 'https://s3.amazonaws.com',
  s3_region: 'ap-southeast-1',
  s3_bucket: 'bblbb-attachments',
  s3_path_style: true,
  s3_presigned_uploads: true,
  signed_url_ttl_seconds: 300,
  upload_max_bytes: 20971520,
  secret_configured: true,
  managed_fields: ['s3_region'],
  version: 2,
  updated_at: 0
};

describe('M06-UI-06/07 存储管理 SSR', () => {
  it('渲染脱敏配置：Secret 只显示掩码，绝不出现在 DOM，且支持 S3 路径与密钥输入', () => {
    const { body } = render(AdminStorage, {
      props: { data: { config, loadError: null }, form: null }
    });
    expect(body).toContain('••••••••••');
    expect(body).toContain('S3 兼容');
    expect(body).not.toContain('s3_secret_access_key" value=');
    expect(body).toContain('name="s3_access_key_id"');
    expect(body).toContain('name="s3_secret_access_key"');
    expect(body).toContain('name="s3_public_base_url"');
    expect(body).toContain('name="reason"');
  });

  it('env 来源字段禁用（只读），显示来源徽标', () => {
    const { body } = render(AdminStorage, {
      props: { data: { config: { ...config, source: 'env' }, loadError: null }, form: null }
    });
    expect(body).toContain('部署环境（只读）');
  });

  it('path-style 勾选 + TTL 输入 + 保存/测试按钮（formaction）', () => {
    const { body } = render(AdminStorage, {
      props: { data: { config, loadError: null }, form: null }
    });
    expect(body).toContain('path-style');
    expect(body).toContain('signed_url_ttl_seconds');
    expect(body).toContain('value="300"');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/save"/);
    expect(body).toContain('formaction="?/test"');
    expect(body).toContain('测试连接');
  });

  it('M06-UI-07：TTL 只影响新签发 URL + 迁移需预演/hash/回滚（按钮禁用）', () => {
    const { body } = render(AdminStorage, {
      props: { data: { config, loadError: null }, form: null }
    });
    expect(body).toContain('TTL 修改只影响新签发的 URL');
    expect(body).toContain('预演');
    expect(body).toContain('迁移流程需在维护窗口按 Runbook 执行');
    // 切换/预演按钮禁用（disabled 属性）
    expect(body).toContain('disabled');
  });

  it('测试结果 → 脱敏诊断渲染', () => {
    const { body } = render(AdminStorage, {
      props: { data: { config, loadError: null }, form: { testResult: { ok: true, message: 'connect ok', code: null, elapsed_ms: 12 } } }
    });
    expect(body).toContain('连接成功');
    expect(body).toContain('12');
  });

  it('测试失败 → 分类与脱敏诊断（error_class + 后端）渲染', () => {
    const { body } = render(AdminStorage, {
      props: {
        data: { config, loadError: null },
        form: {
          testResult: {
            ok: false,
            message: 's3 auth failed',
            backend: 's3',
            error_class: 'auth',
            elapsed_ms: 34
          }
        }
      }
    });
    expect(body).toContain('连接失败');
    expect(body).toContain('s3');
    expect(body).toContain('auth');
  });

  it('step_up_required → 渲染重新验证表单（M02-MFA-07）', () => {
    const { body } = render(AdminStorage, {
      props: {
        data: { config, loadError: null },
        form: { message: '此操作需要重新验证身份', stepUpRequired: true, messageKind: 'error' }
      }
    });
    expect(body).toContain('需要重新验证身份');
    expect(body).toContain('name="password"');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/reauth"/);
    expect(body).toContain('重新验证');
  });

  it('保存成功 → 成功态提示（在线热生效）', () => {
    const { body } = render(AdminStorage, {
      props: {
        data: { config, loadError: null },
        form: {
          message: '存储配置已保存并立即热生效（已更新数据库与服务实例，无需重启进程）。',
          messageKind: 'success'
        }
      }
    });
    expect(body).toContain('已保存并立即热生效');
    expect(body).not.toContain('role="alert"');
  });

  it('load 错误 → 错误横幅', () => {
    const { body } = render(AdminStorage, {
      props: { data: { config: null, loadError: 'forbidden' }, form: null }
    });
    expect(body).toContain('forbidden');
  });
});
