//! Read-only live checks against SMSBower. Every test returns early (and prints `skipped:`) when
//! `SMSBOWER_API_KEY` is not set, so plain `cargo test` stays green offline.
//!
//! Safety budget: 8 requests in total, paced ≥ 1 s apart (a global pacer works across the test
//! threads), one 5 s back-off + retry on HTTP 429. No purchase is ever made: the only `getNumberV2`
//! call uses the invalid service `__probe__`. The API key is redacted from every message.

#![cfg(feature = "ureq")]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use sms_activate::providers::smsbower::SmsBower;
use sms_activate::{
    ActivationId, ApiError, ApiResult, CountryRef, NumberRequest, ServiceCode, SmsActivateApi,
};

const KEY_VAR: &str = "SMSBOWER_API_KEY";
const MIN_GAP: Duration = Duration::from_secs(1);
const RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(5);

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

fn client() -> Option<SmsBower> {
    match std::env::var(KEY_VAR) {
        Ok(key) if !key.trim().is_empty() => Some(SmsBower::with_api_key(key.trim())),
        _ => {
            eprintln!("skipped: {KEY_VAR} not set");
            None
        }
    }
}

/// Runs one request ≥ 1 s after the previous one (across all tests); on 429 waits 5 s and retries once.
fn paced<T>(mut op: impl FnMut() -> ApiResult<T>) -> ApiResult<T> {
    let mut last = LAST_REQUEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut result = wait_then_call(&mut last, &mut op);
    if matches!(result, Err(ApiError::RateLimited { .. })) {
        eprintln!("rate limited; backing off {RATE_LIMIT_BACKOFF:?}");
        std::thread::sleep(RATE_LIMIT_BACKOFF);
        result = wait_then_call(&mut last, &mut op);
    }
    result
}

fn wait_then_call<T>(
    last: &mut Option<Instant>,
    op: &mut impl FnMut() -> ApiResult<T>,
) -> ApiResult<T> {
    if let Some(t) = *last {
        let elapsed = t.elapsed();
        if elapsed < MIN_GAP {
            std::thread::sleep(MIN_GAP - elapsed);
        }
    }
    let result = op();
    *last = Some(Instant::now());
    result
}

fn redact(text: &str) -> String {
    match std::env::var(KEY_VAR) {
        Ok(key) if !key.trim().is_empty() => text.replace(key.trim(), "<redacted>"),
        _ => text.to_owned(),
    }
}

fn must<T>(what: &str, result: ApiResult<T>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => panic!("{what} failed: {}", redact(&e.to_string())),
    }
}

#[test]
fn balance_is_non_negative() {
    let Some(c) = client() else { return };
    let balance = must("getBalance", paced(|| c.get_balance()));
    eprintln!("balance: {balance}");
    assert!(balance >= 0.0);
}

#[test]
fn services_include_telegram() {
    let Some(c) = client() else { return };
    let services = must("getServicesList", paced(|| c.get_services()));
    eprintln!("services: {}", services.len());
    assert!(!services.is_empty());
    assert!(
        services.iter().any(|s| s.code.as_str() == "tg"),
        "no `tg` in services"
    );
}

#[test]
fn countries_include_usa() {
    let Some(c) = client() else { return };
    let countries = must("getCountries", paced(|| c.get_countries()));
    eprintln!("countries: {}", countries.len());
    assert!(!countries.is_empty());
    let usa = countries
        .iter()
        .find(|c| c.id() == Some(187))
        .expect("country 187");
    assert_eq!(usa.name_en, "United States");
}

#[test]
fn prices_for_telegram_are_non_empty() {
    let Some(c) = client() else { return };
    let tg = ServiceCode::from("tg");
    let table = must("getPrices", paced(|| c.get_prices(Some(&tg), None)));
    eprintln!("tg priced in {} countries", table.len());
    assert!(!table.is_empty());
    for (country, row) in &table {
        let price = row
            .get(&tg)
            .unwrap_or_else(|| panic!("country {country} row has no tg"));
        assert!(price.cost > 0.0, "country {country}: cost {}", price.cost);
    }
}

#[test]
fn top_countries_for_telegram_use_slugs() {
    let Some(c) = client() else { return };
    let rows = must(
        "getTopCountriesByService",
        paced(|| c.get_top_countries(&ServiceCode::from("tg"))),
    );
    eprintln!("top rows: {}", rows.len());
    assert!(!rows.is_empty());
    for row in &rows {
        assert!(
            matches!(&row.country, CountryRef::Slug(s) if !s.is_empty()),
            "expected a slug, got {:?}",
            row.country
        );
        assert!(row.provider_id.is_some(), "missing partner id in {row:?}");
        assert!(row.price > 0.0, "non-positive price in {row:?}");
    }
}

#[test]
fn bogus_activation_id_is_no_activation() {
    let Some(c) = client() else { return };
    let result = paced(|| c.get_status(&ActivationId::from("1")));
    match result {
        Err(ApiError::NoActivation) => {}
        Ok(status) => panic!("getStatus id=1 unexpectedly succeeded: {status:?}"),
        Err(e) => panic!("expected NoActivation, got {}", redact(&e.to_string())),
    }
}

#[test]
fn prices_v3_for_telegram_in_usa_are_non_empty() {
    let Some(c) = client() else { return };
    let tg = ServiceCode::from("tg");
    let table = must(
        "getPricesV3",
        paced(|| c.get_prices_v3(Some(&tg), Some(187))),
    );
    let offers = table
        .get(&187)
        .and_then(|row| row.get(&tg))
        .expect("187/tg present");
    eprintln!("tg/187 partner offers: {}", offers.len());
    assert!(!offers.is_empty());
    assert!(offers.windows(2).all(|w| w[0].price <= w[1].price));
    assert!(
        offers
            .iter()
            .all(|o| !o.provider_id.is_empty() && o.price > 0.0)
    );
}

/// Confirms `getNumberV2` exists WITHOUT buying anything: an invalid service must be rejected
/// with a service/validation error (`WRONG_SERVICE` on 2026-08-30), never `BAD_ACTION`.
#[test]
fn get_number_v2_exists_probe_with_invalid_service() {
    let Some(c) = client() else { return };
    assert!(
        c.capabilities().get_number_v2,
        "dialect must route to getNumberV2"
    );
    let result = paced(|| c.get_number(&NumberRequest::new("__probe__", 187)));
    match result {
        Err(ApiError::BadService) => eprintln!("getNumberV2 probe: BadService (action exists)"),
        Err(ApiError::Validation { field, message }) => {
            eprintln!("getNumberV2 probe: validation on `{field}`: {message} (action exists)")
        }
        Err(ApiError::BadAction(a)) => panic!("getNumberV2 is not supported: BAD_ACTION {a}"),
        Err(e) => panic!("unexpected error: {}", redact(&e.to_string())),
        Ok(a) => panic!(
            "probe with an invalid service unexpectedly bought a number (id {}); cancel it manually after 2 minutes",
            a.id
        ),
    }
}
