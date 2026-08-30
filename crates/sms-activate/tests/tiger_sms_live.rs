//! Read-only live tests against Tiger SMS. They run only when `TIGER_SMS_API_KEY` is set:
//!
//! ```sh
//! export TIGER_SMS_API_KEY=$(grep '^TIGER_SMS_API_KEY=' .env | cut -d= -f2-)
//! cargo test -p sms-activate --test tiger_sms_live -- --nocapture
//! ```
//!
//! Rules: never a real purchase (the only `getNumberV2` call uses an invalid service), one request
//! per test, ≥ 1 s between requests (tests are serialised through a mutex), 8 requests in total,
//! and the API key never reaches stdout/stderr.

#![cfg(feature = "ureq")]

use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::Duration;

use sms_activate::providers::tiger_sms::TigerSms;
use sms_activate::{ActivationId, ApiError, ApiResult, ServiceCode, SmsActivateApi};

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
    std::env::var("TIGER_SMS_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
}

fn live() -> Option<(TigerSms, String, Paced)> {
    let Some(key) = key() else {
        eprintln!("skipped: TIGER_SMS_API_KEY not set");
        return None;
    };
    let guard = PACE.lock().unwrap_or_else(|e| e.into_inner());
    Some((
        TigerSms::with_api_key(key.clone()),
        key,
        Paced { _guard: guard },
    ))
}

/// Error text with the key removed, in case a transport error ever echoes the URL.
fn redact(key: &str, err: &ApiError) -> String {
    err.to_string().replace(key, "<api_key>")
}

/// Runs one call; on HTTP 429 waits 5 s and tries once more.
fn once<T>(key: &str, f: impl Fn() -> ApiResult<T>) -> T {
    match f() {
        Err(ApiError::RateLimited { .. }) => {
            eprintln!("rate limited, backing off 5 s");
            sleep(Duration::from_secs(5));
            f().unwrap_or_else(|e| panic!("{}", redact(key, &e)))
        }
        Err(e) => panic!("{}", redact(key, &e)),
        Ok(v) => v,
    }
}

#[test]
fn balance_is_non_negative() {
    let Some((api, key, _p)) = live() else { return };
    let balance = once(&key, || api.get_balance());
    eprintln!("balance: {balance}");
    assert!(balance >= 0.0);
}

#[test]
fn services_include_telegram() {
    let Some((api, key, _p)) = live() else { return };
    let services = once(&key, || api.get_services());
    eprintln!("services: {}", services.len());
    assert!(services.iter().any(|s| s.code.as_str() == "tg"));
}

#[test]
fn countries_are_listed_as_an_array() {
    let Some((api, key, _p)) = live() else { return };
    let countries = once(&key, || api.get_countries());
    // `rent` was absent at capture time; the client accepts it either way, so only observe.
    eprintln!(
        "countries: {} (with rent flag: {})",
        countries.len(),
        countries.iter().filter(|c| c.rent.is_some()).count()
    );
    assert!(!countries.is_empty());
    assert!(countries.iter().any(|c| c.id() == Some(187)));
}

#[test]
fn prices_for_telegram_are_non_empty() {
    let Some((api, key, _p)) = live() else { return };
    let tg = ServiceCode::from("tg");
    let table = once(&key, || api.get_prices(Some(&tg), None));
    eprintln!("tg price rows: {}", table.len());
    assert!(!table.is_empty());
    assert!(
        table
            .values()
            .all(|row| row.contains_key(&tg) && row[&tg].cost > 0.0)
    );
}

#[test]
fn prices_v3_for_telegram_usa_list_providers() {
    let Some((api, key, _p)) = live() else { return };
    let tg = ServiceCode::from("tg");
    let table = once(&key, || api.get_prices_v3(&tg, 187));
    let cell = table
        .get(&187)
        .and_then(|row| row.get(&tg))
        .expect("tg/187 cell");
    eprintln!(
        "tg/187: price {} count {} providers {}",
        cell.price,
        cell.count,
        cell.providers.len()
    );
    assert!(cell.price > 0.0);
    assert!(!cell.providers.is_empty());
    assert!(
        cell.providers
            .windows(2)
            .all(|w| w[0].cheapest() <= w[1].cheapest())
    );
}

#[test]
fn free_prices_for_telegram_usa_form_a_ladder() {
    let Some((api, key, _p)) = live() else { return };
    let tg = ServiceCode::from("tg");
    let table = once(&key, || api.get_free_prices(Some(&tg), Some(187)));
    let ladder = table
        .get(&187)
        .and_then(|row| row.get(&tg))
        .expect("tg/187 ladder");
    eprintln!(
        "tg/187 buckets: {} total {} avg {:?}",
        ladder.buckets.len(),
        ladder.total_count(),
        ladder.sale_average_price
    );
    assert!(!ladder.buckets.is_empty());
    assert!(ladder.buckets.windows(2).all(|w| w[0].price < w[1].price));
    assert!(ladder.recommended_max_price().unwrap() >= ladder.cheapest().unwrap().price);
}

#[test]
fn bogus_activation_id_is_not_found() {
    let Some((api, key, _p)) = live() else { return };
    match api.get_status(&ActivationId::from("1")) {
        Err(ApiError::NoActivation) => {}
        Err(ApiError::RateLimited { .. }) => {
            sleep(Duration::from_secs(5));
            assert!(matches!(
                api.get_status(&ActivationId::from("1")),
                Err(ApiError::NoActivation)
            ));
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

/// The only call to a purchase action: an invalid service can never buy anything. A `BAD_SERVICE`
/// (or validation) answer proves `getNumberV2` exists; `BAD_ACTION` would mean it does not.
#[test]
fn get_number_v2_exists_probe() {
    let Some((api, key, _p)) = live() else { return };
    assert!(api.capabilities().get_number_v2);
    let req = sms_activate::NumberRequest::new("__probe__", 187);
    let result = api.get_number(&req);
    match &result {
        Err(ApiError::BadService) => eprintln!("getNumberV2 probe: BAD_SERVICE (action exists)"),
        Err(ApiError::Validation { field, .. }) => {
            eprintln!("getNumberV2 probe: validation on `{field}` (action exists)");
        }
        Err(ApiError::BadAction(_)) => panic!("getNumberV2 is not supported: capability is wrong"),
        Err(ApiError::RateLimited { .. }) => eprintln!("rate limited; probe inconclusive"),
        Err(e) => panic!("unexpected error: {}", redact(&key, e)),
        Ok(a) => panic!(
            "an invalid service must not buy a number, got activation {}",
            a.id
        ),
    }
}
