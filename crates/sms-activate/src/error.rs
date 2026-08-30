//! Error model shared by every provider.

use crate::transport::TransportError;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("invalid API key")]
    BadKey,
    #[error("unknown action `{0}`")]
    BadAction(String),
    #[error("unknown service")]
    BadService,
    #[error("unknown country")]
    BadCountry,
    #[error("invalid status transition")]
    BadStatus,
    #[error("no numbers available")]
    NoNumbers,
    #[error("insufficient balance")]
    NoBalance,
    #[error("activation not found")]
    NoActivation,
    #[error("the activation cannot be cancelled yet")]
    EarlyCancelDenied,
    #[error("maxPrice is below the minimum{}", .min.map(|m| format!(" ({m})")).unwrap_or_default())]
    WrongMaxPrice { min: Option<f64> },
    #[error("account is banned until {until}")]
    Banned { until: String },
    #[error("validation failed for `{field}`: {message}")]
    Validation { field: String, message: String },
    /// HTTP 429 (or an equivalent token). `retry_after` is seconds when the provider says.
    #[error("rate limited{}", .retry_after.map(|s| format!(", retry after {s}s")).unwrap_or_default())]
    RateLimited { retry_after: Option<u64> },
    /// The action is not part of this provider's dialect.
    #[error("`{0}` is not supported by this provider")]
    Unsupported(&'static str),
    /// A recognised-looking protocol error code we don't map explicitly (e.g. `ERROR_SQL`).
    #[error("provider error `{0}`")]
    Other(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("unexpected response: {0}")]
    Unexpected(String),
    #[error("could not parse response: {0}")]
    Parse(String),
}

impl ApiError {
    /// Maps a standard sms-activate error token (`BAD_KEY`, `NO_NUMBERS`, `BANNED:2024-01-01 …`)
    /// to an [`ApiError`]. Returns `None` for anything that is not an error token.
    pub fn from_code(code: &str) -> Option<ApiError> {
        let code = code.trim();
        let (head, tail) = match code.split_once(':') {
            Some((h, t)) => (h, Some(t.trim())),
            None => (code, None),
        };
        Some(match head {
            "BAD_KEY" => ApiError::BadKey,
            "BAD_ACTION" => ApiError::BadAction(tail.unwrap_or("").to_owned()),
            "BAD_SERVICE" | "WRONG_SERVICE" => ApiError::BadService,
            "BAD_COUNTRY" => ApiError::BadCountry,
            "BAD_STATUS" => ApiError::BadStatus,
            "NO_NUMBERS" => ApiError::NoNumbers,
            "NO_BALANCE" => ApiError::NoBalance,
            "NO_ACTIVATION" | "WRONG_ACTIVATION_ID" | "NOT_FOUND" => ApiError::NoActivation,
            "EARLY_CANCEL_DENIED" => ApiError::EarlyCancelDenied,
            "WRONG_MAX_PRICE" => ApiError::WrongMaxPrice {
                min: tail.and_then(|t| t.parse().ok()),
            },
            "BANNED" => ApiError::Banned {
                until: tail.unwrap_or("").to_owned(),
            },
            other if is_error_token(other) => ApiError::Other(code.to_owned()),
            _ => return None,
        })
    }

    pub fn parse(err: impl std::fmt::Display) -> ApiError {
        ApiError::Parse(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::Parse(e.to_string())
    }
}

/// `ERROR_SQL`, `NO_CONNECTION`, `SERVER_ERROR`, … — SCREAMING_SNAKE tokens that are not success prefixes.
fn is_error_token(head: &str) -> bool {
    const SUCCESS_PREFIXES: [&str; 3] = ["ACCESS_", "STATUS_", "OK"];
    !head.is_empty()
        && head.len() <= 40
        && head
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        && head.contains(|c: char| c.is_ascii_uppercase())
        && !SUCCESS_PREFIXES.iter().any(|p| head.starts_with(p))
}

pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_standard_codes() {
        assert!(matches!(
            ApiError::from_code("BAD_KEY"),
            Some(ApiError::BadKey)
        ));
        assert!(matches!(
            ApiError::from_code("NO_ACTIVATION\n"),
            Some(ApiError::NoActivation)
        ));
        assert!(
            matches!(ApiError::from_code("WRONG_MAX_PRICE:0.25"), Some(ApiError::WrongMaxPrice { min: Some(m) }) if m == 0.25)
        );
        assert!(
            matches!(ApiError::from_code("BANNED:2026-09-01 10:00:00"), Some(ApiError::Banned { until }) if until == "2026-09-01 10:00:00")
        );
        assert!(
            matches!(ApiError::from_code("ERROR_SQL"), Some(ApiError::Other(c)) if c == "ERROR_SQL")
        );
    }

    #[test]
    fn success_and_data_are_not_errors() {
        assert!(ApiError::from_code("ACCESS_BALANCE:12.5").is_none());
        assert!(ApiError::from_code("STATUS_OK:1234").is_none());
        assert!(ApiError::from_code("{\"a\":1}").is_none());
        assert!(ApiError::from_code("").is_none());
    }
}
