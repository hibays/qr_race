//! Token + TOTP authentication (the 2FA the client waits on).
//!
//! A user is provisioned out of band with a bearer token and a base32 TOTP
//! secret. Connecting proves both: the token identifies the account, the code
//! proves possession of the secret. A code is accepted once — replaying the
//! same step is refused even inside the skew window.

use std::sync::Arc;

use qr_race_core::UserId;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

use crate::clock::Clock;
use crate::totp;

/// Shorter tokens are provisioned by mistake, not by choice.
pub const MIN_TOKEN_LEN: usize = 16;

/// Ten bytes is the shortest secret worth calling a secret.
pub const MIN_TOTP_SECRET_BYTES: usize = 10;

/// Twelve hours; long enough for an event, short enough to matter.
pub const DEFAULT_SESSION_TTL_MS: u64 = 12 * 60 * 60 * 1000;

/// Wrong codes allowed before an account is locked for a minute.
pub const MAX_FAILED_ATTEMPTS: u32 = 5;

/// How long a locked account must wait before trying again.
pub const LOCKOUT_MS: u64 = 60_000;

/// One accepted connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub user_id: UserId,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
}

impl Session {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("token must be at least {MIN_TOKEN_LEN} characters")]
    WeakToken,

    #[error("totp secret is invalid: {0}")]
    BadSecret(&'static str),

    #[error("account or token is already provisioned")]
    DuplicateAccount,

    #[error("unknown token")]
    UnknownToken,

    #[error("totp code is invalid or expired")]
    InvalidTotp,

    #[error("totp code was already used")]
    ReplayedTotp,

    #[error("too many failed attempts; retry after {retry_after_ms} ms")]
    Locked { retry_after_ms: u64 },
}

struct Account {
    user_id: UserId,
    token_hash: [u8; 32],
    totp_secret: Vec<u8>,
    last_step: Option<u64>,
    /// Admin is a server-governance flag; it is not a room role.
    admin: bool,
    failed_attempts: u32,
    locked_until_ms: u64,
}

pub struct AuthService {
    accounts: Vec<Account>,
    clock: Arc<dyn Clock>,
    session_ttl_ms: u64,
    skew_steps: u32,
    digits: u32,
}

impl AuthService {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            accounts: Vec::new(),
            clock,
            session_ttl_ms: DEFAULT_SESSION_TTL_MS,
            skew_steps: 1,
            digits: totp::DEFAULT_DIGITS,
        }
    }

    pub fn with_session_ttl(mut self, ttl_ms: u64) -> Self {
        self.session_ttl_ms = ttl_ms;
        self
    }

    pub fn with_skew(mut self, skew_steps: u32) -> Self {
        self.skew_steps = skew_steps;
        self
    }

    pub fn with_digits(mut self, digits: u32) -> Self {
        self.digits = digits;
        self
    }

    /// Provisions a user out of band. The token is stored only as a hash.
    pub fn provision(
        &mut self,
        user_id: UserId,
        token: &str,
        totp_secret_base32: &str,
    ) -> Result<(), AuthError> {
        self.provision_as(user_id, token, totp_secret_base32, false)
    }

    /// Same, but marks the account as a server admin.
    pub fn provision_as(
        &mut self,
        user_id: UserId,
        token: &str,
        totp_secret_base32: &str,
        admin: bool,
    ) -> Result<(), AuthError> {
        if token.len() < MIN_TOKEN_LEN {
            return Err(AuthError::WeakToken);
        }
        let totp_secret = decode_base32_secret(totp_secret_base32)?;
        let token_hash = hash_token(token);

        let duplicate = self.accounts.iter().any(|account| {
            account.user_id == user_id || bool::from(account.token_hash.ct_eq(&token_hash))
        });
        if duplicate {
            return Err(AuthError::DuplicateAccount);
        }

        self.accounts.push(Account {
            user_id,
            token_hash,
            totp_secret,
            last_step: None,
            admin,
            failed_attempts: 0,
            locked_until_ms: 0,
        });
        Ok(())
    }

    /// True when this account carries the server-governance flag.
    pub fn is_admin(&self, user_id: &UserId) -> bool {
        self.accounts
            .iter()
            .any(|account| account.user_id == *user_id && account.admin)
    }

    /// Token-only login. TOTP remains available through [`authenticate`] for
    /// old clients, but the product no longer asks users for a second factor.
    pub fn authenticate_token(&mut self, token: &str) -> Result<Session, AuthError> {
        let token_hash = hash_token(token);
        let index = self
            .accounts
            .iter()
            .position(|account| bool::from(account.token_hash.ct_eq(&token_hash)))
            .ok_or(AuthError::UnknownToken)?;
        let now = self.clock.now_ms();
        let account = &mut self.accounts[index];
        if account.locked_until_ms != 0 && now >= account.locked_until_ms {
            account.locked_until_ms = 0;
            account.failed_attempts = 0;
        }
        if account.locked_until_ms > now {
            return Err(AuthError::Locked {
                retry_after_ms: account.locked_until_ms - now,
            });
        }
        account.failed_attempts = 0;
        account.locked_until_ms = 0;
        Ok(Session {
            id: format!("s-{}", hex(&random_bytes::<32>())),
            user_id: account.user_id.clone(),
            created_at_ms: now,
            expires_at_ms: now + self.session_ttl_ms,
        })
    }

    /// Verifies token and code, then issues a session.
    pub fn authenticate(&mut self, token: &str, code: &str) -> Result<Session, AuthError> {
        let token_hash = hash_token(token);
        let index = self
            .accounts
            .iter()
            .position(|account| bool::from(account.token_hash.ct_eq(&token_hash)))
            .ok_or(AuthError::UnknownToken)?;

        let now = self.clock.now_ms();
        {
            let account = &mut self.accounts[index];
            // An expired lock starts from a clean slate, otherwise one typo would
            // lock the account again immediately.
            if account.locked_until_ms != 0 && now >= account.locked_until_ms {
                account.locked_until_ms = 0;
                account.failed_attempts = 0;
            }
            if account.locked_until_ms > now {
                return Err(AuthError::Locked {
                    retry_after_ms: account.locked_until_ms - now,
                });
            }
        }

        let step = {
            let account = &self.accounts[index];
            totp::verify(
                &account.totp_secret,
                code,
                now,
                self.digits,
                self.skew_steps,
            )
        };

        let Some(step) = step else {
            record_failure(&mut self.accounts[index], now);
            return Err(AuthError::InvalidTotp);
        };

        let account = &mut self.accounts[index];
        if account.last_step.is_some_and(|last| step <= last) {
            record_failure(account, now);
            return Err(AuthError::ReplayedTotp);
        }
        account.failed_attempts = 0;
        account.locked_until_ms = 0;
        account.last_step = Some(step);

        Ok(Session {
            id: format!("s-{}", hex(&random_bytes::<32>())),
            user_id: account.user_id.clone(),
            created_at_ms: now,
            expires_at_ms: now.saturating_add(self.session_ttl_ms),
        })
    }
}

/// A fresh bearer token for provisioning (256 bits, hex).
pub fn generate_token() -> String {
    hex(&random_bytes::<32>())
}

/// A fresh base32 TOTP secret (160 bits, what authenticator apps expect).
pub fn generate_totp_secret_base32() -> String {
    data_encoding::BASE32_NOPAD.encode(&random_bytes::<20>())
}

fn decode_base32_secret(input: &str) -> Result<Vec<u8>, AuthError> {
    let normalized: String = input.trim().trim_end_matches('=').to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(AuthError::BadSecret("secret is empty"));
    }
    let bytes = data_encoding::BASE32_NOPAD
        .decode(normalized.as_bytes())
        .map_err(|_| AuthError::BadSecret("secret is not base32"))?;
    if bytes.len() < MIN_TOTP_SECRET_BYTES {
        return Err(AuthError::BadSecret("secret is shorter than 10 bytes"));
    }
    Ok(bytes)
}

fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn record_failure(account: &mut Account, now_ms: u64) {
    account.failed_attempts = account.failed_attempts.saturating_add(1);
    if account.failed_attempts >= MAX_FAILED_ATTEMPTS {
        account.locked_until_ms = now_ms.saturating_add(LOCKOUT_MS);
    }
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    for byte in &mut bytes {
        *byte = rand::random::<u8>();
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;

    const SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
    const TOKEN: &str = "0123456789abcdef-token";

    fn service(clock: &ManualClock) -> AuthService {
        let clock: Arc<dyn Clock> = Arc::new(clock.clone());
        let mut service = AuthService::new(clock);
        service
            .provision(UserId("u-host".into()), TOKEN, SECRET)
            .unwrap();
        service
    }

    fn code_at(now_ms: u64) -> String {
        totp::current_code(SECRET, now_ms, totp::DEFAULT_DIGITS).unwrap()
    }

    #[test]
    fn token_only_login_issues_a_session_without_a_code() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        let session = service.authenticate_token(TOKEN).unwrap();
        assert_eq!(session.user_id, UserId("u-host".into()));
        assert!(session.expires_at_ms > clock.now_ms());
    }

    #[test]
    fn token_and_code_issue_a_session() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        let session = service
            .authenticate(TOKEN, &code_at(clock.now_ms()))
            .unwrap();
        assert_eq!(session.user_id, UserId("u-host".into()));
        assert_eq!(session.created_at_ms, clock.now_ms());
        assert!(session.id.starts_with("s-"));
        assert!(!session.is_expired(session.created_at_ms));
        assert!(session.is_expired(session.expires_at_ms));
    }

    #[test]
    fn a_wrong_token_is_rejected() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        assert_eq!(
            service.authenticate("not-the-token-000000", &code_at(clock.now_ms())),
            Err(AuthError::UnknownToken)
        );
    }

    #[test]
    fn a_wrong_code_is_rejected() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        let other = totp::current_code("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", 1_700_000_000_000, 6);
        assert_eq!(
            service.authenticate(TOKEN, &other.unwrap()),
            Err(AuthError::InvalidTotp)
        );
    }

    #[test]
    fn the_same_step_cannot_be_replayed() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        let code = code_at(clock.now_ms());
        service.authenticate(TOKEN, &code).unwrap();
        assert_eq!(
            service.authenticate(TOKEN, &code),
            Err(AuthError::ReplayedTotp)
        );
    }

    #[test]
    fn the_next_step_is_accepted() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        service
            .authenticate(TOKEN, &code_at(clock.now_ms()))
            .unwrap();
        clock.advance(totp::STEP_SECONDS * 1000);
        service
            .authenticate(TOKEN, &code_at(clock.now_ms()))
            .unwrap();
    }

    #[test]
    fn a_previous_step_is_accepted_within_skew() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        let previous = code_at(clock.now_ms() - totp::STEP_SECONDS * 1000);
        service.authenticate(TOKEN, &previous).unwrap();
        // The same code is now recorded as used, so it must still be refused.
        assert_eq!(
            service.authenticate(TOKEN, &previous),
            Err(AuthError::ReplayedTotp)
        );
    }

    #[test]
    fn weak_tokens_and_bad_secrets_are_refused() {
        let clock = ManualClock::new(1_700_000_000_000);
        let clock: Arc<dyn Clock> = Arc::new(clock);
        let mut service = AuthService::new(clock);

        assert_eq!(
            service.provision(UserId("u".into()), "short", SECRET),
            Err(AuthError::WeakToken)
        );
        assert_eq!(
            service.provision(UserId("u".into()), TOKEN, "not base32!!"),
            Err(AuthError::BadSecret("secret is not base32"))
        );
        assert_eq!(
            service.provision(UserId("u".into()), TOKEN, "MZXW6==="),
            Err(AuthError::BadSecret("secret is shorter than 10 bytes"))
        );
    }

    #[test]
    fn duplicate_users_and_tokens_are_refused() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);
        assert_eq!(
            service.provision(UserId("u-host".into()), "another-token-0000000", SECRET),
            Err(AuthError::DuplicateAccount)
        );
        assert_eq!(
            service.provision(UserId("u-other".into()), TOKEN, SECRET),
            Err(AuthError::DuplicateAccount)
        );
    }

    #[test]
    fn generated_secrets_and_tokens_are_usable() {
        let token = generate_token();
        let secret = generate_totp_secret_base32();
        assert_eq!(token.len(), 64);
        assert!(secret.len() >= 32);

        let clock = ManualClock::new(1_700_000_000_000);
        let clock: Arc<dyn Clock> = Arc::new(clock);
        let mut service = AuthService::new(clock);
        service
            .provision(UserId("u".into()), &token, &secret)
            .unwrap();
        let code = totp::current_code(&secret, 1_700_000_000_000, 6).unwrap();
        assert!(service.authenticate(&token, &code).is_ok());
    }

    #[test]
    fn failed_codes_lock_the_account_temporarily() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);

        for _ in 0..MAX_FAILED_ATTEMPTS {
            assert_eq!(
                service.authenticate(TOKEN, "abcdef"),
                Err(AuthError::InvalidTotp)
            );
        }

        let code = totp::current_code(SECRET, clock.now_ms(), 6).unwrap();
        assert!(matches!(
            service.authenticate(TOKEN, &code),
            Err(AuthError::Locked { .. })
        ));

        clock.advance(LOCKOUT_MS);
        let code = totp::current_code(SECRET, clock.now_ms(), 6).unwrap();
        assert!(service.authenticate(TOKEN, &code).is_ok());
    }

    #[test]
    fn a_success_clears_the_failure_count() {
        let clock = ManualClock::new(1_700_000_000_000);
        let mut service = service(&clock);

        for _ in 0..2 {
            assert_eq!(
                service.authenticate(TOKEN, "abcdef"),
                Err(AuthError::InvalidTotp)
            );
        }
        let code = totp::current_code(SECRET, clock.now_ms(), 6).unwrap();
        assert!(service.authenticate(TOKEN, &code).is_ok());

        clock.advance(30_000);
        for _ in 0..(MAX_FAILED_ATTEMPTS - 1) {
            assert_eq!(
                service.authenticate(TOKEN, "abcdef"),
                Err(AuthError::InvalidTotp)
            );
        }
        let code = totp::current_code(SECRET, clock.now_ms(), 6).unwrap();
        assert!(
            service.authenticate(TOKEN, &code).is_ok(),
            "the earlier success reset the counter"
        );
    }
}
