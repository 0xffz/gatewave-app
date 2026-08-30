//! Read-only live tests against Hero-SMS. They run only when `HERO_SMS_API_KEY` is set:
//!
//! ```sh
//! export HERO_SMS_API_KEY=$(grep '^HERO_SMS_API_KEY=' .env | cut -d= -f2-)
//! cargo test -p sms-activate --test hero_sms_live -- --nocapture
//! ```
//!
//! Rules: never a real purchase (the only `getNumberV2` call uses an invalid service), exactly one
//! request per test — a HTTP 429 skips the test instead of re-sending, so a full run is at most
//! 8 requests — ≥ 1 s between requests (tests are serialised through a mutex), and the API key
//! never reaches stdout/stderr.
//!
//! Needs the default `ureq` transport; the file is empty under `--no-default-features`.
#![cfg(feature = "ureq")]

use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::Duration;

use sms_activate::providers::hero_sms::HeroSms;
use sms_activate::{ActivationId, ApiError, ApiResult, CountryRef, ServiceCode, SmsActivateApi};

static PACE: Mutex<()> = Mutex::new(());

/// Serialises the live tests and enforces the pause between requests.
struct Paced {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for Paced {
    fn drop(&mut self) {
        sleep(Duration::from_millis(1100));
    }
}

fn key() -> Option<String> {
    std::env::var("HERO_SMS_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
}

fn live() -> Option<(HeroSms, String, Paced)> {
    let Some(key) = key() else {
        eprintln!("skipped: HERO_SMS_API_KEY not set");
        return None;
    };
    let guard = PACE.lock().unwrap_or_else(|e| e.into_inner());
    Some((
        HeroSms::with_api_key(key.clone()),
        key,
        Paced { _guard: guard },
    ))
}

/// Error text with the key removed, in case a transport error ever echoes the URL.
fn redact(key: &str, err: &ApiError) -> String {
    err.to_string().replace(key, "<api_key>")
}

/// Runs exactly one call. A HTTP 429 skips the test (returns `None`) rather than spending a
/// second request; every other error fails the test with the key redacted.
fn once<T>(key: &str, f: impl FnOnce() -> ApiResult<T>) -> Option<T> {
    match f() {
        Err(ApiError::RateLimited { .. }) => {
            eprintln!("skipped: rate limited (HTTP 429); not re-sending");
            None
        }
        Err(e) => panic!("{}", redact(key, &e)),
        Ok(v) => Some(v),
    }
}

#[test]
fn balance_is_non_negative() {
    let Some((api, key, _p)) = live() else { return };
    let Some(balance) = once(&key, || api.get_balance()) else {
        return;
    };
    eprintln!("balance: {balance}");
    assert!(balance >= 0.0);
}

#[test]
fn services_include_telegram() {
    let Some((api, key, _p)) = live() else { return };
    let Some(services) = once(&key, || api.get_services()) else {
        return;
    };
    eprintln!("services: {}", services.len());
    assert!(services.iter().any(|s| s.code.as_str() == "tg"));
}

#[test]
fn countries_are_listed() {
    let Some((api, key, _p)) = live() else { return };
    let Some(countries) = once(&key, || api.get_countries()) else {
        return;
    };
    eprintln!("countries: {}", countries.len());
    assert!(!countries.is_empty());
    assert!(
        countries
            .iter()
            .any(|c| c.name_en == "Ukraine" && c.id() == Some(1))
    );
}

#[test]
fn prices_for_telegram_are_non_empty() {
    let Some((api, key, _p)) = live() else { return };
    let tg = ServiceCode::from("tg");
    let Some(table) = once(&key, || api.get_prices(Some(&tg), None)) else {
        return;
    };
    eprintln!("tg price rows: {}", table.len());
    assert!(!table.is_empty());
    assert!(
        table
            .values()
            .all(|row| row.contains_key(&tg) && row[&tg].cost >= 0.0)
    );
}

#[test]
fn top_countries_with_free_price_tiers() {
    let Some((api, key, _p)) = live() else { return };
    let Some(rows) = once(&key, || {
        api.get_top_countries_free_price(&ServiceCode::from("tg"))
    }) else {
        return;
    };
    eprintln!("tg top countries: {}", rows.len());
    assert!(!rows.is_empty());
    let first = &rows[0];
    assert!(first.count > 0);
    assert!(
        !first.free_price_map.is_empty(),
        "freePrice=true should add freePriceMap"
    );
    assert!(
        first
            .free_price_map
            .windows(2)
            .all(|w| w[0].price < w[1].price)
    );
}

#[test]
fn numbers_status_for_usa_is_non_empty() {
    let Some((api, key, _p)) = live() else { return };
    let Some(counts) = once(&key, || api.get_numbers_status(&CountryRef::Id(187), None)) else {
        return;
    };
    eprintln!("services with stock in 187: {}", counts.len());
    assert!(!counts.is_empty());
}

#[test]
fn bogus_activation_id_is_not_found() {
    let Some((api, key, _p)) = live() else { return };
    match api.get_status(&ActivationId::from("1")) {
        Err(ApiError::NoActivation) => {}
        Err(ApiError::RateLimited { .. }) => {
            eprintln!("skipped: rate limited (HTTP 429); not re-sending");
        }
        other => panic!(
            "expected NoActivation, got {}",
            match other {
                Ok(s) => format!("{s:?}"),
                Err(e) => redact(&key, &e),
            }
        ),
    }
}

/// The only call to a purchase action: an invalid service can never buy anything. A validation
/// (or BAD_SERVICE) answer proves `getNumberV2` exists; `BAD_ACTION` would mean it does not.
#[test]
fn get_number_v2_exists_probe() {
    let Some((api, key, _p)) = live() else { return };
    assert!(api.capabilities().get_number_v2);
    let req = sms_activate::NumberRequest::new("__probe__", 187);
    let result = api.get_number(&req);
    match &result {
        Err(ApiError::Validation { field, .. }) => {
            eprintln!("getNumberV2 probe: validation on `{field}` (action exists)");
            assert_eq!(field, "service");
        }
        Err(ApiError::BadService) => eprintln!("getNumberV2 probe: BAD_SERVICE (action exists)"),
        Err(ApiError::BadAction(_)) => panic!("getNumberV2 is not supported: capability is wrong"),
        Err(ApiError::RateLimited { .. }) => eprintln!("rate limited; probe inconclusive"),
        Err(e) => panic!("unexpected error: {}", redact(&key, e)),
        Ok(a) => panic!(
            "an invalid service must not buy a number, got activation {}",
            a.id
        ),
    }
}
