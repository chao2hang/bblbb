// M02-UX-01：注册字段校验（与后端 backend/src/domain/registration.rs 规则一致）。
//
// 后端仍是权威复检（validate_register + 唯一约束）；本模块只在表单 action
// 内即时产出字段错误，用于前端字段级错误关联（aria-describedby），避免把
// 后端英文 detail 直接展示给用户。规则漂移时以后端为准并同步本文件。

export type RegisterField = 'username' | 'email' | 'password' | 'confirm';

export interface RegistrationInput {
  username: string;
  email: string;
  password: string;
  confirm: string;
}

const USERNAME_PATTERN = /^[A-Za-z0-9_-]+$/;
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** 校验注册表单，返回字段 → 中文错误文案（无错误则无对应键）。 */
export function validateRegistration(
  input: RegistrationInput
): Partial<Record<RegisterField, string>> {
  const errors: Partial<Record<RegisterField, string>> = {};
  const { username, email, password, confirm } = input;

  const uname = username.trim();
  if (uname.length < 3 || uname.length > 20) {
    errors.username = '用户名需为 3-20 个字符';
  } else if (!USERNAME_PATTERN.test(uname)) {
    errors.username = '用户名只能包含字母、数字、下划线和短横线';
  }

  if (email.length > 254) {
    errors.email = '邮箱地址过长';
  } else if (!EMAIL_PATTERN.test(email)) {
    errors.email = '邮箱格式不正确';
  }

  if (password.length < 8 || password.length > 128) {
    errors.password = '密码需为 8-128 个字符';
  } else if (!/[A-Za-z]/.test(password) || !/[0-9]/.test(password)) {
    errors.password = '密码必须同时包含字母和数字';
  }

  if (password !== confirm) {
    errors.confirm = '两次输入的密码不一致';
  }

  return errors;
}
