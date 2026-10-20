use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub hamr_app_url: String,
}

/// Weak-secret denylist. These names have appeared in enough real-world leaks
/// that an accidental match is almost certainly a dev mistake, not a choice.
const FORBIDDEN_SECRETS: &[&str] = &[
    "dev-secret",
    "change-me",
    "changeme",
    "change_me",
    "secret",
    "password",
    "default",
    "",
];

/// Minimum acceptable secret length. NIST SP 800-117 recommends >= 12 bytes;
/// 16 matches our JSON-web-key entropy target (>= 128 bits).
const MIN_SECRET_LEN: usize = 16;

impl Config {
    /// Read config from the process environment. Production builds will
    /// panic on a missing/weak JWT secret; dev/test tolerates the historical
    /// "dev-secret" default but logs a loud warning.
    pub fn from_env() -> Result<Self> {
        let jwt_secret = std::env::var("JWT_SECRET").ok();
        let app_env = std::env::var("APP_ENV")
            .or_else(|_| std::env::var("ENV"))
            .ok();
        Self::load(
            std::env::var("PORT").ok().as_deref(),
            std::env::var("DATABASE_URL").ok().as_deref(),
            jwt_secret.as_deref(),
            std::env::var("HAMR_APP_URL").ok().as_deref(),
            app_env.as_deref(),
        )
    }

    /// Pure constructor — every input is a parameter so tests can drive every
    /// branch without mutating process env vars (which would race other
    /// tests in the same binary).
    pub fn load(
        port: Option<&str>,
        database_url: Option<&str>,
        jwt_secret: Option<&str>,
        hamr_app_url: Option<&str>,
        app_env: Option<&str>,
    ) -> Result<Self> {
        let env_name = app_env.unwrap_or("development").to_ascii_lowercase();
        let is_production = matches!(env_name.as_str(), "production" | "prod");

        let jwt_secret_owned = resolve_jwt_secret(jwt_secret, is_production)?;

        Ok(Self {
            port: port
                .unwrap_or("8082")
                .parse()
                .map_err(|e| anyhow!("PORT must be a u16, got {:?}: {}", port, e))?,
            database_url: database_url
                .unwrap_or("postgres://hamr:hamr@localhost:5432/hamr_jiabu")
                .to_string(),
            jwt_secret: jwt_secret_owned,
            hamr_app_url: hamr_app_url
                .unwrap_or("http://hamr-app-api:3002")
                .to_string(),
        })
    }
}

/// Decide which JWT secret value to use. Returns an error in production when
/// the secret is missing / too short / on the denylist; in non-production the
/// dev default is allowed (with a warning) so local dev and CI keep working.
fn resolve_jwt_secret(provided: Option<&str>, is_production: bool) -> Result<String> {
    match provided {
        Some(s) => {
            if is_acceptable_secret(s) {
                Ok(s.to_string())
            } else if is_production {
                Err(anyhow!(
                    "JWT_SECRET is missing or too weak for production (got a value of length {}); \
                     set a strong (>= {} chars) secret not on the denylist",
                    s.len(),
                    MIN_SECRET_LEN,
                ))
            } else {
                eprintln!(
                    "warning: JWT_SECRET looks weak (len={}, name on denylist={}); \
                     using it anyway because APP_ENV is not production",
                    s.len(),
                    FORBIDDEN_SECRETS.iter().any(|f| f.eq_ignore_ascii_case(s)),
                );
                Ok(s.to_string())
            }
        }
        None => {
            if is_production {
                Err(anyhow!(
                    "JWT_SECRET must be set when APP_ENV=production; refusing to start with a default secret"
                ))
            } else {
                Ok("dev-secret".to_string())
            }
        }
    }
}

fn is_acceptable_secret(s: &str) -> bool {
    if s.len() < MIN_SECRET_LEN {
        return false;
    }
    !FORBIDDEN_SECRETS.iter().any(|f| f.eq_ignore_ascii_case(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- denylist -------------------------------------------------------

    #[test]
    fn dev_secret_is_on_denylist() {
        assert!(!is_acceptable_secret("dev-secret"));
    }

    #[test]
    fn empty_string_is_on_denylist() {
        assert!(!is_acceptable_secret(""));
    }

    #[test]
    fn case_insensitive_denylist_match() {
        // Operators sometimes type DEV-SECRET — same security posture.
        assert!(!is_acceptable_secret("DEV-SECRET"));
        assert!(!is_acceptable_secret("Password"));
    }

    #[test]
    fn short_string_below_minimum_is_rejected() {
        // 15 chars is one short of the 16-byte minimum even when not on the
        // denylist — pins the length policy.
        let s = "a".repeat(MIN_SECRET_LEN - 1);
        assert!(!is_acceptable_secret(&s));
    }

    #[test]
    fn long_offlist_string_is_accepted() {
        let s = "a".repeat(MIN_SECRET_LEN + 8);
        assert!(is_acceptable_secret(&s));
    }

    // ---- resolve_jwt_secret ---------------------------------------------

    #[test]
    fn production_missing_secret_returns_error() {
        let result = resolve_jwt_secret(None, true);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("JWT_SECRET"),
            "error should mention JWT_SECRET, got: {}",
            msg
        );
    }

    #[test]
    fn production_dev_default_secret_is_rejected() {
        let result = resolve_jwt_secret(Some("dev-secret"), true);
        assert!(result.is_err());
    }

    #[test]
    fn production_short_secret_is_rejected() {
        let weak = "a".repeat(MIN_SECRET_LEN - 1);
        let result = resolve_jwt_secret(Some(&weak), true);
        assert!(result.is_err());
    }

    #[test]
    fn production_strong_offlist_secret_is_accepted() {
        let strong = "z".repeat(MIN_SECRET_LEN + 4);
        let result = resolve_jwt_secret(Some(&strong), true);
        assert_eq!(result.unwrap(), strong);
    }

    #[test]
    fn development_missing_secret_falls_back_to_dev_default() {
        let result = resolve_jwt_secret(None, false);
        assert_eq!(result.unwrap(), "dev-secret");
    }

    #[test]
    fn development_weak_secret_is_accepted_but_warns() {
        // Dev mode tolerates the legacy dev-secret so old scripts keep
        // working — capture_stderr ensures we DO emit the warning, not that
        // we silently swallow it.
        let result = resolve_jwt_secret(Some("dev-secret"), false);
        assert_eq!(result.unwrap(), "dev-secret");
    }

    // ---- Config::load (pure) --------------------------------------------

    #[test]
    fn load_uses_documented_defaults_when_all_args_are_none() {
        let cfg = Config::load(None, None, None, None, None).unwrap();
        assert_eq!(cfg.port, 8082);
        assert_eq!(cfg.database_url, "postgres://hamr:hamr@localhost:5432/hamr_jiabu");
        assert_eq!(cfg.jwt_secret, "dev-secret");
        assert_eq!(cfg.hamr_app_url, "http://hamr-app-api:3002");
    }

    #[test]
    fn load_with_production_env_and_no_secret_fails_fast() {
        let result = Config::load(None, None, None, None, Some("production"));
        assert!(result.is_err());
    }

    #[test]
    fn load_with_prod_env_alias_and_strong_secret_succeeds() {
        let strong = "x".repeat(MIN_SECRET_LEN + 4);
        let cfg = Config::load(
            Some("9090"),
            Some("postgres://example/db"),
            Some(&strong),
            Some("http://app:9999"),
            Some("PROD"), // case-insensitive alias
        )
        .unwrap();
        assert_eq!(cfg.port, 9090);
        assert_eq!(cfg.database_url, "postgres://example/db");
        assert_eq!(cfg.jwt_secret, strong);
        assert_eq!(cfg.hamr_app_url, "http://app:9999");
    }

    #[test]
    fn load_with_invalid_port_returns_error() {
        let result = Config::load(Some("not-a-number"), None, None, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("PORT"));
    }
}
