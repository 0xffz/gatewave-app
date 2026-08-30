//! Tiger SMS — <https://tiger-sms.com/api>
//!
//! The sms-activate-compatible dialect of Tiger SMS. Everything below was verified against live
//! responses captured on 2026-08-30 (`fixtures/tiger_sms/`, HTTP statuses in
//! `fixtures/README.md`) and the provider's OpenAPI 3.0.3 document
//! (`fixtures/tiger_sms/openapi.json`, published at [`OPENAPI_URL`]; a Postman collection lives at
//! [`POSTMAN_URL`]). Tiger SMS and Hero-SMS share a backend family: the JSON error envelope
//! `{"title","details","info"}` is byte-identical.
//!
//! # Endpoint and authentication
//!
//! * [`ENDPOINT`] = `https://api.tiger-sms.com/stubs/handler_api.php` (the OpenAPI server);
//!   [`ALT_ENDPOINT`] = `https://tiger-sms.com/stubs/handler_api.php` answers identically.
//! * `GET` and `POST` are both accepted and read the same query parameters (verified:
//!   `post_getBalance.txt`). This crate always uses `GET`.
//! * The key travels as the `api_key` query parameter (32-character alphanumeric). A missing or
//!   invalid key is checked by middleware before any action runs: HTTP **401** with the plain body
//!   `BAD_KEY` (fixture `badkey_getBalance.txt`).
//! * Every amount is USD (ISO 4217 numeric `840`, see [`CURRENCY_USD`]).
//!
//! # Capability matrix (evidence: live probes + `openapi.json`)
//!
//! | action | status | evidence |
//! | --- | --- | --- |
//! | `getBalance[&format=json]` | ✔ | `ACCESS_BALANCE:4.682`; `format=json` → `{"balance":"4.6820","currency":840}` ([`TigerSms::get_balance_json`]) |
//! | `getNumber` | ✔ | `service=__probe__` → HTTP 200 plain `BAD_SERVICE` (exists; no purchase made) |
//! | `getNumberV2` | ✔ (default) | same probe → HTTP **200** `{"title":"BAD_SERVICE","details":"This service/country combination is not available"}` |
//! | `multiple`, `maxPrice`, `providerIds`, `exceptProviderIds`, `ref`, `activationType`, `fixedPrice` | ✔ (documented) | [`TigerNumberOptions`] / [`TigerSms::get_number_with`]; `operator`, `phoneException` and `minPrice` are accepted but **ignored** — [`TigerSmsDialect::adjust_params`] never sends `operator` / `minPrice` |
//! | `getStatus[&full_text]` / `setStatus` | ✔ | standard tokens; unknown **or non-numeric** id → plain `NO_ACTIVATION`; `getStatus` may answer `ACCESS_CANCEL` (documented) |
//! | `getStatusV2` | ✔ | `{"verificationType":0\|1,"sms":null\|{dateTime,code,text}}`; unknown id → HTTP 404 `NOT_FOUND` envelope |
//! | `setStatusV2` | ✔ | `{"status":"success"}`; unknown id → HTTP 404 `NOT_FOUND`; 409 `EARLY_CANCEL_DENIED`; `BAD_STATUS` envelope on 200 |
//! | `getPrices[&service][&country]` | ✔ | `{country:{service:{cost:"0.2500",count}}}` — `cost` is a **string** |
//! | `getFreePrices` = `getPricesV2` | ✔ | `{country:{service:{prices:{"<price>":count},has_multi:{"<price>":bool},saleAveragePrice}}}` ([`TigerSms::get_free_prices`]) |
//! | `getPricesV3&service&country` | ✔ | both params **required**; `{country:{service:{price,count,currency,saleAveragePrice,providers:{id:{count,price:[…],provider_id}}}}}` |
//! | `getOffers[&services][&countries]` | ✔ | `{"data":{service:{country:{prices:{default,avg,retail,min},counts:{total,defaultPrice},map:{…}}}}}` — **service first** |
//! | `getProviders[&service][&country]` | ✔ | JSON array wrapped in `<html>…<body>[…]</body></html>` (`Content-Type: text/html`) |
//! | `getServiceNumbersCount&service` | ✔ | `[{"countryCode":187,"numbersCount":44432},…]` (numeric codes live, strings in the docs) |
//! | `getServicesList[&country][&lang]` | ✔ | `{status,services:[{code,name}]}`; `country=187` filters (977 → 580 rows); `lang` ∈ en ru cn es pt vn id hi tr ko jp |
//! | `getCountries` | ✔ | bare **array** `[{id,rus,eng,chn,visible:1,retry:1}]`, no `rent`; `visible`/`retry` are always 1 |
//! | `getActiveActivations[&start][&limit]` | ✔ | `{"status":"success","data":[…]}` (`limit` ≤ 100, default 50) |
//! | `getTopCountriesByService` | ✘ | plain `BAD_ACTION` — see [`TigerSms::top_countries_from_prices`] |
//! | `getNumbersStatus` / `getOperators` / `getServices` / `getFullSms` | ✘ | plain `BAD_ACTION` |
//!
//! # Error shapes
//!
//! Three families coexist, all handled by [`classify`]:
//!
//! 1. **Plain tokens with HTTP 200** for the legacy (v1) actions: `NO_ACTIVATION`, `BAD_ACTION`
//!    (also returned for *any* unexpected server error inside a known action — the dispatcher
//!    never emits a 5xx), `BAD_SERVICE`, `BAD_COUNTRY`, `NO_NUMBERS`, `NO_BALANCE`, `BAD_STATUS`,
//!    `EARLY_CANCEL_DENIED`, `NO_PROVIDERS`, `ERROR` (`getProviders`), and the Tiger-only
//!    validation tokens `BAD_VALUES`, `BAD_MULTIPLE`, `BAD_MAX_PRICE`, `BAD_PROVIDER_IDS`,
//!    `BAD_EXCEPT_PROVIDER_IDS`, `BAD_ACTIVATION_TYPE`, `BAD_FIXED_PRICE`, which map to
//!    [`ApiError::Validation`] with the offending field name. `WRONG_MAX_PRICE:<min>` is
//!    documented with HTTP 400 (not yet live — `NO_NUMBERS` is returned instead).
//! 2. **Plain `BAD_KEY` with HTTP 401** for every action.
//! 3. **JSON envelopes** `{"title":"<CODE>","details":"…","info":{…}}` for the V2/V3 actions
//!    (decoded by the shared [`protocol::TitleEnvelope`] — Hero-SMS uses the same shape),
//!    on HTTP 200 (`BAD_SERVICE`, `BAD_COUNTRY`, `NO_NUMBERS`, `NO_PROVIDERS`, `BAD_STATUS`),
//!    400 (`WRONG_MAX_PRICE`, `info.min`), 402 (`NO_BALANCE`), 404 (`NOT_FOUND`), 409
//!    (`EARLY_CANCEL_DENIED`, `info.minActivationTime`), 422 (`UNPROCESSABLE_ENTITY`,
//!    optional `info:{field,code,message}`; on `getNumberV2` the `details` may instead be the
//!    first validation token, e.g. `BAD_SERVICE` / `BAD_MAX_PRICE`) and 429 (`RATE_LIMIT`).
//!
//! Precedence: an envelope wins on any status; then HTTP 429 is always
//! [`ApiError::RateLimited`] whatever the body (edge `Too Many Requests`, application
//! `RATE_LIMIT`); other 4xx bodies that are a *specifically mapped* token (401 `BAD_KEY`, 400
//! `WRONG_MAX_PRICE:<min>`, 402 `NO_BALANCE`) map to that variant and everything else non-2xx
//! (all 5xx included) is [`ApiError::Http`] so the status is never lost; on 2xx a plain token is
//! the error and anything else is data.
//!
//! | observed | mapped to |
//! | --- | --- |
//! | 401 `BAD_KEY` (plain) / `{"title":"BAD_KEY"}` | [`ApiError::BadKey`] |
//! | `NO_ACTIVATION` / 404 `{"title":"NOT_FOUND"}` | [`ApiError::NoActivation`] |
//! | `BAD_ACTION` / `{"title":"BAD_ACTION"}` | [`ApiError::BadAction`] (empty payload: the envelope's `details` is the fixed "Method Not Found") |
//! | `BAD_SERVICE` (plain, envelope, or 422 `details`) | [`ApiError::BadService`] |
//! | `BAD_COUNTRY` (plain, envelope, or 422 `details`) | [`ApiError::BadCountry`] |
//! | `NO_NUMBERS` / 402 `NO_BALANCE` / `BAD_STATUS` / 409 `EARLY_CANCEL_DENIED` | the matching variant |
//! | 400 `WRONG_MAX_PRICE` (`info.min`) / `WRONG_MAX_PRICE:<min>` | [`ApiError::WrongMaxPrice`] |
//! | 422 `UNPROCESSABLE_ENTITY` (`info.field` or prose `details`) | [`ApiError::Validation`] |
//! | `BAD_VALUES`, `BAD_MULTIPLE`, `BAD_MAX_PRICE`, … (plain, as a title, or as 422 `details`) | [`ApiError::Validation`] (`field` = the parameter, `message` = the token) |
//! | `NO_PROVIDERS`, `ERROR`, anything else in SCREAMING_SNAKE | [`ApiError::Other`] (`"<title>: <details>"` for envelopes) |
//! | HTTP 429 (plain `Too Many Requests`, `RATE_LIMIT`, or `{"title":"RATE_LIMIT"}`) | [`ApiError::RateLimited`] |
//! | any other non-2xx (5xx, 403, …) | [`ApiError::Http`] with the status |
//!
//! # Quirks confirmed against live responses
//!
//! * `getPrices` returns `cost` as a 4-decimal **string** (`"0.2500"`); the standard parser
//!   already accepts numeric strings. Per the docs `cost` is the recommended `maxPrice` for a
//!   purchase, and `count` can be a rough 50–100 placeholder when exact stock is not tracked.
//! * `getCountries` is a bare JSON array (not an id-keyed object) and carries no `rent` flag.
//! * The price ladders of `getFreePrices` / `getPricesV2` and the `map` of `getOffers` are
//!   documented as per-bucket counts but are **cumulative** live: the count at a price is the
//!   number of numbers obtainable at that price *or cheaper*, and the last bucket equals the
//!   `count` of `getPrices` (`44432` for `tg`/187 in the fixtures; per-provider counts of
//!   `getPricesV3` add up to the ladder steps). See [`PriceLadder`].
//! * `getProviders` wraps its JSON array in an HTML document; `delivery_percent` may be `null`;
//!   `name` is always the literal `Provider<id>` (real vendor names are never exposed).
//! * `getServiceNumbersCount` sends numeric `countryCode`s while the docs show strings; both are
//!   accepted.
//! * `getStatus` is documented to answer `ACCESS_CANCEL` (not `STATUS_CANCEL`) for a cancelled
//!   activation; the dialect maps it to [`ActivationStatus::Cancelled`].
//! * `getActiveActivations` uses a provider-specific `activationStatus` numbering: 1 WAIT_CODE,
//!   2 WAIT_ACCEPTING, 3 WAIT_NEXT_SMS, 6 OK, 7 REFUND, 8 CANCEL ([`ActiveActivation::status`]
//!   keeps the raw string). It degrades to `data: []` instead of failing.
//! * `getServicesList&lang=ru` still returned English names for most services at capture time.
//! * `setStatus` accepts `-1` as an alias for 8 (cancel); this crate always sends the canonical
//!   code.
//! * `getNumberV2` documents extra fields (`currency`, `countryPhoneCode`, `activationEndTime`)
//!   that [`Activation`] does not carry; `activationCost` may later be adjusted downwards.
//! * `country=any` is not supported on purchases; `country` must be a numeric code.
//! * `getOffers` and `getPricesV3` are cached server-side for up to 45 s.
//!
//! # Rate limits
//!
//! `getNumber`, `getNumberV2` and `getPricesV3` are rate-limited per user + service + country
//! (`getOffers` per user), partly at the edge: an edge 429 is the plain text `Too Many Requests`
//! with a `Retry-After` header that [`HttpResponse`] does not expose, so `retry_after` is always
//! `None`. Concrete limits are not published; no 429 was observed at ≤ 1 request/second. Keep that
//! pace and back off ≥ 5 s on [`ApiError::RateLimited`].
//!
//! # Example
//!
//! ```no_run
//! use sms_activate::providers::tiger_sms::{TigerNumberOptions, TigerSms};
//! use sms_activate::{NumberRequest, ServiceCode};
//!
//! let tiger = TigerSms::with_api_key(std::env::var("TIGER_SMS_API_KEY").unwrap());
//! let tg = ServiceCode::from("tg");
//! let table = tiger.get_prices_v3(&tg, 187).unwrap();
//! let offer = &table[&187][&tg];
//! let best = &offer.providers[0];
//! let req = NumberRequest::new("tg", 187).max_price(offer.price);
//! let options = TigerNumberOptions::new().provider_ids([best.provider_id]);
//! let activation = tiger.get_number_with(&req, &options).unwrap();
//! println!("{} costs {:?}", activation.phone, activation.cost);
//! ```

use std::collections::BTreeMap;
use std::fmt::Display;

use serde_json::Value;

use crate::api::{Client, Dialect};
use crate::error::{ApiError, ApiResult};
use crate::protocol::{
    self, as_object, value_to_bool, value_to_f64, value_to_string, value_to_u64,
};
use crate::transport::{HttpResponse, Transport};
use crate::types::*;

/// The OpenAPI server: `https://api.tiger-sms.com`.
pub const ENDPOINT: &str = "https://api.tiger-sms.com/stubs/handler_api.php";

/// The handler on the main domain; answers identically to [`ENDPOINT`].
pub const ALT_ENDPOINT: &str = "https://tiger-sms.com/stubs/handler_api.php";

/// Machine-readable API description (OpenAPI 3.0.3); a copy is in `fixtures/tiger_sms/openapi.json`.
pub const OPENAPI_URL: &str = "https://tiger-sms.com/api/openapi.json";

/// Postman collection published next to the docs.
pub const POSTMAN_URL: &str = "https://tiger-sms.com/api/postman.json";

/// ISO 4217 numeric code of the only currency Tiger SMS uses.
pub const CURRENCY_USD: u32 = 840;

/// What Tiger SMS implements (see the module docs for the evidence).
///
/// `price_bounds` means `maxPrice` only: the OpenAPI document lists `minPrice` (and `operator`,
/// `phoneException`) as "accepted but currently ignored", so [`NumberRequest::min_price`] and
/// [`NumberRequest::operator`] are never sent (see [`TigerSmsDialect::adjust_params`]) and
/// [`TigerSms::get_number_with`] rejects them with [`ApiError::Unsupported`].
pub const CAPABILITIES: Capabilities = Capabilities {
    get_number_v2: true,
    numbers_status: false,
    active_activations: true,
    operators: false,
    prices_v2: true,
    prices_v3: true,
    price_bounds: true,
    provider_filters: true,
};

/// Query-parameter names of the Tiger-SMS-specific options.
pub mod params {
    /// `1`/`0` — require a number that can receive several SMS.
    pub const MULTIPLE: &str = "multiple";
    /// Comma-separated public provider ids to buy from (see `getPricesV3` / `getProviders`).
    pub const PROVIDER_IDS: &str = "providerIds";
    /// Comma-separated public provider ids to exclude.
    pub const EXCEPT_PROVIDER_IDS: &str = "exceptProviderIds";
    /// Referral id.
    pub const REF: &str = "ref";
    /// `SMS` (default), `CALL_FLASH`, `CALL_VOICE`, `CALL_INTERACTIVE`, `RENT_SMS`.
    pub const ACTIVATION_TYPE: &str = "activationType";
    /// Literal `true`/`false`; requires `maxPrice`.
    pub const FIXED_PRICE: &str = "fixedPrice";
    /// `getStatus` — return the full SMS text instead of the bare code.
    pub const FULL_TEXT: &str = "full_text";
    /// `getBalance&format=json`.
    pub const FORMAT: &str = "format";
}

/// The Tiger SMS dialect. [`Default`] targets [`ENDPOINT`]; use [`TigerSmsDialect::at`] for
/// [`ALT_ENDPOINT`].
#[derive(Clone, Debug)]
pub struct TigerSmsDialect {
    endpoint: &'static str,
}

impl TigerSmsDialect {
    /// Dialect bound to a specific handler URL (e.g. [`ALT_ENDPOINT`]).
    pub const fn at(endpoint: &'static str) -> Self {
        Self { endpoint }
    }
}

impl Default for TigerSmsDialect {
    fn default() -> Self {
        Self::at(ENDPOINT)
    }
}

impl Dialect for TigerSmsDialect {
    fn name(&self) -> &'static str {
        "Tiger SMS"
    }

    fn endpoint(&self) -> &str {
        self.endpoint
    }

    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn classify(&self, resp: &HttpResponse) -> ApiResult<()> {
        classify(resp)
    }

    /// Drops `minPrice` and `operator` from `getNumber` / `getNumberV2`: the OpenAPI document
    /// lists both as "accepted but currently ignored (silently dropped, no error)", so they are
    /// never sent and the outgoing URL matches what the provider actually applies (see
    /// [`CAPABILITIES`]).
    fn adjust_params(&self, action: &str, params: &mut Vec<(String, String)>) {
        if matches!(action, "getNumber" | "getNumberV2") {
            params.retain(|(k, _)| !IGNORED_NUMBER_PARAMS.contains(&k.as_str()));
        }
    }

    /// The docs list `ACCESS_CANCEL` as the `getStatus` answer for a cancelled activation; the
    /// standard parser only knows `STATUS_CANCEL`.
    fn parse_status(&self, body: &str) -> ApiResult<ActivationStatus> {
        if body.trim() == "ACCESS_CANCEL" {
            return Ok(ActivationStatus::Cancelled);
        }
        protocol::parse_status(body)
    }
}

/// `getNumber` / `getNumberV2` query parameters the provider documents as accepted but ignored.
const IGNORED_NUMBER_PARAMS: [&str; 2] = ["minPrice", "operator"];

/// Tiger SMS response classification, in this order:
///
/// 1. a JSON error envelope on any status (the `getNumberV2` probe returned one with HTTP 200);
/// 2. HTTP 429 → [`ApiError::RateLimited`] whatever the body (edge `Too Many Requests`, or an
///    application `RATE_LIMIT` / `TOO_MANY_REQUESTS` token), so back-off logic always triggers;
/// 3. other 4xx whose body is a *specifically mapped* plain token (401 `BAD_KEY`, 400
///    `WRONG_MAX_PRICE:<min>`, 402 `NO_BALANCE`) → that variant;
/// 4. any other non-2xx (every 5xx — the dispatcher never emits one, so the body comes from the
///    edge — and unmapped 4xx bodies) → [`ApiError::Http`] carrying the status;
/// 5. 2xx: a plain token is the error, anything else is data.
pub fn classify(resp: &HttpResponse) -> ApiResult<()> {
    let body = resp.body.trim();
    if let Some(err) = error_from_envelope(body) {
        return Err(err);
    }
    if resp.status == 429 {
        return Err(ApiError::RateLimited { retry_after: None });
    }
    if !(200..300).contains(&resp.status) {
        if (400..500).contains(&resp.status)
            && let Some(err) = error_from_token(body).filter(|e| !matches!(e, ApiError::Other(_)))
        {
            return Err(err);
        }
        return Err(ApiError::Http {
            status: resp.status,
            body: body.to_owned(),
        });
    }
    if let Some(err) = error_from_token(body) {
        return Err(err);
    }
    Ok(())
}

/// Tiger-only validation tokens → the query parameter they complain about.
fn validation_field(token: &str) -> Option<&'static str> {
    Some(match token {
        "BAD_VALUES" => "",
        "BAD_MULTIPLE" => params::MULTIPLE,
        "BAD_MAX_PRICE" => "maxPrice",
        "BAD_PROVIDER_IDS" => params::PROVIDER_IDS,
        "BAD_EXCEPT_PROVIDER_IDS" => params::EXCEPT_PROVIDER_IDS,
        "BAD_ACTIVATION_TYPE" => params::ACTIVATION_TYPE,
        "BAD_FIXED_PRICE" => params::FIXED_PRICE,
        _ => return None,
    })
}

/// Plain-text error tokens (`NO_ACTIVATION`, `BAD_ACTION`, `BAD_KEY`, …) including the
/// Tiger-only validation tokens documented for `getNumber`. `None` for JSON, HTML and data.
pub fn error_from_token(body: &str) -> Option<ApiError> {
    if body.starts_with('{') || body.starts_with('[') || body.starts_with('<') {
        return None;
    }
    let head = body.split_once(':').map_or(body, |(h, _)| h).trim();
    if let Some(field) = validation_field(head) {
        return Some(ApiError::Validation {
            field: field.to_owned(),
            message: body.to_owned(),
        });
    }
    protocol::error_from_body(body)
}

/// `{"title":"BAD_SERVICE","details":"…","info":{…}}` → the matching [`ApiError`], via the
/// family-wide [`protocol::TitleEnvelope`] plus two Tiger-specific readings:
///
/// * a Tiger-only validation token as the title (`BAD_MAX_PRICE`, …) → [`ApiError::Validation`]
///   with the parameter name;
/// * `UNPROCESSABLE_ENTITY` without `info.field` whose `details` is itself a token (documented
///   for `getNumberV2`: "first validation message, e.g. BAD_SERVICE/BAD_COUNTRY/BAD_MAX_PRICE/
///   BAD_FIXED_PRICE") → the token's own mapping (`BadService`, `Validation{field:"maxPrice"}`, …)
///   instead of a field-less `Validation`.
///
/// Returns `None` when the body is not an error envelope (data objects never carry `title`).
pub fn error_from_envelope(body: &str) -> Option<ApiError> {
    let env = protocol::TitleEnvelope::parse(body)?;
    let title = env.title.as_str();
    let details = env.details.as_str();
    if let Some(field) = validation_field(title) {
        return Some(ApiError::Validation {
            field: field.to_owned(),
            message: if details.is_empty() {
                title.to_owned()
            } else {
                format!("{title}: {details}")
            },
        });
    }
    if title == "UNPROCESSABLE_ENTITY"
        && env.info("field").is_none()
        && let Some(err) = error_from_token(details).filter(|e| !matches!(e, ApiError::Other(_)))
    {
        return Some(err);
    }
    env.to_error()
}

#[cfg(feature = "ureq")]
pub type TigerSms<T = crate::transport::UreqTransport> = Client<T, TigerSmsDialect>;
#[cfg(not(feature = "ureq"))]
pub type TigerSms<T> = Client<T, TigerSmsDialect>;

#[cfg(feature = "ureq")]
impl TigerSms {
    /// Client over the default `ureq` transport, talking to [`ENDPOINT`].
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Client::new(
            crate::transport::UreqTransport::new(),
            TigerSmsDialect::default(),
            api_key,
        )
    }
}

// ---------------------------------------------------------------------------
// Provider-only types

/// `getBalance&format=json`.
#[derive(Clone, Debug, PartialEq)]
pub struct BalanceInfo {
    pub balance: f64,
    /// ISO 4217 numeric code; always [`CURRENCY_USD`].
    pub currency: u32,
}

/// One step of a price ladder: `count` numbers are obtainable at `price` (cumulative — see the
/// module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PriceBucket {
    pub price: f64,
    pub count: u64,
    /// Whether numbers at this price can receive several SMS (`getFreePrices` only).
    pub has_multi: Option<bool>,
}

/// `getFreePrices` / `getPricesV2` cell: buckets sorted by ascending price plus the dynamic
/// sale-average price.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PriceLadder {
    pub buckets: Vec<PriceBucket>,
    /// Dynamic average of recent sales; can dip below the cheapest bucket.
    pub sale_average_price: Option<f64>,
}

impl PriceLadder {
    /// Cheapest bucket.
    pub fn cheapest(&self) -> Option<&PriceBucket> {
        self.buckets.first()
    }

    /// Cheapest bucket that offers at least `min_count` numbers.
    pub fn cheapest_with(&self, min_count: u64) -> Option<&PriceBucket> {
        self.buckets.iter().find(|b| b.count >= min_count)
    }

    /// Cheapest bucket that allows several SMS on the same number.
    pub fn cheapest_multi(&self) -> Option<&PriceBucket> {
        self.buckets.iter().find(|b| b.has_multi == Some(true))
    }

    /// Total stock: the last (cumulative) bucket.
    pub fn total_count(&self) -> u64 {
        self.buckets.last().map_or(0, |b| b.count)
    }

    /// The docs' recommended `maxPrice`: `max(saleAveragePrice, cheapest bucket)`.
    pub fn recommended_max_price(&self) -> Option<f64> {
        let floor = self.cheapest()?.price;
        Some(self.sale_average_price.map_or(floor, |avg| avg.max(floor)))
    }
}

/// `getFreePrices` result: country → service → ladder.
pub type FreePriceTable = BTreeMap<CountryId, BTreeMap<ServiceCode, PriceLadder>>;

/// One provider's offer in `getPricesV3`. `provider_id` is the value for
/// [`TigerNumberOptions::provider_ids`] / [`TigerNumberOptions::except_provider_ids`].
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderOffer {
    pub provider_id: u64,
    /// This provider's own stock, or the cumulative stock of its buckets.
    pub count: u64,
    /// Bucket prices in USD, ascending.
    pub prices: Vec<f64>,
}

impl ProviderOffer {
    pub fn cheapest(&self) -> Option<f64> {
        self.prices.first().copied()
    }
}

/// `getPricesV3` cell.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderPrices {
    /// Dynamic sale-average price, never below the default offer; the recommended `maxPrice`.
    pub price: f64,
    /// Same value as `getPrices`' `count`.
    pub count: u64,
    pub currency: Option<u32>,
    pub sale_average_price: Option<f64>,
    /// Providers serving this pair, sorted by cheapest price then id. Empty when none sells.
    pub providers: Vec<ProviderOffer>,
}

/// `getPricesV3` result: country → service → [`ProviderPrices`].
pub type ProviderPriceTable = BTreeMap<CountryId, BTreeMap<ServiceCode, ProviderPrices>>;

/// `prices` block of a `getOffers` cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OfferPrices {
    /// Recommended `maxPrice` (dynamic sale average, never below the default bucket).
    pub default: f64,
    /// Omitted by the provider when there is no sales data.
    pub avg: Option<f64>,
    /// Catalog retail price, markup included.
    pub retail: Option<f64>,
    /// Sellable minimum — the `WRONG_MAX_PRICE` threshold.
    pub min: Option<f64>,
}

/// `counts` block of a `getOffers` cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OfferCounts {
    /// Same value as `getPrices`' `count`.
    pub total: u64,
    /// Numbers in the default bucket.
    pub default_price: Option<u64>,
}

/// One `getOffers` cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub prices: OfferPrices,
    pub counts: OfferCounts,
    /// Price ladder (same data as `getFreePrices`, without `has_multi`).
    pub map: Vec<PriceBucket>,
}

/// `getOffers` result: **service** → country → [`Offer`] (note the order).
pub type OfferTable = BTreeMap<ServiceCode, BTreeMap<CountryId, Offer>>;

/// Row of `getProviders`.
#[derive(Clone, Debug, PartialEq)]
pub struct Provider {
    /// Public numeric id, usable with `providerIds` / `exceptProviderIds`.
    pub id: u64,
    /// Always the literal `Provider<id>`.
    pub name: String,
    pub numbers_count: u64,
    /// SMS delivery rate in percent; `null` when unknown.
    pub delivery_percent: Option<f64>,
    /// Number lifetime as reported by the provider. The unit is **not documented** (the OpenAPI
    /// document only lists the field); observed values are 6–20 live and 1200 in the docs example.
    pub number_lifetime: Option<u64>,
}

/// The SMS block of `getStatusV2`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sms {
    /// RFC 3339 timestamp (`2026-07-02T18:12:31+00:00`).
    pub date_time: Option<String>,
    pub code: Option<String>,
    pub text: Option<String>,
}

/// Result of `getStatusV2`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusV2 {
    /// The provider answered with a classic token (not documented; accepted defensively).
    Plain(ActivationStatus),
    /// JSON answer: `verificationType` is `0` while waiting and `1` once a code arrived (there is
    /// no voice type); `sms` is `None` until then.
    Json {
        verification_type: Option<u64>,
        sms: Option<Sms>,
    },
}

impl StatusV2 {
    /// The received code, if any.
    pub fn code(&self) -> Option<&str> {
        match self {
            StatusV2::Plain(s) => s.code(),
            StatusV2::Json { sms, .. } => sms.as_ref().and_then(|s| s.code.as_deref()),
        }
    }
}

/// `activationType` values of `getNumber` / `getNumberV2`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActivationType {
    #[default]
    Sms,
    CallFlash,
    CallVoice,
    CallInteractive,
    /// Accepted by the API but rental is not a supported product yet.
    RentSms,
}

impl ActivationType {
    pub fn as_str(self) -> &'static str {
        match self {
            ActivationType::Sms => "SMS",
            ActivationType::CallFlash => "CALL_FLASH",
            ActivationType::CallVoice => "CALL_VOICE",
            ActivationType::CallInteractive => "CALL_INTERACTIVE",
            ActivationType::RentSms => "RENT_SMS",
        }
    }
}

// ---------------------------------------------------------------------------
// Request helpers

/// Tiger-SMS-only `getNumber` / `getNumberV2` parameters (all documented; none verified live —
/// they are purchase parameters). Applied to a [`NumberRequest`] by [`TigerNumberOptions::apply`]
/// or, with validation, by [`TigerSms::get_number_with`].
///
/// A plain struct rather than an extension trait on [`NumberRequest`] so that an application
/// importing several providers never sees method-name collisions (SMSBower's
/// `provider_ids` / `except_provider_ids` / `referral` live on the same request type).
///
/// ```
/// use sms_activate::NumberRequest;
/// use sms_activate::providers::tiger_sms::TigerNumberOptions;
///
/// let req = NumberRequest::new("tg", 187).max_price(0.25);
/// let req = TigerNumberOptions::new()
///     .multiple(true)
///     .provider_ids([14, 163])
///     .except_provider_ids([36])
///     .apply(&req);
/// assert!(req.extra.contains(&("providerIds".to_owned(), "14,163".to_owned())));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TigerNumberOptions {
    /// `multiple=1` — only numbers that can receive several SMS (charged at the cheapest
    /// qualifying bucket when combined with `providerIds`). Sent only when `true`.
    pub multiple: bool,
    /// `providerIds` — only buy from these providers (ids from `getPricesV3` / `getProviders`).
    /// Empty means no filter (the parameter is not sent).
    pub provider_ids: Vec<u64>,
    /// `exceptProviderIds` — never buy from these providers. Empty means no filter.
    pub except_provider_ids: Vec<u64>,
    /// `ref` — referral id.
    pub referral: Option<String>,
    /// `activationType` — SMS (the provider's default when absent) or one of the call types.
    pub activation_type: Option<ActivationType>,
    /// `fixedPrice=true` — charge exactly `maxPrice` instead of hold-then-settle. Documented as
    /// requiring `maxPrice`: [`TigerNumberOptions::apply`] only emits it when
    /// [`NumberRequest::max_price`] is set and [`TigerSms::get_number_with`] rejects the
    /// combination without one.
    pub fixed_price: bool,
}

impl TigerNumberOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }
    /// Replaces the provider filter.
    pub fn provider_ids(mut self, ids: impl IntoIterator<Item = u64>) -> Self {
        self.provider_ids = ids.into_iter().collect();
        self
    }
    /// Replaces the provider exclusion list.
    pub fn except_provider_ids(mut self, ids: impl IntoIterator<Item = u64>) -> Self {
        self.except_provider_ids = ids.into_iter().collect();
        self
    }
    pub fn referral(mut self, referral_id: impl Into<String>) -> Self {
        self.referral = Some(referral_id.into());
        self
    }
    pub fn activation_type(mut self, activation_type: ActivationType) -> Self {
        self.activation_type = Some(activation_type);
        self
    }
    pub fn fixed_price(mut self, fixed: bool) -> Self {
        self.fixed_price = fixed;
        self
    }

    /// Applies the options to a [`NumberRequest`] as extra query parameters, replacing any
    /// earlier value of the same parameter in [`NumberRequest::extra`]. `fixedPrice` is only
    /// emitted alongside a `max_price`; use [`TigerSms::get_number_with`] to have that
    /// combination rejected instead.
    pub fn apply(&self, request: &NumberRequest) -> NumberRequest {
        let mut req = request.clone();
        if self.multiple {
            set_extra(&mut req, params::MULTIPLE, "1");
        }
        if !self.provider_ids.is_empty() {
            set_extra(&mut req, params::PROVIDER_IDS, join_csv(&self.provider_ids));
        }
        if !self.except_provider_ids.is_empty() {
            set_extra(
                &mut req,
                params::EXCEPT_PROVIDER_IDS,
                join_csv(&self.except_provider_ids),
            );
        }
        if let Some(r) = &self.referral {
            set_extra(&mut req, params::REF, r.clone());
        }
        if let Some(t) = self.activation_type {
            set_extra(&mut req, params::ACTIVATION_TYPE, t.as_str());
        }
        if self.fixed_price && request.max_price.is_some() {
            set_extra(&mut req, params::FIXED_PRICE, "true");
        }
        req
    }
}

fn join_csv<I>(items: I) -> String
where
    I: IntoIterator,
    I::Item: Display,
{
    items
        .into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Replaces `key` in `extra` (removing an earlier value first).
fn set_extra(req: &mut NumberRequest, key: &str, value: impl Into<String>) {
    req.extra.retain(|(k, _)| k != key);
    req.extra.push((key.to_owned(), value.into()));
}

// ---------------------------------------------------------------------------
// Provider-only actions

impl<T: Transport> TigerSms<T> {
    /// `getNumberV2` with the Tiger-SMS-only parameters of [`TigerNumberOptions`].
    ///
    /// Fails **before any request** with [`ApiError::Unsupported`]`("minPrice")` /
    /// `("operator")` when `request.min_price` / `request.operator` is set (the provider
    /// documents both as accepted but ignored, so a caller relying on them would be misled —
    /// the trait path [`crate::SmsActivateApi::get_number`] silently drops them instead), and
    /// with [`ApiError::Validation`] on `maxPrice` when `options.fixed_price` is set without a
    /// `max_price` (the flag is documented only together with `maxPrice`).
    pub fn get_number_with(
        &self,
        request: &NumberRequest,
        options: &TigerNumberOptions,
    ) -> ApiResult<Activation> {
        if request.min_price.is_some() {
            return Err(ApiError::Unsupported("minPrice"));
        }
        if request.operator.is_some() {
            return Err(ApiError::Unsupported("operator"));
        }
        if options.fixed_price && request.max_price.is_none() {
            return Err(ApiError::Validation {
                field: "maxPrice".to_owned(),
                message: "fixedPrice requires max_price".to_owned(),
            });
        }
        crate::SmsActivateApi::get_number(self, &options.apply(request))
    }

    /// `getBalance&format=json` — balance with its currency code.
    pub fn get_balance_json(&self) -> ApiResult<BalanceInfo> {
        let body = self.call(
            "getBalance",
            vec![(params::FORMAT.to_owned(), "json".to_owned())],
        )?;
        parse_balance_json(&body)
    }

    /// `getFreePrices[&service][&country]` — price ladder per country/service. Omitting both
    /// filters returns every pair (large). Empty when the account has no price-bucket data.
    pub fn get_free_prices(
        &self,
        service: Option<&ServiceCode>,
        country: Option<CountryId>,
    ) -> ApiResult<FreePriceTable> {
        let body = self.call("getFreePrices", price_params(service, country))?;
        parse_free_prices(&body)
    }

    /// `getPricesV2` — documented alias of [`TigerSms::get_free_prices`] (same shape).
    pub fn get_prices_v2(
        &self,
        service: Option<&ServiceCode>,
        country: Option<CountryId>,
    ) -> ApiResult<FreePriceTable> {
        let body = self.call("getPricesV2", price_params(service, country))?;
        parse_free_prices(&body)
    }

    /// `getPricesV3&service&country` — price and stock per provider. Both parameters are
    /// required by the provider (unlike `getPrices`).
    pub fn get_prices_v3(
        &self,
        service: &ServiceCode,
        country: CountryId,
    ) -> ApiResult<ProviderPriceTable> {
        let body = self.call("getPricesV3", price_params(Some(service), Some(country)))?;
        parse_prices_v3(&body)
    }

    /// `getOffers[&services][&countries]` — price summary + ladder per service and country.
    /// Empty slices mean "every service" / "every country" (≤ 50 items per list otherwise).
    pub fn get_offers(
        &self,
        services: &[ServiceCode],
        countries: &[CountryId],
    ) -> ApiResult<OfferTable> {
        let mut params = Vec::new();
        if !services.is_empty() {
            params.push(("services".to_owned(), join_csv(services)));
        }
        if !countries.is_empty() {
            params.push(("countries".to_owned(), join_csv(countries)));
        }
        let body = self.call("getOffers", params)?;
        parse_offers(&body)
    }

    /// `getProviders[&service][&country]` — public provider list; with both filters, only the
    /// providers serving that pair. Rows are returned in the provider's order.
    pub fn get_providers(
        &self,
        service: Option<&ServiceCode>,
        country: Option<CountryId>,
    ) -> ApiResult<Vec<Provider>> {
        let body = self.call("getProviders", price_params(service, country))?;
        parse_providers(&body)
    }

    /// `getServiceNumbersCount&service` — available numbers per country for one service.
    pub fn get_service_numbers_count(
        &self,
        service: &ServiceCode,
    ) -> ApiResult<BTreeMap<CountryId, u64>> {
        let body = self.call(
            "getServiceNumbersCount",
            vec![("service".to_owned(), service.0.clone())],
        )?;
        parse_service_numbers_count(&body)
    }

    /// `getServicesList[&country][&lang]` — services with live stock in `country` (all when
    /// `None`), names in `lang` (`en` ru cn es pt vn id hi tr ko jp; unknown → en).
    pub fn get_services_for(
        &self,
        country: Option<CountryId>,
        lang: Option<&str>,
    ) -> ApiResult<Vec<Service>> {
        let mut params = Vec::new();
        if let Some(c) = country {
            params.push(("country".to_owned(), c.to_string()));
        }
        if let Some(l) = lang {
            params.push(("lang".to_owned(), l.to_owned()));
        }
        let body = self.call("getServicesList", params)?;
        self.dialect().parse_services(&body)
    }

    /// `getActiveActivations&start=…&limit=…` (`limit` ≤ 100, default 50).
    pub fn get_active_activations_page(
        &self,
        start: u64,
        limit: u64,
    ) -> ApiResult<Vec<ActiveActivation>> {
        let body = self.call(
            "getActiveActivations",
            vec![
                ("start".to_owned(), start.to_string()),
                ("limit".to_owned(), limit.to_string()),
            ],
        )?;
        self.dialect().parse_active_activations(&body)
    }

    /// `getStatus&full_text=true` — like `get_status`, but `STATUS_OK:` carries the whole SMS
    /// text instead of the extracted code. The spec types `full_text` as a boolean and this
    /// backend spells booleans as the literal `true`/`false` elsewhere (`fixedPrice`), so
    /// `true` is sent (accepted by a literal compare, a `(bool)` cast and `FILTER_VALIDATE_BOOLEAN`
    /// alike, where `1` would fail the first). Documented; not exercised against a live activation.
    pub fn get_status_full_text(&self, id: &ActivationId) -> ApiResult<ActivationStatus> {
        let body = self.call(
            "getStatus",
            vec![
                ("id".to_owned(), id.0.clone()),
                (params::FULL_TEXT.to_owned(), "true".to_owned()),
            ],
        )?;
        self.dialect().parse_status(&body)
    }

    /// `getStatusV2` — structured status with the SMS text and timestamp.
    pub fn get_status_v2(&self, id: &ActivationId) -> ApiResult<StatusV2> {
        let body = self.call("getStatusV2", vec![("id".to_owned(), id.0.clone())])?;
        parse_status_v2(&body)
    }

    /// `setStatusV2` — like `set_status`, with JSON error envelopes (404 `NOT_FOUND`, 409
    /// `EARLY_CANCEL_DENIED`, `BAD_STATUS`). Succeeds on `{"status":"success"}`.
    pub fn set_status_v2(&self, id: &ActivationId, action: StatusAction) -> ApiResult<()> {
        let body = self.call(
            "setStatusV2",
            vec![
                ("id".to_owned(), id.0.clone()),
                ("status".to_owned(), action.code().to_string()),
            ],
        )?;
        parse_set_status_v2(&body)
    }

    /// Substitute for the missing `getTopCountriesByService`: `getPrices&service=…` reshaped
    /// into [`TopCountry`] rows sorted by descending stock (then ascending price). `retail_price`
    /// and `provider_id` are always `None`.
    ///
    /// The trait method [`crate::SmsActivateApi::get_top_countries`] is intentionally *not*
    /// emulated: a [`Dialect`] can only reshape parameters and parse bodies, not swap the action,
    /// and shadowing the trait method on the concrete type would behave differently through
    /// `dyn SmsActivateApi`. It answers [`ApiError::BadAction`] like the provider does.
    pub fn top_countries_from_prices(&self, service: &ServiceCode) -> ApiResult<Vec<TopCountry>> {
        let table = crate::SmsActivateApi::get_prices(self, Some(service), None)?;
        Ok(top_countries_from_price_table(&table, service))
    }
}

fn price_params(
    service: Option<&ServiceCode>,
    country: Option<CountryId>,
) -> Vec<(String, String)> {
    let mut params = Vec::new();
    if let Some(s) = service {
        params.push(("service".to_owned(), s.0.clone()));
    }
    if let Some(c) = country {
        params.push(("country".to_owned(), c.to_string()));
    }
    params
}

// ---------------------------------------------------------------------------
// Parsers for the provider-only shapes

fn parse_country_key(key: &str) -> ApiResult<CountryId> {
    key.trim()
        .parse()
        .map_err(|_| ApiError::Parse(format!("bad country key `{key}`")))
}

fn parse_price_key(key: &str) -> ApiResult<f64> {
    key.trim()
        .parse()
        .map_err(|_| ApiError::Parse(format!("bad price key `{key}`")))
}

/// Walks `{"<country>":{"<service>":<cell>}}` and maps every cell.
fn walk_country_service<C>(
    body: &str,
    what: &str,
    mut cell: impl FnMut(&Value) -> ApiResult<C>,
) -> ApiResult<BTreeMap<CountryId, BTreeMap<ServiceCode, C>>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let mut table = BTreeMap::new();
    for (country, services) in as_object(&v, what)? {
        let country_id = parse_country_key(country)?;
        let mut row = BTreeMap::new();
        for (service, value) in as_object(services, what)? {
            row.insert(ServiceCode(service.clone()), cell(value)?);
        }
        table.insert(country_id, row);
    }
    Ok(table)
}

/// `{"<price>":count,…}` → buckets sorted by ascending price; `has_multi` from the parallel map.
fn parse_price_map(
    prices: Option<&Value>,
    has_multi: Option<&Value>,
) -> ApiResult<Vec<PriceBucket>> {
    let mut buckets = Vec::new();
    if let Some(map) = prices.and_then(Value::as_object) {
        for (key, count) in map {
            buckets.push(PriceBucket {
                price: parse_price_key(key)?,
                count: value_to_u64(Some(count)).unwrap_or(0),
                has_multi: has_multi.and_then(|m| value_to_bool(m.get(key))),
            });
        }
    }
    buckets.sort_by(|a, b| a.price.total_cmp(&b.price));
    Ok(buckets)
}

/// `{"balance":"4.6820","currency":840}`
pub fn parse_balance_json(body: &str) -> ApiResult<BalanceInfo> {
    let v: Value = serde_json::from_str(body.trim())?;
    Ok(BalanceInfo {
        balance: value_to_f64(v.get("balance"))
            .ok_or_else(|| ApiError::Unexpected(body.trim().to_owned()))?,
        currency: value_to_u64(v.get("currency")).map_or(CURRENCY_USD, |c| c as u32),
    })
}

/// `{"187":{"tg":{"prices":{"0.2500":22518,…},"has_multi":{"0.2500":true,…},"saleAveragePrice":0.199}}}`
pub fn parse_free_prices(body: &str) -> ApiResult<FreePriceTable> {
    walk_country_service(body, "getFreePrices", |cell| {
        Ok(PriceLadder {
            buckets: parse_price_map(cell.get("prices"), cell.get("has_multi"))?,
            sale_average_price: value_to_f64(cell.get("saleAveragePrice")),
        })
    })
}

/// `{"187":{"tg":{"price":0.25,"count":44432,"currency":840,"saleAveragePrice":0.199,"providers":{"14":{"count":22518,"price":[0.25],"provider_id":14},…}}}}`
pub fn parse_prices_v3(body: &str) -> ApiResult<ProviderPriceTable> {
    walk_country_service(body, "getPricesV3", |cell| {
        let mut providers = Vec::new();
        if let Some(map) = cell.get("providers").and_then(Value::as_object) {
            for (key, p) in map {
                let provider_id = value_to_u64(p.get("provider_id"))
                    .or_else(|| key.trim().parse().ok())
                    .ok_or_else(|| ApiError::Parse(format!("bad provider id `{key}`")))?;
                let mut prices: Vec<f64> = match p.get("price") {
                    Some(Value::Array(items)) => {
                        items.iter().filter_map(|x| value_to_f64(Some(x))).collect()
                    }
                    other => value_to_f64(other).into_iter().collect(),
                };
                prices.sort_by(f64::total_cmp);
                providers.push(ProviderOffer {
                    provider_id,
                    count: value_to_u64(p.get("count")).unwrap_or(0),
                    prices,
                });
            }
        }
        providers.sort_by(|a, b| {
            a.cheapest()
                .unwrap_or(f64::INFINITY)
                .total_cmp(&b.cheapest().unwrap_or(f64::INFINITY))
                .then(a.provider_id.cmp(&b.provider_id))
        });
        Ok(ProviderPrices {
            price: value_to_f64(cell.get("price"))
                .ok_or_else(|| ApiError::Parse("getPricesV3 cell without price".into()))?,
            count: value_to_u64(cell.get("count")).unwrap_or(0),
            currency: value_to_u64(cell.get("currency")).map(|c| c as u32),
            sale_average_price: value_to_f64(cell.get("saleAveragePrice")),
            providers,
        })
    })
}

/// `{"data":{"tg":{"187":{"prices":{"default":0.25,"avg":0.199,"retail":0.25,"min":0.25},"counts":{"total":44432,"defaultPrice":22518},"map":{"0.2500":22518,…}}}}}`
pub fn parse_offers(body: &str) -> ApiResult<OfferTable> {
    let v: Value = serde_json::from_str(body.trim())?;
    let data = v.get("data").unwrap_or(&v);
    let mut table = BTreeMap::new();
    for (service, countries) in as_object(data, "getOffers")? {
        let mut row = BTreeMap::new();
        for (country, cell) in as_object(countries, "getOffers service")? {
            let prices = cell.get("prices");
            let p = |k: &str| prices.and_then(|x| x.get(k));
            let counts = cell.get("counts");
            let c = |k: &str| counts.and_then(|x| x.get(k));
            row.insert(
                parse_country_key(country)?,
                Offer {
                    prices: OfferPrices {
                        default: value_to_f64(p("default")).ok_or_else(|| {
                            ApiError::Parse(format!(
                                "getOffers {service}/{country} without default price"
                            ))
                        })?,
                        avg: value_to_f64(p("avg")),
                        retail: value_to_f64(p("retail")),
                        min: value_to_f64(p("min")),
                    },
                    counts: OfferCounts {
                        total: value_to_u64(c("total")).unwrap_or(0),
                        default_price: value_to_u64(c("defaultPrice")),
                    },
                    map: parse_price_map(cell.get("map"), None)?,
                },
            );
        }
        table.insert(ServiceCode(service.clone()), row);
    }
    Ok(table)
}

/// `<html><head>…</head><body>[{"id":14,"name":"Provider14","numbers_count":22518,"delivery_percent":8.95,"number_lifetime":20},…]</body></html>`
/// (a bare JSON array is accepted too).
pub fn parse_providers(body: &str) -> ApiResult<Vec<Provider>> {
    let trimmed = body.trim();
    let json = match (trimmed.find('['), trimmed.rfind(']')) {
        (Some(start), Some(end)) if start < end => &trimmed[start..=end],
        _ => return Err(ApiError::Unexpected(trimmed.to_owned())),
    };
    let rows: Vec<Value> = serde_json::from_str(json)?;
    rows.iter()
        .map(|r| {
            let id = value_to_u64(r.get("id"))
                .ok_or_else(|| ApiError::Parse("provider without id".into()))?;
            Ok(Provider {
                id,
                name: value_to_string(r.get("name")).unwrap_or_else(|| format!("Provider{id}")),
                numbers_count: value_to_u64(r.get("numbers_count")).unwrap_or(0),
                delivery_percent: value_to_f64(r.get("delivery_percent")),
                number_lifetime: value_to_u64(r.get("number_lifetime")),
            })
        })
        .collect()
}

/// `[{"countryCode":187,"numbersCount":44432},…]` — `countryCode` may be a string.
pub fn parse_service_numbers_count(body: &str) -> ApiResult<BTreeMap<CountryId, u64>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let rows = v
        .as_array()
        .ok_or_else(|| ApiError::Unexpected(body.trim().to_owned()))?;
    let mut out = BTreeMap::new();
    for r in rows {
        let country = value_to_u64(r.get("countryCode"))
            .ok_or_else(|| ApiError::Parse("row without countryCode".into()))?
            as CountryId;
        out.insert(country, value_to_u64(r.get("numbersCount")).unwrap_or(0));
    }
    Ok(out)
}

/// `{"verificationType":1,"sms":{"dateTime":"…","code":"852508","text":"…"}}` or
/// `{"verificationType":0,"sms":null}`; classic `STATUS_*` tokens are accepted defensively.
pub fn parse_status_v2(body: &str) -> ApiResult<StatusV2> {
    let trimmed = body.trim();
    if !trimmed.starts_with('{') {
        return TigerSmsDialect::default()
            .parse_status(trimmed)
            .map(StatusV2::Plain);
    }
    let v: Value = serde_json::from_str(trimmed)?;
    let sms = match v.get("sms") {
        Some(Value::Object(s)) => Some(Sms {
            date_time: value_to_string(s.get("dateTime")),
            code: value_to_string(s.get("code")),
            text: value_to_string(s.get("text")),
        }),
        _ => None,
    };
    Ok(StatusV2::Json {
        verification_type: value_to_u64(v.get("verificationType")),
        sms,
    })
}

/// `{"status":"success"}`
pub fn parse_set_status_v2(body: &str) -> ApiResult<()> {
    let trimmed = body.trim();
    let v: Value = serde_json::from_str(trimmed)?;
    match value_to_string(v.get("status")).as_deref() {
        Some("success") => Ok(()),
        _ => Err(ApiError::Unexpected(trimmed.to_owned())),
    }
}

/// Reshapes a `getPrices&service=…` table into [`TopCountry`] rows, most stock first.
pub fn top_countries_from_price_table(
    table: &PriceTable,
    service: &ServiceCode,
) -> Vec<TopCountry> {
    let mut rows: Vec<TopCountry> = table
        .iter()
        .filter_map(|(country, services)| {
            let price = services.get(service)?;
            Some(TopCountry {
                country: country.clone(),
                price: price.cost,
                retail_price: None,
                count: price.count,
                provider_id: None,
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.price.total_cmp(&b.price))
            .then_with(|| a.country.to_string().cmp(&b.country.to_string()))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SmsActivateApi;
    use crate::transport::FakeTransport;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../fixtures/tiger_sms/", $name))
        };
    }

    fn client(t: FakeTransport) -> TigerSms<FakeTransport> {
        Client::new(t, TigerSmsDialect::default(), "KEY")
    }

    fn only_request(c: &TigerSms<FakeTransport>) -> String {
        let reqs = c.transport().requests();
        assert_eq!(reqs.len(), 1);
        reqs[0].clone()
    }

    fn tg() -> ServiceCode {
        ServiceCode::from("tg")
    }

    #[test]
    fn identity_and_capabilities() {
        let c = client(FakeTransport::new());
        assert_eq!(c.provider(), "Tiger SMS");
        assert_eq!(c.dialect().endpoint(), ENDPOINT);
        assert_eq!(TigerSmsDialect::at(ALT_ENDPOINT).endpoint(), ALT_ENDPOINT);
        let caps = c.capabilities();
        assert!(caps.get_number_v2);
        assert!(caps.active_activations);
        assert!(caps.prices_v2);
        assert!(caps.prices_v3);
        assert!(caps.price_bounds);
        assert!(caps.provider_filters);
        assert!(!caps.numbers_status);
        assert!(!caps.operators);
    }

    #[test]
    fn unsupported_actions_do_not_hit_the_network() {
        let c = client(FakeTransport::new());
        assert!(matches!(
            c.get_numbers_status(&CountryRef::Id(187), None),
            Err(ApiError::Unsupported("getNumbersStatus"))
        ));
        assert!(matches!(
            c.get_operators(None),
            Err(ApiError::Unsupported("getOperators"))
        ));
        assert!(c.transport().requests().is_empty());
    }

    #[test]
    fn balance_plain_and_json() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getBalance.txt"))
                .push(200, fixture!("post_getBalance.txt"))
                .push(200, fixture!("getBalance_format_json.txt")),
        );
        assert_eq!(c.get_balance().unwrap(), 4.682);
        assert_eq!(c.get_balance().unwrap(), 4.682);
        assert_eq!(
            c.get_balance_json().unwrap(),
            BalanceInfo {
                balance: 4.682,
                currency: CURRENCY_USD
            }
        );
        let reqs = c.transport().requests();
        assert_eq!(reqs[0], format!("{ENDPOINT}?api_key=KEY&action=getBalance"));
        assert!(reqs[2].ends_with("action=getBalance&format=json"));
    }

    #[test]
    fn prices_with_string_costs() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getPrices_service_tg.txt"))
                .push(200, fixture!("getPrices_service_tg_country_187.txt"))
                .push(200, "{}"),
        );
        let all = c.get_prices(Some(&tg()), None).unwrap();
        assert_eq!(all.len(), 197);
        assert_eq!(all[&CountryRef::Id(1)][&tg()].cost, 0.334);
        assert_eq!(all[&CountryRef::Id(1)][&tg()].count, 9292);
        assert_eq!(all[&CountryRef::Id(187)][&tg()].physical_count, None);

        let usa = c
            .get_prices(Some(&tg()), Some(&CountryRef::Id(187)))
            .unwrap();
        assert_eq!(usa.len(), 1);
        assert_eq!(usa[&CountryRef::Id(187)][&tg()].cost, 0.25);
        assert_eq!(usa[&CountryRef::Id(187)][&tg()].count, 44432);
        assert!(c.get_prices(None, None).unwrap().is_empty());

        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getPrices&service=tg"));
        assert!(reqs[1].ends_with("action=getPrices&service=tg&country=187"));
        assert!(reqs[2].ends_with("action=getPrices"));
    }

    #[test]
    fn services_plain_and_filtered() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getServicesList.txt"))
                .push(200, fixture!("getServicesList_country_187_lang_ru.txt")),
        );
        let all = c.get_services().unwrap();
        assert_eq!(all.len(), 977);
        assert!(
            all.iter()
                .any(|s| s.code.as_str() == "tg" && s.name == "Telegram")
        );
        let usa = c.get_services_for(Some(187), Some("ru")).unwrap();
        assert_eq!(usa.len(), 580);
        assert!(usa.iter().any(|s| s.code.as_str() == "tg"));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getServicesList"));
        assert!(reqs[1].ends_with("action=getServicesList&country=187&lang=ru"));
    }

    #[test]
    fn countries_array_form() {
        let c = client(FakeTransport::new().push(200, fixture!("getCountries.txt")));
        let countries = c.get_countries().unwrap();
        assert!(countries.len() > 150);
        let af = countries.iter().find(|c| c.id() == Some(74)).unwrap();
        assert_eq!(af.name_en, "Afghanistan");
        assert_eq!(af.name_ru.as_deref(), Some("Афганистан"));
        assert_eq!(af.visible, Some(true));
        assert_eq!(af.retry, Some(true));
        assert_eq!(af.rent, None);
        assert!(countries.iter().any(|c| c.id() == Some(187)));
        assert!(countries.windows(2).all(|w| w[0].key < w[1].key));
        assert!(only_request(&c).ends_with("action=getCountries"));
    }

    #[test]
    fn bad_key_is_a_plain_401() {
        let c = client(
            FakeTransport::new()
                .push(401, fixture!("badkey_getBalance.txt"))
                .push(401, r#"{"title":"BAD_KEY","details":"Unauthorized"}"#),
        );
        assert!(matches!(c.get_balance(), Err(ApiError::BadKey)));
        assert!(matches!(c.get_balance(), Err(ApiError::BadKey)));
    }

    #[test]
    fn no_activation_for_unknown_and_non_numeric_ids() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getStatus_id_1.txt"))
                .push(200, fixture!("getStatus_id_abc.txt"))
                .push(200, fixture!("setStatus_id_1_status_8.txt")),
        );
        assert!(matches!(
            c.get_status(&ActivationId::from("1")),
            Err(ApiError::NoActivation)
        ));
        assert!(matches!(
            c.get_status(&ActivationId::from("abc")),
            Err(ApiError::NoActivation)
        ));
        assert!(matches!(
            c.cancel(&ActivationId::from("1")),
            Err(ApiError::NoActivation)
        ));
        let reqs = c.transport().requests();
        assert!(reqs[1].ends_with("action=getStatus&id=abc"));
        assert!(reqs[2].ends_with("action=setStatus&id=1&status=8"));
    }

    #[test]
    fn bad_action_tokens() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("nosuchaction.txt"))
                .push(200, fixture!("getTopCountriesByService_service_tg.txt"))
                .push(200, fixture!("getServices.txt"))
                .push(200, fixture!("getFullSms_id_1.txt")),
        );
        assert!(matches!(
            c.call("noSuchAction", Vec::new()),
            Err(ApiError::BadAction(_))
        ));
        assert!(matches!(
            c.get_top_countries(&tg()),
            Err(ApiError::BadAction(_))
        ));
        assert!(matches!(
            c.call("getServices", Vec::new()),
            Err(ApiError::BadAction(_))
        ));
        assert!(matches!(
            c.call("getFullSms", vec![("id".into(), "1".into())]),
            Err(ApiError::BadAction(_))
        ));
    }

    #[test]
    fn json_not_found_envelopes_on_404() {
        let c = client(
            FakeTransport::new()
                .push(404, fixture!("getStatusV2_id_1.txt"))
                .push(404, fixture!("setStatusV2_id_1_status_8.txt")),
        );
        let id = ActivationId::from("1");
        assert!(matches!(c.get_status_v2(&id), Err(ApiError::NoActivation)));
        assert!(matches!(
            c.set_status_v2(&id, StatusAction::Cancel),
            Err(ApiError::NoActivation)
        ));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getStatusV2&id=1"));
        assert!(reqs[1].ends_with("action=setStatusV2&id=1&status=8"));
    }

    #[test]
    fn get_number_probe_envelopes_v2_and_v1() {
        // getNumberV2: JSON envelope with HTTP 200 (live fixture).
        let c = client(FakeTransport::new().push(
            200,
            fixture!("getNumberV2_service___probe___country_187.txt"),
        ));
        assert!(matches!(
            c.get_number(&NumberRequest::new("__probe__", 187)),
            Err(ApiError::BadService)
        ));
        assert!(only_request(&c).ends_with("action=getNumberV2&service=__probe__&country=187"));
        // getNumber (v1): plain token (live fixture), parsed through the same classifier.
        let resp = HttpResponse::new(200, fixture!("getNumber_service___probe___country_187.txt"));
        assert!(matches!(classify(&resp), Err(ApiError::BadService)));
    }

    #[test]
    fn documented_envelopes_and_tokens() {
        let env = |status, body: &str| classify(&HttpResponse::new(status, body));
        assert!(matches!(
            env(400, r#"{"title":"WRONG_MAX_PRICE","details":"The maximum price is less than the permitted price","info":{"min":0.1453}}"#),
            Err(ApiError::WrongMaxPrice { min: Some(m) }) if m == 0.1453
        ));
        assert!(
            matches!(env(400, "WRONG_MAX_PRICE:0.1453"), Err(ApiError::WrongMaxPrice { min: Some(m) }) if m == 0.1453)
        );
        assert!(matches!(
            env(
                402,
                r#"{"title":"NO_BALANCE","details":"Insufficient balance"}"#
            ),
            Err(ApiError::NoBalance)
        ));
        assert!(matches!(
            env(
                200,
                r#"{"title":"NO_NUMBERS","details":"No numbers available for the requested service and country"}"#
            ),
            Err(ApiError::NoNumbers)
        ));
        assert!(matches!(
            env(
                200,
                r#"{"title":"BAD_COUNTRY","details":"Unknown country code."}"#
            ),
            Err(ApiError::BadCountry)
        ));
        assert!(matches!(
            env(200, r#"{"title":"NO_PROVIDERS","details":"None of the selected providers currently offer this service/country"}"#),
            Err(ApiError::Other(t)) if t == "NO_PROVIDERS: None of the selected providers currently offer this service/country"
        ));
        match env(
            422,
            r#"{"title":"UNPROCESSABLE_ENTITY","details":"The service field is required."}"#,
        ) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "");
                assert_eq!(message, "The service field is required.");
            }
            other => panic!("unexpected: {other:?}"),
        }
        match env(
            422,
            r#"{"title":"UNPROCESSABLE_ENTITY","details":"Validation failed","info":{"field":"country","code":"INVALID","message":"Param 'country' must be a number"}}"#,
        ) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "country");
                assert_eq!(message, "Param 'country' must be a number");
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            env(
                200,
                r#"{"title":"BAD_STATUS","details":"Wrong status value"}"#
            ),
            Err(ApiError::BadStatus)
        ));
        assert!(matches!(
            env(
                409,
                r#"{"title":"EARLY_CANCEL_DENIED","details":"Early cancel denied","info":{"minActivationTime":120}}"#
            ),
            Err(ApiError::EarlyCancelDenied)
        ));
        assert!(matches!(
            env(429, r#"{"title":"RATE_LIMIT","details":""}"#),
            Err(ApiError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            env(429, "Too Many Requests"),
            Err(ApiError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            env(404, r#"{"title":"BAD_ACTION","details":"Method Not Found"}"#),
            Err(ApiError::BadAction(d)) if d.is_empty()
        ));
        // 422 whose `details` is the first validation token (documented for getNumberV2).
        match env(
            422,
            r#"{"title":"UNPROCESSABLE_ENTITY","details":"BAD_MAX_PRICE"}"#,
        ) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "maxPrice");
                assert_eq!(message, "BAD_MAX_PRICE");
            }
            other => panic!("unexpected: {other:?}"),
        }
        match env(
            422,
            r#"{"title":"UNPROCESSABLE_ENTITY","details":"BAD_FIXED_PRICE"}"#,
        ) {
            Err(ApiError::Validation { field, .. }) => assert_eq!(field, "fixedPrice"),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            env(
                422,
                r#"{"title":"UNPROCESSABLE_ENTITY","details":"BAD_SERVICE"}"#
            ),
            Err(ApiError::BadService)
        ));
        assert!(matches!(
            env(
                422,
                r#"{"title":"UNPROCESSABLE_ENTITY","details":"BAD_COUNTRY"}"#
            ),
            Err(ApiError::BadCountry)
        ));
        // `info.field` wins over a token in `details`; an `Other`-class token stays a Validation.
        match env(
            422,
            r#"{"title":"UNPROCESSABLE_ENTITY","details":"BAD_SERVICE","info":{"field":"service","message":"Unknown service"}}"#,
        ) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "service");
                assert_eq!(message, "Unknown service");
            }
            other => panic!("unexpected: {other:?}"),
        }
        match env(422, r#"{"title":"UNPROCESSABLE_ENTITY","details":"ERROR"}"#) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "");
                assert_eq!(message, "ERROR");
            }
            other => panic!("unexpected: {other:?}"),
        }
        // HTTP 429 wins over any token body; 5xx and unmapped 4xx keep their status.
        assert!(matches!(
            env(429, "RATE_LIMIT"),
            Err(ApiError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            env(429, "TOO_MANY_REQUESTS"),
            Err(ApiError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            env(
                429,
                r#"{"title":"RATE_LIMIT","details":"","info":{"retry_after_seconds":5}}"#
            ),
            Err(ApiError::RateLimited {
                retry_after: Some(5)
            })
        ));
        assert!(matches!(
            env(502, "BAD_GATEWAY"),
            Err(ApiError::Http { status: 502, body }) if body == "BAD_GATEWAY"
        ));
        assert!(matches!(
            env(500, "BAD_ACTION"),
            Err(ApiError::Http { status: 500, .. })
        ));
        assert!(matches!(
            env(403, "FORBIDDEN"),
            Err(ApiError::Http { status: 403, .. })
        ));
        assert!(matches!(env(402, "NO_BALANCE"), Err(ApiError::NoBalance)));
        // Tiger-only validation tokens on HTTP 200.
        match env(200, "BAD_MAX_PRICE") {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "maxPrice");
                assert_eq!(message, "BAD_MAX_PRICE");
            }
            other => panic!("unexpected: {other:?}"),
        }
        for (token, field) in [
            ("BAD_VALUES", ""),
            ("BAD_MULTIPLE", "multiple"),
            ("BAD_PROVIDER_IDS", "providerIds"),
            ("BAD_EXCEPT_PROVIDER_IDS", "exceptProviderIds"),
            ("BAD_ACTIVATION_TYPE", "activationType"),
            ("BAD_FIXED_PRICE", "fixedPrice"),
        ] {
            assert!(
                matches!(env(200, token), Err(ApiError::Validation { field: f, .. }) if f == field)
            );
        }
        assert!(matches!(env(200, "NO_PROVIDERS"), Err(ApiError::Other(t)) if t == "NO_PROVIDERS"));
        assert!(matches!(env(200, "ERROR"), Err(ApiError::Other(t)) if t == "ERROR"));
        assert!(matches!(
            env(200, "EARLY_CANCEL_DENIED"),
            Err(ApiError::EarlyCancelDenied)
        ));
        assert!(matches!(
            env(503, "Service Unavailable"),
            Err(ApiError::Http { status: 503, .. })
        ));
        // Data passes through, whatever its shape.
        assert!(env(200, "ACCESS_BALANCE:1").is_ok());
        assert!(env(200, r#"{"status":"success"}"#).is_ok());
        assert!(env(200, "[]").is_ok());
        assert!(env(200, fixture!("getProviders_service_tg_country_187.txt")).is_ok());
    }

    #[test]
    fn get_number_v2_url_encoding_with_tiger_params() {
        let req = NumberRequest::new("tg", 187).max_price(0.25);
        let options = TigerNumberOptions::new()
            .multiple(true)
            .provider_ids([14, 163])
            .except_provider_ids([36])
            .referral("partner-1")
            .activation_type(ActivationType::CallFlash)
            .fixed_price(true);
        let c = client(FakeTransport::new().push(
            200,
            r#"{"activationId":"557860099","phoneNumber":"18734721259","activationCost":0.05,"currency":840,"countryCode":6,"countryPhoneCode":1,"canGetAnotherSms":true,"activationTime":"2026-07-02T18:09:54+00:00","activationEndTime":"2026-07-02T18:29:54+00:00","activationOperator":"any"}"#,
        ));
        let a = c.get_number_with(&req, &options).unwrap();
        assert_eq!(a.id.as_str(), "557860099");
        assert_eq!(a.phone, "18734721259");
        assert_eq!(a.cost, Some(0.05));
        assert_eq!(a.country, Some(CountryRef::Id(6)));
        assert_eq!(a.can_get_another_sms, Some(true));
        assert_eq!(a.operator.as_deref(), Some("any"));
        assert_eq!(
            only_request(&c),
            format!(
                "{ENDPOINT}?api_key=KEY&action=getNumberV2&service=tg&country=187&maxPrice=0.25\
                 &multiple=1&providerIds=14%2C163&exceptProviderIds=36&ref=partner-1\
                 &activationType=CALL_FLASH&fixedPrice=true"
            )
        );
    }

    #[test]
    fn number_options_apply_and_replace_previous_values() {
        let bare = NumberRequest::new("tg", 187).extra("providerIds", "1,2");
        let applied = TigerNumberOptions::new()
            .provider_ids([3])
            .multiple(true)
            .fixed_price(true) // dropped: no max_price
            .apply(&bare);
        // Wire order is multiple → providerIds → …; the replaced `providerIds` moves to the end.
        assert_eq!(
            applied.extra,
            vec![
                ("multiple".to_owned(), "1".to_owned()),
                ("providerIds".to_owned(), "3".to_owned()),
            ]
        );
        // Defaults add nothing; empty filters are not sent.
        assert_eq!(TigerNumberOptions::default().apply(&bare), bare);
        assert_eq!(
            TigerNumberOptions::new()
                .provider_ids(Vec::<u64>::new())
                .multiple(false)
                .apply(&bare),
            bare
        );
        let with_max = NumberRequest::new("tg", 187).max_price(0.3);
        assert_eq!(
            TigerNumberOptions::new()
                .fixed_price(true)
                .apply(&with_max)
                .extra,
            vec![("fixedPrice".to_owned(), "true".to_owned())]
        );
        assert_eq!(ActivationType::default().as_str(), "SMS");
        assert_eq!(ActivationType::RentSms.as_str(), "RENT_SMS");
    }

    #[test]
    fn ignored_number_params_are_never_sent_and_rejected_by_get_number_with() {
        // Trait path: minPrice / operator are stripped by adjust_params (provider ignores them).
        let c = client(FakeTransport::new().push(200, "ACCESS_NUMBER:1:2"));
        let req = NumberRequest::new("tg", 187)
            .operator("any")
            .max_price(0.5)
            .min_price(0.1);
        assert_eq!(c.get_number(&req).unwrap().phone, "2");
        assert!(
            only_request(&c).ends_with("action=getNumberV2&service=tg&country=187&maxPrice=0.5")
        );
        // Other actions are untouched.
        let mut params = vec![("operator".to_owned(), "any".to_owned())];
        TigerSmsDialect::default().adjust_params("getNumbersStatus", &mut params);
        assert_eq!(params.len(), 1);

        // Typed path: refuse before any request.
        let c = client(FakeTransport::new());
        let options = TigerNumberOptions::new();
        assert!(matches!(
            c.get_number_with(&NumberRequest::new("tg", 187).min_price(0.1), &options),
            Err(ApiError::Unsupported("minPrice"))
        ));
        assert!(matches!(
            c.get_number_with(&NumberRequest::new("tg", 187).operator("mts"), &options),
            Err(ApiError::Unsupported("operator"))
        ));
        assert!(matches!(
            c.get_number_with(
                &NumberRequest::new("tg", 187),
                &TigerNumberOptions::new().fixed_price(true)
            ),
            Err(ApiError::Validation { field, .. }) if field == "maxPrice"
        ));
        assert!(c.transport().requests().is_empty());
    }

    #[test]
    fn free_prices_and_v2_alias() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getFreePrices_service_tg_country_187.txt"))
                .push(200, fixture!("getPricesV2_service_tg_country_187.txt"))
                .push(200, "{}"),
        );
        let free = c.get_free_prices(Some(&tg()), Some(187)).unwrap();
        let v2 = c.get_prices_v2(Some(&tg()), Some(187)).unwrap();
        assert_eq!(free, v2);
        let ladder = &free[&187][&tg()];
        assert_eq!(ladder.buckets.len(), 12);
        assert_eq!(ladder.sale_average_price, Some(0.199));
        let first = ladder.cheapest().unwrap();
        assert_eq!(first.price, 0.25);
        assert_eq!(first.count, 22518);
        assert_eq!(first.has_multi, Some(true));
        assert_eq!(ladder.buckets[2].price, 0.255);
        assert_eq!(ladder.buckets[2].has_multi, Some(false));
        assert!(
            ladder
                .buckets
                .windows(2)
                .all(|w| w[0].price < w[1].price && w[0].count <= w[1].count)
        );
        assert_eq!(ladder.total_count(), 44432);
        assert_eq!(ladder.cheapest_with(40000).unwrap().price, 1.058);
        assert_eq!(ladder.cheapest_multi().unwrap().price, 0.25);
        // saleAveragePrice (0.199) is below the floor, so the floor wins.
        assert_eq!(ladder.recommended_max_price(), Some(0.25));
        assert!(c.get_prices_v2(None, None).unwrap().is_empty());
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getFreePrices&service=tg&country=187"));
        assert!(reqs[1].ends_with("action=getPricesV2&service=tg&country=187"));
        assert!(reqs[2].ends_with("action=getPricesV2"));

        // Documented example.
        let doc = parse_free_prices(
            r#"{"6":{"tg":{"prices":{"0.1491":100,"0.1650":40},"has_multi":{"0.1491":false,"0.1650":true},"saleAveragePrice":0.1554}}}"#,
        )
        .unwrap();
        let l = &doc[&6][&tg()];
        assert_eq!(l.recommended_max_price(), Some(0.1554));
        assert_eq!(l.cheapest_multi().unwrap().price, 0.165);
        assert!(PriceLadder::default().recommended_max_price().is_none());
    }

    #[test]
    fn prices_v3_provider_offers() {
        let c = client(
            FakeTransport::new().push(200, fixture!("getPricesV3_service_tg_country_187.txt")),
        );
        let table = c.get_prices_v3(&tg(), 187).unwrap();
        assert!(only_request(&c).ends_with("action=getPricesV3&service=tg&country=187"));
        let cell = &table[&187][&tg()];
        assert_eq!(cell.price, 0.25);
        assert_eq!(cell.count, 44432);
        assert_eq!(cell.currency, Some(CURRENCY_USD));
        assert_eq!(cell.sale_average_price, Some(0.199));
        assert_eq!(cell.providers.len(), 16);
        let best = &cell.providers[0];
        assert_eq!(best.provider_id, 14);
        assert_eq!(best.count, 22518);
        assert_eq!(best.prices, vec![0.25]);
        assert_eq!(best.cheapest(), Some(0.25));
        // ties on price are ordered by id
        assert_eq!(cell.providers[1].provider_id, 52);
        assert_eq!(cell.providers[2].provider_id, 163);
        assert_eq!(cell.providers[3].provider_id, 784);
        assert!(
            cell.providers
                .windows(2)
                .all(|w| w[0].cheapest() <= w[1].cheapest())
        );
        assert_eq!(cell.providers.last().unwrap().cheapest(), Some(1.236));

        let doc = parse_prices_v3(
            r#"{"6":{"tg":{"price":0.15,"count":25370,"currency":840,"saleAveragePrice":0.15,"providers":{"235":{"count":5603,"price":[0.23],"provider_id":235}}}}}"#,
        )
        .unwrap();
        assert_eq!(doc[&6][&tg()].providers[0].provider_id, 235);
        let empty =
            parse_prices_v3(r#"{"6":{"tg":{"price":0.15,"count":0,"providers":{}}}}"#).unwrap();
        assert!(empty[&6][&tg()].providers.is_empty());
    }

    #[test]
    fn offers_service_first() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getOffers_services_tg_countries_187.txt"))
                .push(200, r#"{"data":{}}"#),
        );
        let offers = c.get_offers(&[tg()], &[187]).unwrap();
        assert_eq!(offers.len(), 1);
        let o = &offers[&tg()][&187];
        assert_eq!(o.prices.default, 0.25);
        assert_eq!(o.prices.avg, Some(0.199));
        assert_eq!(o.prices.retail, Some(0.25));
        assert_eq!(o.prices.min, Some(0.25));
        assert_eq!(o.counts.total, 44432);
        assert_eq!(o.counts.default_price, Some(22518));
        assert_eq!(o.map.len(), 12);
        assert_eq!(o.map[0].price, 0.25);
        assert_eq!(o.map[0].count, 22518);
        assert_eq!(o.map[0].has_multi, None);
        assert_eq!(o.map.last().unwrap().count, o.counts.total);
        assert!(c.get_offers(&[], &[]).unwrap().is_empty());
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getOffers&services=tg&countries=187"));
        assert!(reqs[1].ends_with("action=getOffers"));

        let doc = parse_offers(
            r#"{"data":{"tg":{"6":{"prices":{"default":0.16,"avg":0.16,"retail":0.165,"min":0.15},"counts":{"total":22598,"defaultPrice":4787},"map":{"0.1500":4787,"0.1765":12043,"1.7648":5814}}}}}"#,
        )
        .unwrap();
        assert_eq!(doc[&tg()][&6].prices.min, Some(0.15));
        assert_eq!(doc[&tg()][&6].map[2].price, 1.7648);
        let multi = c.get_offers(&[tg(), ServiceCode::from("go")], &[6, 33]);
        assert!(multi.is_err()); // no response queued — only the URL matters here
        assert!(
            c.transport().requests()[2]
                .ends_with("action=getOffers&services=tg%2Cgo&countries=6%2C33")
        );
    }

    #[test]
    fn providers_html_wrapper() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getProviders_service_tg_country_187.txt"))
                .push(
                    200,
                    r#"<html><head><meta charset="utf-8"></head><body>[{"id":235,"name":"Provider235","numbers_count":5603,"delivery_percent":97.5,"number_lifetime":1200}]</body></html>"#,
                )
                .push(200, "BAD_VALUES")
                .push(200, "ERROR"),
        );
        let providers = c.get_providers(Some(&tg()), Some(187)).unwrap();
        assert_eq!(providers.len(), 16);
        assert_eq!(providers[0].id, 14);
        assert_eq!(providers[0].name, "Provider14");
        assert_eq!(providers[0].numbers_count, 22518);
        assert_eq!(providers[0].delivery_percent, Some(8.95));
        assert_eq!(providers[0].number_lifetime, Some(20));
        let p52 = providers.iter().find(|p| p.id == 52).unwrap();
        assert_eq!(p52.delivery_percent, None);
        let doc = c.get_providers(None, None).unwrap();
        assert_eq!(doc[0].id, 235);
        assert_eq!(doc[0].delivery_percent, Some(97.5));
        assert!(matches!(
            c.get_providers(None, None),
            Err(ApiError::Validation { .. })
        ));
        assert!(matches!(c.get_providers(None, None), Err(ApiError::Other(t)) if t == "ERROR"));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getProviders&service=tg&country=187"));
        assert!(reqs[1].ends_with("action=getProviders"));
        assert!(parse_providers("[]").unwrap().is_empty());
        assert!(parse_providers("<html></html>").is_err());
    }

    #[test]
    fn service_numbers_count_numeric_and_string_codes() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getServiceNumbersCount_service_tg.txt"))
                .push(200, r#"[{"countryCode":"6","numbersCount":25370}]"#)
                .push(200, "BAD_SERVICE"),
        );
        let counts = c.get_service_numbers_count(&tg()).unwrap();
        assert_eq!(counts.len(), 198);
        assert_eq!(counts[&187], 44432);
        assert_eq!(c.get_service_numbers_count(&tg()).unwrap()[&6], 25370);
        assert!(matches!(
            c.get_service_numbers_count(&ServiceCode::from("__probe__")),
            Err(ApiError::BadService)
        ));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getServiceNumbersCount&service=tg"));
    }

    #[test]
    fn active_activations_empty_and_documented_rows() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getActiveActivations.txt"))
                .push(200, fixture!("getActiveActivations_start_0_limit_1.txt"))
                .push(
                    200,
                    r#"{"status":"success","data":[{"activationId":"557860099","serviceCode":"tg","phoneNumber":"18734721259","activationCost":0.1453,"activationStatus":"6","smsCode":"","smsText":"","activationTime":"2026-07-02T18:09:54+00:00","countryCode":6,"countryName":"United States","canGetAnotherSms":true,"currency":840}]}"#,
                ),
        );
        assert!(c.get_active_activations().unwrap().is_empty());
        assert!(c.get_active_activations_page(0, 1).unwrap().is_empty());
        let rows = c.get_active_activations_page(0, 50).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id.as_str(), "557860099");
        assert_eq!(rows[0].service, Some(tg()));
        assert_eq!(rows[0].phone.as_deref(), Some("18734721259"));
        assert_eq!(rows[0].cost, Some(0.1453));
        assert_eq!(rows[0].status.as_deref(), Some("6"));
        assert_eq!(rows[0].sms_code, None);
        assert_eq!(rows[0].country, Some(CountryRef::Id(6)));
        assert_eq!(rows[0].can_get_another_sms, Some(true));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getActiveActivations"));
        assert!(reqs[1].ends_with("action=getActiveActivations&start=0&limit=1"));
    }

    #[test]
    fn status_tokens_including_access_cancel() {
        let c = client(
            FakeTransport::new()
                .push(200, "STATUS_WAIT_CODE")
                .push(200, "STATUS_WAIT_RETRY:8522")
                .push(200, "STATUS_OK:852508")
                .push(200, "ACCESS_CANCEL")
                .push(200, "STATUS_OK:Your Telegram code is 852508")
                .push(200, "ACCESS_READY")
                .push(200, "ACCESS_ACTIVATION")
                .push(200, "BAD_STATUS")
                .push(200, "EARLY_CANCEL_DENIED"),
        );
        let id = ActivationId::from("42");
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::WaitCode);
        assert_eq!(
            c.get_status(&id).unwrap(),
            ActivationStatus::WaitRetry {
                last_code: "8522".into()
            }
        );
        assert_eq!(c.get_status(&id).unwrap().code(), Some("852508"));
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::Cancelled);
        assert_eq!(
            c.get_status_full_text(&id).unwrap().code(),
            Some("Your Telegram code is 852508")
        );
        assert_eq!(
            c.set_status(&id, StatusAction::Ready).unwrap(),
            StatusAck::Ready
        );
        assert_eq!(c.complete(&id).unwrap(), StatusAck::Activation);
        assert!(matches!(c.complete(&id), Err(ApiError::BadStatus)));
        assert!(matches!(c.cancel(&id), Err(ApiError::EarlyCancelDenied)));
        let reqs = c.transport().requests();
        assert!(reqs[4].ends_with("action=getStatus&id=42&full_text=true"));
        assert!(reqs[5].ends_with("action=setStatus&id=42&status=1"));
    }

    #[test]
    fn status_v2_documented_shapes() {
        let c = client(
            FakeTransport::new()
                .push(200, r#"{"verificationType":0,"sms":null}"#)
                .push(
                    200,
                    r#"{"verificationType":1,"sms":{"dateTime":"2026-07-02T18:12:31+00:00","code":"852508","text":"Your Telegram code is 852508"}}"#,
                )
                .push(200, "STATUS_CANCEL"),
        );
        let id = ActivationId::from("7");
        let waiting = c.get_status_v2(&id).unwrap();
        assert_eq!(
            waiting,
            StatusV2::Json {
                verification_type: Some(0),
                sms: None
            }
        );
        assert_eq!(waiting.code(), None);
        let done = c.get_status_v2(&id).unwrap();
        assert_eq!(done.code(), Some("852508"));
        match &done {
            StatusV2::Json {
                verification_type: Some(1),
                sms: Some(sms),
            } => {
                assert_eq!(sms.date_time.as_deref(), Some("2026-07-02T18:12:31+00:00"));
                assert_eq!(sms.text.as_deref(), Some("Your Telegram code is 852508"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            c.get_status_v2(&id).unwrap(),
            StatusV2::Plain(ActivationStatus::Cancelled)
        );
        assert!(c.transport().requests()[0].ends_with("action=getStatusV2&id=7"));
    }

    #[test]
    fn set_status_v2_success_and_errors() {
        let c = client(
            FakeTransport::new()
                .push(200, r#"{"status":"success"}"#)
                .push(200, r#"{"title":"BAD_STATUS","details":"Wrong status value"}"#)
                .push(
                    409,
                    r#"{"title":"EARLY_CANCEL_DENIED","details":"Early cancel denied","info":{"minActivationTime":120}}"#,
                )
                .push(200, r#"{"status":"nope"}"#),
        );
        let id = ActivationId::from("7");
        c.set_status_v2(&id, StatusAction::Complete).unwrap();
        assert!(matches!(
            c.set_status_v2(&id, StatusAction::Complete),
            Err(ApiError::BadStatus)
        ));
        assert!(matches!(
            c.set_status_v2(&id, StatusAction::Cancel),
            Err(ApiError::EarlyCancelDenied)
        ));
        assert!(matches!(
            c.set_status_v2(&id, StatusAction::Cancel),
            Err(ApiError::Unexpected(_))
        ));
        assert!(c.transport().requests()[0].ends_with("action=setStatusV2&id=7&status=6"));
    }

    #[test]
    fn top_countries_are_derived_from_prices() {
        let c = client(FakeTransport::new().push(200, fixture!("getPrices_service_tg.txt")));
        let rows = c.top_countries_from_prices(&tg()).unwrap();
        assert!(only_request(&c).ends_with("action=getPrices&service=tg"));
        assert_eq!(rows.len(), 197);
        assert!(rows.windows(2).all(|w| w[0].count >= w[1].count));
        let usa = rows
            .iter()
            .find(|r| r.country == CountryRef::Id(187))
            .unwrap();
        assert_eq!(usa.price, 0.25);
        assert_eq!(usa.count, 44432);
        assert_eq!(usa.retail_price, None);
        assert_eq!(usa.provider_id, None);
        assert!(top_countries_from_price_table(&PriceTable::new(), &tg()).is_empty());
    }

    #[test]
    fn dialect_errors_short_circuit_extra_actions() {
        let c = client(
            FakeTransport::new()
                .push(401, "BAD_KEY")
                .push(200, "BAD_ACTION")
                .push(200, "BAD_SERVICE")
                .push(
                    200,
                    r#"{"title":"BAD_SERVICE","details":"Unknown or missing service code."}"#,
                ),
        );
        assert!(matches!(
            c.get_free_prices(None, None),
            Err(ApiError::BadKey)
        ));
        assert!(matches!(
            c.get_offers(&[], &[]),
            Err(ApiError::BadAction(_))
        ));
        assert!(matches!(
            c.get_free_prices(Some(&ServiceCode::from("__probe__")), None),
            Err(ApiError::BadService)
        ));
        assert!(matches!(
            c.get_prices_v3(&ServiceCode::from("__probe__"), 187),
            Err(ApiError::BadService)
        ));
    }
}
