pub mod identity;
pub mod login;
pub mod password;
pub mod password_reset;
pub mod registration;
pub mod resend;
pub mod session;
pub mod token;
pub mod verification;

pub use identity::{normalize_email, normalize_username};
pub use login::{login_user, LoginError, LoginLimits, LoginOutcome};
pub use password::{hash_password, verify_password, VerifyResult};
pub use password_reset::{
    confirm_password_reset, request_password_reset, ConfirmResetError, ConfirmResetOutcome,
    PasswordResetLimits, RequestResetError, RequestResetOutcome,
};
pub use registration::{register_user, RegisterUserError, RegistrationOutcome};
pub use resend::{resend_verification_email, ResendError, ResendLimits, ResendOutcome};
pub use session::{
    list_sessions, revoke_all_sessions, revoke_session_by_id, rotate_session, AuthSession,
    DeviceSession, SessionUser,
};
pub use token::{generate_token, hash_token};
pub use verification::{verify_email_token, VerifyEmailError, VerifyEmailOutcome};
