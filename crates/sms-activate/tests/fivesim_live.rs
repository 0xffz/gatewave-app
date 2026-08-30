//! Read-only live tests against 5SIM. The `/guest/*` tests always run (no key needed); the
//! `/user/*` tests run only when `FIVESIM_API_KEY` is set:
//!
//! ```sh
//! export FIVESIM_API_KEY=$(grep '^FIVESIM_API_KEY=' .env | cut -d= -f2-)
//! cargo test -p sms-activate --test fivesim_live -- --nocapture
//! ```
//!
//! Rules: never a real purchase (the only buy call uses a product that does not exist), exactly
//! one request per test — a rate limit (HTTP 429 / 503 → `ApiError::RateLimited`) skips the test
//! instead of re-sending, so a full run is at most 7 requests — ≥ 1 s between requests (tests
//! are serialised through a mutex), and the token never reaches stdout/stderr.
//!
//! Needs the default `ureq` transport; the file is empty under `--no-default-features`.
#![cfg(feature = "ureq")]

use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::Duration;

use sms_activate::providers::fivesim::{FiveSim, OrderCategory, Page};
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
    std::env::var("FIVESIM_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
}

fn pace() -> Paced {
    Paced {
        _guard: PACE.lock().unwrap_or_else(|e| e.into_inner()),
    }
}

/// Guest client (empty token) — `/guest/*` needs no authentication.
fn guest() -> (FiveSim, Paced) {
    (FiveSim::with_api_key(""), pace())
}

fn live() -> Option<(FiveSim, String, Paced)> {
    let Some(key) = key() else {
        eprintln!("skipped: FIVESIM_API_KEY not set");
        return None;
    };
    let paced = pace();
    Some((FiveSim::with_api_key(key.clone()), key, paced))
}

/// Error text with the token removed, in case a transport error ever echoes a header. An empty
/// key (guest tests) is left alone: `str::replace("")` would match every char boundary.
fn redact(key: &str, err: &ApiError) -> String {
    let text = err.to_string();
    if key.is_empty() {
        text
    } else {
        text.replace(key, "<api_key>")
    }
}

/// Runs exactly one call. A rate limit (HTTP 429 / 503) skips the test (returns `None`) rather
/// than spending a second request; every other error fails the test with the token redacted.
fn once<T>(key: &str, f: impl FnOnce() -> ApiResult<T>) -> Option<T> {
    match f() {
        Err(ApiError::RateLimited { .. }) => {
            eprintln!("skipped: rate limited (HTTP 429/503); not re-sending");
            None
        }
        Err(e) => panic!("{}", redact(key, &e)),
        Ok(v) => Some(v),
    }
}

// -- guest endpoints ----------------------------------------------------------

#[test]
fn guest_countries_include_england() {
    let (api, _p) = guest();
    let Some(countries) = once("", || api.get_countries()) else {
        return;
    };
    eprintln!("countries: {}", countries.len());
    assert!(countries.len() > 50);
    let england = countries
        .iter()
        .find(|c| c.key == CountryRef::Slug("england".into()))
        .expect("england is listed");
    assert_eq!(england.name_en, "England");
    assert_eq!(england.iso.as_deref(), Some("gb"));
    assert_eq!(england.prefix.as_deref(), Some("+44"));
    assert!(england.name_ru.is_some());
}

#[test]
fn guest_products_for_england_include_telegram() {
    let (api, _p) = guest();
    let england = CountryRef::Slug("england".into());
    let Some(counts) = once("", || api.get_numbers_status(&england, None)) else {
        return;
    };
    eprintln!("activation products in england: {}", counts.len());
    assert!(counts.len() > 100);
    assert!(counts.contains_key(&ServiceCode::from("telegram")));
}

#[test]
fn guest_prices_england_telegram_are_non_empty() {
    let (api, _p) = guest();
    let england = CountryRef::Slug("england".into());
    let telegram = ServiceCode::from("telegram");
    let Some(table) = once("", || api.get_prices(Some(&telegram), Some(&england))) else {
        return;
    };
    let price = &table[&england][&telegram];
    eprintln!(
        "england/telegram: cost {} count {}",
        price.cost, price.count
    );
    assert!(price.cost > 0.0);
    assert_eq!(price.physical_count, None);
}

// -- user endpoints (key-gated) ----------------------------------------------

#[test]
fn balance_is_non_negative_via_profile() {
    let Some((api, key, _p)) = live() else { return };
    let Some(profile) = once(&key, || api.profile()) else {
        return;
    };
    eprintln!(
        "balance: {} rating: {} active orders: {:?}",
        profile.balance, profile.rating, profile.total_active_orders
    );
    assert!(profile.balance >= 0.0);
    assert!(profile.rating >= 0.0);
}

#[test]
fn orders_list_parses() {
    let Some((api, key, _p)) = live() else { return };
    let Some(page) = once(&key, || {
        api.orders(OrderCategory::Activation, &Page::new(5, 0))
    }) else {
        return;
    };
    eprintln!(
        "orders returned: {} (total {})",
        page.data.len(),
        page.total
    );
    assert!(page.data.len() <= 5);
    for order in &page.data {
        assert!(order.id > 0);
        assert!(order.phone.starts_with('+'), "phones keep the leading +");
        assert!(!order.product.is_empty());
    }
}

#[test]
fn bogus_order_id_is_not_found() {
    let Some((api, key, _p)) = live() else { return };
    match api.get_status(&ActivationId::from("1")) {
        Err(ApiError::NoActivation) => {}
        Err(ApiError::RateLimited { .. }) => {
            eprintln!("skipped: rate limited (HTTP 429/503); not re-sending");
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

/// The only call to the purchase endpoint: a product that does not exist can never buy anything.
/// `no product` (→ `BadService`) proves the route and the error mapping.
#[test]
fn buy_probe_with_unknown_product_buys_nothing() {
    let Some((api, key, _p)) = live() else { return };
    let req = sms_activate::NumberRequest::new("zzprobezz", "england");
    match api.get_number(&req) {
        Err(ApiError::BadService) => eprintln!("buy probe: no product (nothing bought)"),
        Err(ApiError::RateLimited { .. }) => eprintln!("rate limited; probe inconclusive"),
        Err(ApiError::BadKey) => panic!("the token was rejected (HTTP 401)"),
        Err(e) => panic!("unexpected error: {}", redact(&key, &e)),
        Ok(a) => panic!(
            "an unknown product must not buy a number, got order {} ({})",
            a.id, a.phone
        ),
    }
}
