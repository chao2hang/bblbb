pub mod identity;
pub mod password;
pub mod password_reset;
pub mod registration;
pub mod resend;
pub mod session;
pub mod token;
pub mod verification;

pub use identity::{normalize_email, normalize_username};
pub use password::{hash_password, verify_password, VerifyResult};
pub use password_reset::{
    confirm_password_reset, request_password_reset, ConfirmResetError, ConfirmResetOutcome,
    PasswordResetLimits, RequestResetError, RequestResetOutcome,
};
pub use registration::{register_user, RegisterUserError, RegistrationOutcome};
pub use resend::{resend_verification_email, ResendError, ResendLimits, ResendOutcome};
pub use session::{AuthSession, SessionUser};
pub use token::{generate_token, hash_token};
pub use verification::{verify_email_token, VerifyEmailError, VerifyEmailOutcome};
