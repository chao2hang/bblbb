pub mod identity;
pub mod login;
pub mod mfa;
pub mod password;
pub mod password_reset;
pub mod preauth;
pub mod registration;
pub mod resend;
pub mod session;
pub mod token;
pub mod verification;

pub use identity::{normalize_email, normalize_username};
pub use login::{login_user, LoginError, LoginLimits, LoginOutcome};
pub use mfa::{
    base32_decode, base32_encode, begin_enrollment, cancel_enrollment, confirm_enrollment,
    consume_recovery_code, decrypt_secret, encrypt_secret, generate_recovery_codes,
    generate_totp_secret, otpauth_uri, totp_at, verify_totp, verify_totp_login, MfaError,
    TotpChallenge, VerifyTotpOutcome, RECOVERY_CODE_BYTES, RECOVERY_CODE_COUNT, TOTP_DIGITS,
    TOTP_PERIOD_SECS, TOTP_SECRET_BYTES,
};
pub use password::{hash_password, verify_password, VerifyResult};
pub use password_reset::{
    confirm_password_reset, request_password_reset, ConfirmResetError, ConfirmResetOutcome,
    PasswordResetLimits, RequestResetError, RequestResetOutcome,
};
pub use preauth::{
    build_clear_preauth_cookie, build_preauth_cookie, issue_preauth, resolve_preauth,
    PREAUTH_COOKIE_NAME, PREAUTH_TTL_MS,
};
pub use registration::{register_user, RegisterUserError, RegistrationOutcome};
pub use resend::{resend_verification_email, ResendError, ResendLimits, ResendOutcome};
pub use session::{
    is_step_up_required_for_session, list_sessions, mark_step_up, revoke_all_sessions,
    revoke_session_by_id, rotate_session, step_up_required, AuthSession, DeviceSession,
    SessionUser, DEFAULT_STEP_UP_WINDOW_SECS,
};
pub use token::{generate_token, hash_token};
pub use verification::{verify_email_token, VerifyEmailError, VerifyEmailOutcome};
