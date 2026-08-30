//! Hero-SMS — <https://hero-sms.com/api#tag/sms-activate/>
//!
//! The sms-activate-compatible dialect of Hero-SMS. Everything below was verified against live
//! responses captured on 2026-08-30 (`fixtures/hero_sms/`) and the provider's OpenAPI document
//! (`https://hero-sms.com/docs/v1/openapi.json`, the source behind the JS-rendered docs page).
//!
//! # Endpoint and authentication
//!
//! * `GET https://hero-sms.com/stubs/handler_api.php?api_key=<key>&action=<action>&…`
//! * The key travels as the `api_key` query parameter (the REST API at `/api/v1` also accepts an
//!   `Authorization: ApiKey <key>` header, but the compatibility endpoint is query-based).
//! * Unknown query parameters are ignored (verified: `getBalance&foo=bar&minPrice=0.1` answered
//!   normally), so passing a parameter Hero-SMS does not document is harmless.
//!
//! # Capability matrix
//!
//! | action | status | evidence |
//! | --- | --- | --- |
//! | `getBalance` | ✔ | `ACCESS_BALANCE:12.5085` |
//! | `getNumber` | ✔ | `ACCESS_NUMBER:<id>:<phone>`; invalid service → HTTP 200 **plain** `UNPROCESSABLE_ENTITY:service:INVALID` |
//! | `getNumberV2` | ✔ (default) | invalid service → HTTP 422 validation envelope (action exists; documented JSON shape) |
//! | `getStatus` / `setStatus` | ✔ | standard tokens; unknown id → HTTP 404 `NOT_FOUND`. Only `status` 3 / 6 / 8 are documented — [`StatusAction::Ready`] (1) is **not** and is expected to answer HTTP 400 `BAD_STATUS` |
//! | `getStatusV2` | ✔ | JSON `{verificationType,data:{id,phoneFrom,code,text,service,date,type}}` or `STATUS_CANCEL`; unknown id → 404 |
//! | `getPrices[&service][&country]` | ✔ | `{country:{service:{cost,count,physicalCount}}}`; `{}` when nothing matches; full table ≈ 1 MB |
//! | `getPricesV2` / `getPricesV3` | ✘ | HTTP 404 `BAD_ACTION` |
//! | `getServicesList[&country][&lang]` | ✔ | `{status,services:[{code,name}]}`; `country` filters, `lang` ∈ en cn es de fr pt ru id vi tr |
//! | `getServices` / `getFullSms` | ✘ | HTTP 404 `BAD_ACTION` |
//! | `getCountries` | ✔ | `{id:{id:int,rus,eng,chn,visible,retry,rent}}` (docs show an array; the shared parser accepts both) |
//! | `getTopCountriesByService&service=` | ✔ | `{idx:{country,price,retail_price,count}}`; `&freePrice=true` adds `freePriceMap` |
//! | `getTopCountriesByService` (no service) | ✔ | `{service:{idx:{…}}}` — 1.35 MB, 725 services (the docs wrap it in a one-element array; both accepted) |
//! | `getNumbersStatus&country=` | ✔ | `{service:count}`; `country` is **required** (422 otherwise) |
//! | `getOperators[&country]` | ✔ | `{status,countryOperators:{country:[…]}}` (137 countries without the filter) |
//! | `getActiveActivations[&start][&limit]` | ✔ | `{status,data:[…],activeActivations:{affected_rows,num_rows,row,rows}}` |
//! | `getHistory[&start][&end][&offset][&size]` | ✔ | `[{id,date,phone,sms,cost,status,currency}]` |
//! | `getAllSms&id=[&size][&page]` | ✔ | `{data:[otp…],meta:{total,service}}` (documented; a live page was not observable — unknown id → 404) |
//! | `finishActivation` / `cancelActivation` | ✔ | HTTP 204 empty body; unknown id → 404 |
//! | `maxPrice`, `fixedPrice`, `ref`, `phoneException` on `getNumber*` | ✔ (documented) | see [`HeroNumberOptions`] |
//! | `minPrice` on `getNumber*` | ✘ | not part of the API (the server ignores unknown parameters); [`HeroSmsDialect::adjust_params`] strips it and [`HeroSms::get_number_with`] rejects it |
//! | `providerIds` / `exceptProviderIds` | ✘ | not documented; ignored like any unknown parameter |
//!
//! # Error shapes
//!
//! Hero-SMS uses real HTTP status codes with a JSON envelope
//! `{"title":"<CODE>","details":"<text>","info":{…}}` (`info` is optional):
//!
//! | HTTP | title | mapped to |
//! | --- | --- | --- |
//! | 401 | `BAD_KEY` | [`ApiError::BadKey`] |
//! | 404 | `NOT_FOUND` ("Activation Not Found") | [`ApiError::NoActivation`] |
//! | 404 | `BAD_ACTION` ("Method Not Found") | [`ApiError::BadAction`] |
//! | 422 | `UNPROCESSABLE_ENTITY`, `info:{field,code,message}` (also for a missing `api_key`) | [`ApiError::Validation`] |
//! | 429 | `RATE_LIMIT` | [`ApiError::RateLimited`] |
//! | 400 | `WRONG_MAX_PRICE`, `info:{min}` | [`ApiError::WrongMaxPrice`] |
//! | 400 | `BAD_STATUS` | [`ApiError::BadStatus`] |
//! | 402 | `NO_BALANCE` | [`ApiError::NoBalance`] |
//! | 403 | `BANNED`, `info:{scope,banned_until,retry_after_seconds,readable_date}` | [`ApiError::Banned`] |
//! | 403 | `CHANNELS_LIMIT`, `SERVICE_NOT_AVAILABLE`, `ACCOUNT_INACTIVE` | [`ApiError::Other`] (`"<title>: <details>"`) |
//! | 409 | `EARLY_CANCEL_DENIED`, `info:{minActivationTime}` | [`ApiError::EarlyCancelDenied`] |
//! | 409 | `NEW_OTP_RECEIVED`, `OTP_RECEIVED`, `FREE_CANCELLATION_EXPIRED`, `ACTIVATION_NOT_ACTIVE` | [`ApiError::Other`] |
//! | 500 | `SERVER_ERROR` | [`ApiError::Other`] |
//!
//! A few legacy answers still come as **plain text with HTTP 200**: `NO_NUMBERS`, `NO_KEY`,
//! `BAD_KEY`, `OPERATORS_NOT_FOUND`, `ERROR_SQL`, and — on `getNumber` (v1) only — the token form
//! `UNPROCESSABLE_ENTITY:<field>:<code>`. `getPrices` may also answer
//! `{"status":"false","msg":"service is incorrect"}` (→ [`ApiError::BadService`] /
//! [`ApiError::BadCountry`]). All of these are handled by [`classify`].
//!
//! # Rate limits and transport quirks
//!
//! * HTTP 429 `{"title":"RATE_LIMIT","details":""}` was observed after ~6 requests in quick
//!   succession; keep traffic at ≤ 1 request/second and back off ≥ 5 s on 429. The spec documents
//!   a `Retry-After` header on 429, which cannot be surfaced ([`HttpResponse`] carries no
//!   headers); [`ApiError::RateLimited::retry_after`] is only populated if the envelope ever
//!   carries `info.retry_after_seconds`, which has not been observed (the documented 429 body is
//!   `{title,details}` only).
//! * On a flaky network, large bodies (`getPrices`, `getTopCountriesByService` without a service)
//!   arrived truncated (exactly 18 323 bytes) with HTTP 200, which surfaces as
//!   [`ApiError::Parse`]. [`RetryTransport`] is an opt-in wrapper that detects unbalanced JSON
//!   bodies (bracket depth counted outside string literals), transport failures and 429s and
//!   retries with a backoff — but **only read-only actions** are re-sent after a transport
//!   error or a truncated body, so a `getNumberV2` that timed out client-side can never be
//!   bought twice; see [`HeroSms::with_api_key_retrying`].
//! * With `freePrice=true`, `price` in the top-countries rows is the cheapest tier
//!   (`0.2353` vs `0.4` without the flag) and `count` the total across all tiers.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use crate::api::{Client, Dialect};
use crate::error::{ApiError, ApiResult};
use crate::protocol::{self, as_object, value_to_f64, value_to_string, value_to_u64};
use crate::transport::{HttpResponse, Transport, TransportError};
use crate::types::*;

pub const ENDPOINT: &str = "https://hero-sms.com/stubs/handler_api.php";

/// Machine-readable API description behind the docs page.
pub const OPENAPI_URL: &str = "https://hero-sms.com/docs/v1/openapi.json";

/// What Hero-SMS implements (see the module docs for the evidence).
///
/// `price_bounds` means `maxPrice` only: Hero-SMS has no `minPrice`, so
/// [`NumberRequest::min_price`] is never sent (see [`HeroSmsDialect::adjust_params`]) and
/// [`HeroSms::get_number_with`] rejects it with [`ApiError::Unsupported`].
pub const CAPABILITIES: Capabilities = Capabilities {
    get_number_v2: true,
    numbers_status: true,
    active_activations: true,
    operators: true,
    prices_v2: false,
    prices_v3: false,
    price_bounds: true,
    provider_filters: false,
};

#[derive(Clone, Debug, Default)]
pub struct HeroSmsDialect;

impl Dialect for HeroSmsDialect {
    fn name(&self) -> &'static str {
        "Hero SMS"
    }

    fn endpoint(&self) -> &str {
        ENDPOINT
    }

    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn classify(&self, resp: &HttpResponse) -> ApiResult<()> {
        classify(resp)
    }

    /// Drops `minPrice` from `getNumber` / `getNumberV2`: it is not part of the Hero-SMS API and
    /// the server would silently ignore it, so it is never sent (see [`CAPABILITIES`]).
    fn adjust_params(&self, action: &str, params: &mut Vec<(String, String)>) {
        if matches!(action, "getNumber" | "getNumberV2") {
            params.retain(|(k, _)| k != "minPrice");
        }
    }

    /// `{"status":"success","data":[…],"activeActivations":{…,"rows":[…]}}` — the documented
    /// rows live in `data`; the shared parser would look at `activeActivations.rows` first, which
    /// was empty at capture time, so prefer `data` whenever it has rows.
    fn parse_active_activations(&self, body: &str) -> ApiResult<Vec<ActiveActivation>> {
        let v: Value = serde_json::from_str(body.trim())?;
        match v.get("data") {
            Some(data @ Value::Array(rows)) if !rows.is_empty() => {
                protocol::active_activations_from_value(data)
            }
            _ => protocol::active_activations_from_value(&v),
        }
    }
}

/// Hero-SMS response classification: JSON error envelopes on any status, then HTTP status, then
/// the legacy plain-text tokens and `{"status":"false","msg":…}` that still arrive with HTTP 200.
pub fn classify(resp: &HttpResponse) -> ApiResult<()> {
    let body = resp.body.trim();
    if let Some(err) = error_from_envelope(body) {
        return Err(err);
    }
    if resp.status == 429 {
        return Err(ApiError::RateLimited { retry_after: None });
    }
    if !(200..300).contains(&resp.status) {
        return Err(ApiError::Http {
            status: resp.status,
            body: body.to_owned(),
        });
    }
    if let Some(err) = error_from_token(body) {
        return Err(err);
    }
    if let Some(err) = error_from_status_msg(body) {
        return Err(err);
    }
    Ok(())
}

/// Bodies larger than this are only parsed for an error shape when their first bytes name the
/// key we are looking for. Every envelope observed is ≤ 151 bytes; the 1 MB `getPrices` and
/// 1.35 MB no-service `getTopCountriesByService` answers start with a country / service key and
/// are therefore left to the action parser alone.
const ERROR_SHAPE_SCAN_LIMIT: usize = 4096;

/// Whether `body` is worth deserialising to look for `key` at the top level: small bodies always,
/// large ones only when `key` appears within the first 64 bytes (JSON objects list it first).
fn may_contain_top_level_key(body: &str, key: &str) -> bool {
    if !body.starts_with('{') {
        return false;
    }
    if body.len() <= ERROR_SHAPE_SCAN_LIMIT {
        return true;
    }
    let end = (0..=64.min(body.len()))
        .rev()
        .find(|&i| body.is_char_boundary(i))
        .unwrap_or(0);
    body[..end].contains(key)
}

/// `{"title":"BAD_KEY","details":"Unauthorized","info":{…}}` → the matching [`ApiError`].
/// Returns `None` when the body is not an error envelope (data objects never carry `title`).
///
/// `BAD_ACTION` maps to [`ApiError::BadAction`] with an empty payload, like the plain-text token
/// (the envelope's `details` is the fixed text "Method Not Found", not the action name); the
/// provider-only methods on [`HeroSms`] fill in the action they sent.
pub fn error_from_envelope(body: &str) -> Option<ApiError> {
    if !may_contain_top_level_key(body, "\"title\"") {
        return None;
    }
    let v: Value = serde_json::from_str(body).ok()?;
    let title = v.get("title")?.as_str()?.trim();
    let details = v
        .get("details")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let info = v.get("info");
    let field = |k: &str| info.and_then(|i| i.get(k));
    Some(match title {
        "BAD_KEY" | "NO_KEY" => ApiError::BadKey,
        "BAD_ACTION" => ApiError::BadAction(String::new()),
        "NOT_FOUND" => ApiError::NoActivation,
        "RATE_LIMIT" => ApiError::RateLimited {
            retry_after: value_to_u64(field("retry_after_seconds")),
        },
        "UNPROCESSABLE_ENTITY" => ApiError::Validation {
            field: value_to_string(field("field")).unwrap_or_default(),
            message: value_to_string(field("message"))
                .filter(|m| !m.is_empty())
                .or_else(|| value_to_string(field("code")))
                .unwrap_or_else(|| details.to_owned()),
        },
        "WRONG_MAX_PRICE" => ApiError::WrongMaxPrice {
            min: value_to_f64(field("min")),
        },
        "BANNED" => ApiError::Banned {
            until: value_to_string(field("readable_date"))
                .or_else(|| value_to_string(field("banned_until")))
                .unwrap_or_else(|| details.to_owned()),
        },
        "EARLY_CANCEL_DENIED" => ApiError::EarlyCancelDenied,
        other => match ApiError::from_code(other)? {
            ApiError::Other(code) if !details.is_empty() => {
                ApiError::Other(format!("{code}: {details}"))
            }
            mapped => mapped,
        },
    })
}

/// Plain-text error tokens that Hero-SMS still returns with HTTP 200, including the v1
/// `UNPROCESSABLE_ENTITY:<field>:<code>` form observed on `getNumber`.
pub fn error_from_token(body: &str) -> Option<ApiError> {
    if body.starts_with('{') || body.starts_with('[') {
        return None;
    }
    let mut parts = body.splitn(3, ':');
    match parts.next()?.trim() {
        "UNPROCESSABLE_ENTITY" => Some(ApiError::Validation {
            field: parts.next().unwrap_or("").trim().to_owned(),
            message: parts.next().unwrap_or("").trim().to_owned(),
        }),
        "NO_KEY" => Some(ApiError::BadKey),
        _ => protocol::error_from_body(body),
    }
}

/// `getPrices` documents `{"status":"false","msg":"service is incorrect"}` for bad filters.
pub fn error_from_status_msg(body: &str) -> Option<ApiError> {
    if !may_contain_top_level_key(body, "\"status\"") {
        return None;
    }
    let v: Value = serde_json::from_str(body).ok()?;
    let status = v.get("status")?;
    let failed = matches!(status, Value::Bool(false))
        || matches!(status.as_str(), Some("false") | Some("error"));
    if !failed {
        return None;
    }
    let msg = value_to_string(v.get("msg").or_else(|| v.get("message"))).unwrap_or_default();
    let lower = msg.to_ascii_lowercase();
    Some(if lower.contains("service") {
        ApiError::BadService
    } else if lower.contains("country") {
        ApiError::BadCountry
    } else {
        ApiError::Other(msg)
    })
}

#[cfg(feature = "ureq")]
pub type HeroSms<T = crate::transport::UreqTransport> = Client<T, HeroSmsDialect>;
#[cfg(not(feature = "ureq"))]
pub type HeroSms<T> = Client<T, HeroSmsDialect>;

#[cfg(feature = "ureq")]
impl HeroSms {
    /// Client over the default `ureq` transport.
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Client::new(
            crate::transport::UreqTransport::new(),
            HeroSmsDialect,
            api_key,
        )
    }
}

#[cfg(feature = "ureq")]
impl HeroSms<RetryTransport<crate::transport::UreqTransport>> {
    /// Client over `ureq` wrapped in [`RetryTransport`] with its defaults (3 attempts, 1 s
    /// backoff for transport errors / truncated bodies, 5 s for HTTP 429).
    ///
    /// Safe for purchases: after a transport error or a truncated body only the read-only
    /// actions listed in [`RetryTransport::RETRYABLE_ACTIONS`] are re-sent; `getNumber*`,
    /// `setStatus`, `finishActivation`, `cancelActivation` … fail on the first such error so a
    /// request the server may already have fulfilled is never repeated. HTTP 429 (request
    /// rejected, nothing processed) is retried for every action.
    pub fn with_api_key_retrying(api_key: impl Into<String>) -> Self {
        Client::new(
            RetryTransport::new(crate::transport::UreqTransport::new()),
            HeroSmsDialect,
            api_key,
        )
    }
}

// ---------------------------------------------------------------------------
// Provider-only types

/// One `price → count` tier of a `freePriceMap` / `physicalPriceMap`.
#[derive(Clone, Debug, PartialEq)]
pub struct PriceTier {
    pub price: f64,
    /// Numbers obtainable when bidding `price` (cumulative: higher price, more numbers).
    pub count: u64,
}

/// Row of `getTopCountriesByService&freePrice=true`.
#[derive(Clone, Debug, PartialEq)]
pub struct HeroTopCountry {
    pub country: CountryId,
    /// Cheapest tier when `freePrice=true`, the default price otherwise.
    pub price: f64,
    pub retail_price: Option<f64>,
    /// Total numbers across all tiers.
    pub count: u64,
    /// Tiers sorted by ascending price. Empty when the provider sent no map.
    pub free_price_map: Vec<PriceTier>,
    pub physical_total_count: Option<u64>,
    pub physical_count_for_default_price: Option<u64>,
    /// Documented but not seen live; empty unless present.
    pub physical_price_map: Vec<PriceTier>,
}

impl HeroTopCountry {
    /// Cheapest tier that still offers at least `min_count` numbers.
    pub fn cheapest_tier_with(&self, min_count: u64) -> Option<&PriceTier> {
        self.free_price_map.iter().find(|t| t.count >= min_count)
    }
}

/// One OTP as reported by `getStatusV2` / `getAllSms`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Otp {
    pub id: String,
    /// Sender (`"Telegram"` or a phone number).
    pub phone_from: Option<String>,
    pub code: Option<String>,
    pub text: Option<String>,
    pub service: Option<ServiceCode>,
    /// RFC 3339 timestamp (`2026-02-16T12:36:59+03:00`).
    pub date: Option<String>,
    /// `"sms"` or `"call"`.
    pub kind: Option<String>,
}

/// Result of `getStatusV2`.
#[derive(Clone, Debug, PartialEq)]
pub enum StatusV2 {
    /// The provider answered with a classic token (`STATUS_CANCEL` is documented; the others are
    /// accepted defensively).
    Plain(ActivationStatus),
    /// JSON answer. `otp` is `None` when `data` is absent/null (shape before the first OTP is not
    /// documented — an activation without an OTP was not available at capture time).
    Otp {
        verification_type: Option<String>,
        otp: Option<Otp>,
    },
}

impl StatusV2 {
    /// The received code, if any.
    pub fn code(&self) -> Option<&str> {
        match self {
            StatusV2::Plain(s) => s.code(),
            StatusV2::Otp { otp, .. } => otp.as_ref().and_then(|o| o.code.as_deref()),
        }
    }
}

/// Metadata (`meta`) of `getAllSms`, as documented: `{"total":42,"service":"full"}`.
/// (A live page could not be observed — no activation existed at capture time.)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageMeta {
    /// Total number of OTPs on the activation (across pages).
    pub total: Option<u64>,
    /// Service code of the activation.
    pub service: Option<ServiceCode>,
}

/// Result of `getAllSms`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OtpPage {
    pub data: Vec<Otp>,
    pub meta: PageMeta,
}

/// Row of `getHistory`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryEntry {
    pub id: ActivationId,
    /// `YYYY-MM-DD HH:MM:SS`
    pub date: Option<String>,
    pub phone: Option<String>,
    /// Last SMS text, `None` when nothing arrived.
    pub sms: Option<String>,
    pub cost: Option<f64>,
    /// Raw provider status (`"6"` finished, `"8"` cancelled …).
    pub status: Option<String>,
    /// ISO 4217 numeric code (840 = USD).
    pub currency: Option<u32>,
}

/// Filters for `getHistory`. Timestamps are Unix seconds; `size` is capped at 100 by the provider.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryQuery {
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub offset: Option<u64>,
    pub size: Option<u64>,
}

impl HistoryQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn start(mut self, unix: u64) -> Self {
        self.start = Some(unix);
        self
    }
    pub fn end(mut self, unix: u64) -> Self {
        self.end = Some(unix);
        self
    }
    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }
    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }
}

/// Hero-SMS-only `getNumber` / `getNumberV2` parameters (all documented, none verified live —
/// they are purchase parameters). Price bounds: only [`NumberRequest::max_price`] (`maxPrice`)
/// exists on Hero-SMS; `min_price` is rejected by [`HeroSms::get_number_with`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeroNumberOptions {
    /// `fixedPrice=true`: buy strictly at `maxPrice`. Documented as "use with the maxPrice
    /// parameter": [`HeroNumberOptions::apply`] only emits it when `max_price` is set and
    /// [`HeroSms::get_number_with`] rejects the combination without one.
    pub fixed_price: bool,
    /// `ref`: referral identifier.
    pub referral: Option<String>,
    /// `phoneException`: number prefixes you do not want (max 20), sent comma-separated.
    pub phone_exception: Vec<String>,
}

impl HeroNumberOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn fixed_price(mut self, fixed: bool) -> Self {
        self.fixed_price = fixed;
        self
    }
    pub fn referral(mut self, referral: impl Into<String>) -> Self {
        self.referral = Some(referral.into());
        self
    }
    pub fn exclude_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.phone_exception.push(prefix.into());
        self
    }

    /// Applies the options to a [`NumberRequest`] as extra query parameters. `fixedPrice` is
    /// only emitted alongside a `max_price` (the server's behaviour for a dangling flag is
    /// unknown); use [`HeroSms::get_number_with`] to have that combination rejected instead.
    pub fn apply(&self, request: &NumberRequest) -> NumberRequest {
        let mut req = request.clone();
        if self.fixed_price && request.max_price.is_some() {
            req.extra.push(("fixedPrice".to_owned(), "true".to_owned()));
        }
        if let Some(r) = &self.referral {
            req.extra.push(("ref".to_owned(), r.clone()));
        }
        if !self.phone_exception.is_empty() {
            req.extra
                .push(("phoneException".to_owned(), self.phone_exception.join(",")));
        }
        req
    }
}

// ---------------------------------------------------------------------------
// Provider-only actions

impl<T: Transport> HeroSms<T> {
    /// [`Client::call`] for a provider-only action: a `BAD_ACTION` answer (whose envelope carries
    /// no action name) is reported as `BadAction("<action>")`.
    fn call_named(&self, action: &'static str, params: Vec<(String, String)>) -> ApiResult<String> {
        self.call(action, params).map_err(|e| match e {
            ApiError::BadAction(name) if name.is_empty() => ApiError::BadAction(action.to_owned()),
            other => other,
        })
    }

    /// `getNumber` / `getNumberV2` with the Hero-SMS-only parameters of [`HeroNumberOptions`].
    ///
    /// Fails **before any request** with [`ApiError::Unsupported`]`("minPrice")` when
    /// `request.min_price` is set (Hero-SMS cannot enforce a price floor) and with
    /// [`ApiError::Validation`] on `maxPrice` when `options.fixed_price` is set without a
    /// `max_price` (the flag is documented only together with `maxPrice`).
    pub fn get_number_with(
        &self,
        request: &NumberRequest,
        options: &HeroNumberOptions,
    ) -> ApiResult<Activation> {
        if request.min_price.is_some() {
            return Err(ApiError::Unsupported("minPrice"));
        }
        if options.fixed_price && request.max_price.is_none() {
            return Err(ApiError::Validation {
                field: "maxPrice".to_owned(),
                message: "fixedPrice requires max_price".to_owned(),
            });
        }
        crate::SmsActivateApi::get_number(self, &options.apply(request))
    }

    /// `getTopCountriesByService&service=…&freePrice=true` — per-country price tiers.
    pub fn get_top_countries_free_price(
        &self,
        service: &ServiceCode,
    ) -> ApiResult<Vec<HeroTopCountry>> {
        let body = self.call_named(
            "getTopCountriesByService",
            vec![
                ("service".to_owned(), service.0.clone()),
                ("freePrice".to_owned(), "true".to_owned()),
            ],
        )?;
        parse_top_countries_free_price(&body)
    }

    /// `getTopCountriesByService` without a service: every service's top countries.
    /// **Heavy** (≈ 1.35 MB, 725 services at capture time); prefer the per-service call.
    pub fn get_top_countries_all(&self) -> ApiResult<BTreeMap<ServiceCode, Vec<TopCountry>>> {
        let body = self.call_named("getTopCountriesByService", Vec::new())?;
        parse_top_countries_all(&body)
    }

    /// `getOperators&country=…` — operator names for one country (empty when none listed).
    ///
    /// Hero-SMS keys countries numerically, so this takes the numeric id and wraps it in
    /// [`CountryRef::Id`] for [`SmsActivateApi::get_operators`](crate::SmsActivateApi::get_operators).
    pub fn get_operators_in(&self, country: CountryId) -> ApiResult<Vec<String>> {
        let key = CountryRef::Id(country);
        let mut map = crate::SmsActivateApi::get_operators(self, Some(&key))?;
        Ok(map.remove(&key).unwrap_or_default())
    }

    /// `getOperators` — operators for every country that has any, keyed by numeric id.
    ///
    /// Hero-SMS only ever answers with numeric country keys; a non-numeric key would be dropped.
    /// Use [`SmsActivateApi::get_operators`](crate::SmsActivateApi::get_operators) for the
    /// provider-neutral [`CountryRef`] map.
    pub fn get_all_operators(&self) -> ApiResult<BTreeMap<CountryId, Vec<String>>> {
        Ok(crate::SmsActivateApi::get_operators(self, None)?
            .into_iter()
            .filter_map(|(country, ops)| Some((country.id()?, ops)))
            .collect())
    }

    /// `getServicesList[&country][&lang]` — services sold in `country` (all when `None`), names
    /// in `lang` (`en` cn es de fr pt ru id vi tr).
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
        let body = self.call_named("getServicesList", params)?;
        self.dialect().parse_services(&body)
    }

    /// `getActiveActivations&start=…&limit=…` (limit ≤ 100). No live activation existed when the
    /// shape was captured; rows are parsed from `data` per the documentation.
    pub fn get_active_activations_page(
        &self,
        start: u64,
        limit: u64,
    ) -> ApiResult<Vec<ActiveActivation>> {
        let body = self.call_named(
            "getActiveActivations",
            vec![
                ("start".to_owned(), start.to_string()),
                ("limit".to_owned(), limit.to_string()),
            ],
        )?;
        self.dialect().parse_active_activations(&body)
    }

    /// `getStatusV2` — structured status with the full OTP (code, text, sender, timestamp).
    pub fn get_status_v2(&self, id: &ActivationId) -> ApiResult<StatusV2> {
        let body = self.call_named("getStatusV2", vec![("id".to_owned(), id.0.clone())])?;
        parse_status_v2(&body)
    }

    /// `getAllSms&id=…[&size][&page]` — every OTP received on the activation.
    pub fn get_all_sms(
        &self,
        id: &ActivationId,
        size: Option<u64>,
        page: Option<u64>,
    ) -> ApiResult<OtpPage> {
        let mut params = vec![("id".to_owned(), id.0.clone())];
        if let Some(s) = size {
            params.push(("size".to_owned(), s.to_string()));
        }
        if let Some(p) = page {
            params.push(("page".to_owned(), p.to_string()));
        }
        let body = self.call_named("getAllSms", params)?;
        parse_otp_page(&body)
    }

    /// `getHistory` — past activations (newest first as delivered by the provider).
    pub fn get_history(&self, query: &HistoryQuery) -> ApiResult<Vec<HistoryEntry>> {
        let mut params = Vec::new();
        for (k, v) in [
            ("start", query.start),
            ("end", query.end),
            ("offset", query.offset),
            ("size", query.size),
        ] {
            if let Some(v) = v {
                params.push((k.to_owned(), v.to_string()));
            }
        }
        let body = self.call_named("getHistory", params)?;
        parse_history(&body)
    }

    /// `finishActivation` — HTTP 204 on success (equivalent to `setStatus` 6).
    pub fn finish_activation(&self, id: &ActivationId) -> ApiResult<()> {
        self.call_named("finishActivation", vec![("id".to_owned(), id.0.clone())])?;
        Ok(())
    }

    /// `cancelActivation` — HTTP 204 on success (equivalent to `setStatus` 8).
    pub fn cancel_activation(&self, id: &ActivationId) -> ApiResult<()> {
        self.call_named("cancelActivation", vec![("id".to_owned(), id.0.clone())])?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Parsers for the provider-only shapes

fn parse_price_map(v: Option<&Value>) -> Vec<PriceTier> {
    let mut tiers: Vec<PriceTier> = v
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(price, count)| {
                    Some(PriceTier {
                        price: price.trim().parse().ok()?,
                        count: value_to_u64(Some(count))?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    tiers.sort_by(|a, b| a.price.total_cmp(&b.price));
    tiers
}

/// `{"0":{"country":73,"price":0.2353,"retail_price":0.48,"count":1213663,"freePriceMap":{"2.9471":1213663,…}},…}`
pub fn parse_top_countries_free_price(body: &str) -> ApiResult<Vec<HeroTopCountry>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let entries: Vec<(u64, &Value)> = match &v {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, x)| (i as u64, x))
            .collect(),
        Value::Object(map) => map
            .iter()
            .map(|(k, x)| (k.parse().unwrap_or(u64::MAX), x))
            .collect(),
        _ => return Err(ApiError::Unexpected(body.trim().to_owned())),
    };
    let mut rows: Vec<(u64, HeroTopCountry)> = entries
        .into_iter()
        .map(|(idx, row)| {
            let country = value_to_u64(row.get("country"))
                .ok_or_else(|| ApiError::Parse("missing country".into()))?
                as CountryId;
            Ok((
                idx,
                HeroTopCountry {
                    country,
                    price: value_to_f64(row.get("price")).unwrap_or(0.0),
                    retail_price: value_to_f64(row.get("retail_price")),
                    count: value_to_u64(row.get("count")).unwrap_or(0),
                    free_price_map: parse_price_map(row.get("freePriceMap")),
                    physical_total_count: value_to_u64(row.get("physicalTotalCount")),
                    physical_count_for_default_price: value_to_u64(
                        row.get("physicalCountForDefaultPrice"),
                    ),
                    physical_price_map: parse_price_map(row.get("physicalPriceMap")),
                },
            ))
        })
        .collect::<ApiResult<_>>()?;
    rows.sort_by_key(|(i, _)| *i);
    Ok(rows.into_iter().map(|(_, r)| r).collect())
}

/// `{"tg":{"0":{country,price,retail_price,count},…},"wa":{…}}` (live) — each value is the
/// per-service indexed shape handled by [`protocol::parse_top_countries_indexed`]. The docs wrap
/// the map in a one-element array (`[{"ig":[…]}]`); that is unwrapped first.
pub fn parse_top_countries_all(body: &str) -> ApiResult<BTreeMap<ServiceCode, Vec<TopCountry>>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let map = match &v {
        Value::Array(items) if items.len() == 1 => &items[0],
        other => other,
    };
    let mut out = BTreeMap::new();
    for (service, rows) in as_object(map, "top countries")? {
        out.insert(
            ServiceCode(service.clone()),
            protocol::top_countries_indexed_from_value(rows)?,
        );
    }
    Ok(out)
}

fn parse_otp(v: &Value) -> ApiResult<Otp> {
    let id =
        value_to_string(v.get("id")).ok_or_else(|| ApiError::Parse("otp without id".into()))?;
    Ok(Otp {
        id,
        phone_from: value_to_string(v.get("phoneFrom")),
        code: value_to_string(v.get("code")),
        text: value_to_string(v.get("text")),
        service: value_to_string(v.get("service")).map(ServiceCode),
        date: value_to_string(v.get("date")),
        kind: value_to_string(v.get("type")),
    })
}

/// `{"verificationType":"sms","data":{"id":…,"phoneFrom":…,"code":…,"text":…,"service":…,"date":…,"type":…}}`
/// or a classic `STATUS_*` token.
pub fn parse_status_v2(body: &str) -> ApiResult<StatusV2> {
    let trimmed = body.trim();
    if !trimmed.starts_with('{') {
        return protocol::parse_status(trimmed).map(StatusV2::Plain);
    }
    let v: Value = serde_json::from_str(trimmed)?;
    let otp = match v.get("data") {
        Some(Value::Object(_)) => Some(parse_otp(&v["data"])?),
        _ => None,
    };
    Ok(StatusV2::Otp {
        verification_type: value_to_string(v.get("verificationType")),
        otp,
    })
}

/// `{"data":[otp…],"meta":{"total":42,"service":"full"}}` (documented shape; `meta` may be absent).
pub fn parse_otp_page(body: &str) -> ApiResult<OtpPage> {
    let v: Value = serde_json::from_str(body.trim())?;
    let data = match v.get("data").or(Some(&v)) {
        Some(Value::Array(items)) => items.iter().map(parse_otp).collect::<ApiResult<_>>()?,
        _ => Vec::new(),
    };
    let meta = v.get("meta");
    let m = |k: &str| meta.and_then(|m| m.get(k));
    Ok(OtpPage {
        data,
        meta: PageMeta {
            total: value_to_u64(m("total")),
            service: value_to_string(m("service")).map(ServiceCode),
        },
    })
}

/// `[{"id":"…","date":"2026-08-29 12:00:36","phone":"…","sms":"…"|null,"cost":0.1275,"status":"6","currency":840}]`
pub fn parse_history(body: &str) -> ApiResult<Vec<HistoryEntry>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let rows = match &v {
        Value::Array(a) => a.iter().collect::<Vec<_>>(),
        Value::Object(_) => v
            .get("data")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => return Err(ApiError::Unexpected(body.trim().to_owned())),
    };
    rows.into_iter()
        .map(|r| {
            let id = value_to_string(r.get("id"))
                .ok_or_else(|| ApiError::Parse("history row without id".into()))?;
            Ok(HistoryEntry {
                id: ActivationId(id),
                date: value_to_string(r.get("date")),
                phone: value_to_string(r.get("phone")),
                sms: value_to_string(r.get("sms")).filter(|s| !s.is_empty()),
                cost: value_to_f64(r.get("cost")),
                status: value_to_string(r.get("status")),
                currency: value_to_u64(r.get("currency")).map(|c| c as u32),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Opt-in retrying transport

/// Wraps any [`Transport`] and retries on transport errors, on JSON bodies that arrived truncated
/// (unbalanced brackets — see [`looks_truncated`]) and on HTTP 429. Retries are blocking sleeps,
/// so use it only where that is acceptable.
///
/// **Idempotency rule.** A transport error (timeout, connection reset) or a truncated body does
/// not tell us whether the server processed the request, so those are retried only for the
/// read-only actions in [`RetryTransport::RETRYABLE_ACTIONS`]; any other action (`getNumber`,
/// `getNumberV2`, `getRentNumber`, `setStatus`, `finishActivation`, `cancelActivation`, unknown
/// ones) gets exactly one attempt and its first error is returned unchanged, so a purchase can
/// never be issued twice. HTTP 429 means the request was rejected before processing and is
/// retried for every action.
#[derive(Clone, Debug)]
pub struct RetryTransport<T> {
    inner: T,
    attempts: u32,
    retry_delay: Duration,
    rate_limit_delay: Duration,
}

impl<T> RetryTransport<T> {
    pub const DEFAULT_ATTEMPTS: u32 = 3;
    pub const DEFAULT_RETRY_DELAY: Duration = Duration::from_secs(1);
    pub const DEFAULT_RATE_LIMIT_DELAY: Duration = Duration::from_secs(5);

    /// Read-only actions that are safe to re-send after a transport error or a truncated body.
    pub const RETRYABLE_ACTIONS: [&'static str; 15] = [
        "getBalance",
        "getPrices",
        "getServicesList",
        "getCountries",
        "getTopCountriesByService",
        "getTopCountriesByServiceRank",
        "getNumbersStatus",
        "getOperators",
        "getActiveActivations",
        "getHistory",
        "getStatus",
        "getStatusV2",
        "getAllSms",
        "getRentServicesAndCountries",
        "serviceCountRent",
    ];

    /// Whether the `action=` of `url` may be re-sent when the outcome of a request is unknown.
    pub fn is_retryable_url(url: &str) -> bool {
        action_of(url).is_some_and(|a| Self::RETRYABLE_ACTIONS.contains(&a))
    }

    pub fn new(inner: T) -> Self {
        Self {
            inner,
            attempts: Self::DEFAULT_ATTEMPTS,
            retry_delay: Self::DEFAULT_RETRY_DELAY,
            rate_limit_delay: Self::DEFAULT_RATE_LIMIT_DELAY,
        }
    }

    /// Total attempts (≥ 1).
    pub fn attempts(mut self, attempts: u32) -> Self {
        self.attempts = attempts.max(1);
        self
    }

    /// Delay before retrying a transport error or truncated body (doubles each attempt).
    pub fn retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// Delay before retrying after HTTP 429 (doubles each attempt).
    pub fn rate_limit_delay(mut self, delay: Duration) -> Self {
        self.rate_limit_delay = delay;
        self
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

/// The `action` query value of a handler URL (`…?api_key=…&action=getBalance&…`).
fn action_of(url: &str) -> Option<&str> {
    let query = url.split_once('?')?.1;
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("action="))
        .map(|v| v.split('#').next().unwrap_or(v))
}

/// A JSON body that was cut short in transit: it opens with `{` / `[` but the brackets do not
/// balance (counted outside string literals) or it ends inside a string literal. Anything that
/// does not start as JSON (plain-text tokens, empty 204 bodies) is never "truncated".
pub fn looks_truncated(body: &str) -> bool {
    let b = body.trim();
    if !(b.starts_with('{') || b.starts_with('[')) {
        return false;
    }
    let mut depth: i64 = 0;
    let mut in_string = false;
    let mut escaped = false;
    for c in b.bytes() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
    }
    in_string || depth != 0
}

impl<T: Transport> Transport for RetryTransport<T> {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        let unknown_outcome_ok = Self::is_retryable_url(url);
        let mut retry_delay = self.retry_delay;
        let mut rate_delay = self.rate_limit_delay;
        let mut last: Option<Result<HttpResponse, TransportError>> = None;
        for attempt in 1..=self.attempts {
            let delay = match self.inner.get(url) {
                Ok(resp) if resp.status == 429 => {
                    last = Some(Ok(resp));
                    let d = rate_delay;
                    rate_delay *= 2;
                    d
                }
                // The request may have been processed: only re-send read-only actions.
                Ok(resp) if resp.status < 300 && looks_truncated(&resp.body) => {
                    if !unknown_outcome_ok {
                        return Ok(resp);
                    }
                    last = Some(Err(TransportError(format!(
                        "response body truncated ({} bytes) after {attempt} attempt(s)",
                        resp.body.len()
                    ))));
                    let d = retry_delay;
                    retry_delay *= 2;
                    d
                }
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    if !unknown_outcome_ok {
                        return Err(e);
                    }
                    last = Some(Err(e));
                    let d = retry_delay;
                    retry_delay *= 2;
                    d
                }
            };
            if attempt < self.attempts && !delay.is_zero() {
                std::thread::sleep(delay);
            }
        }
        last.unwrap_or_else(|| Err(TransportError("no attempts made".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SmsActivateApi;
    use crate::transport::FakeTransport;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../fixtures/hero_sms/", $name))
        };
    }

    fn client(t: FakeTransport) -> HeroSms<FakeTransport> {
        Client::new(t, HeroSmsDialect, "KEY")
    }

    fn only_request(c: &HeroSms<FakeTransport>) -> String {
        let reqs = c.transport().requests();
        assert_eq!(reqs.len(), 1);
        reqs[0].clone()
    }

    #[test]
    fn identity_and_capabilities() {
        let c = client(FakeTransport::new());
        assert_eq!(c.provider(), "Hero SMS");
        assert_eq!(c.dialect().endpoint(), ENDPOINT);
        let caps = c.capabilities();
        assert!(caps.get_number_v2);
        assert!(caps.numbers_status);
        assert!(caps.active_activations);
        assert!(caps.operators);
        assert!(caps.price_bounds);
        assert!(!caps.prices_v2);
        assert!(!caps.prices_v3);
        assert!(!caps.provider_filters);
    }

    #[test]
    fn balance() {
        let c = client(FakeTransport::new().push(200, fixture!("getBalance.txt")));
        assert_eq!(c.get_balance().unwrap(), 12.5085);
        assert_eq!(
            only_request(&c),
            format!("{ENDPOINT}?api_key=KEY&action=getBalance")
        );
    }

    #[test]
    fn prices_for_service_and_empty_table() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getPrices_service_tg.txt"))
                .push(200, fixture!("getPrices_service_tg_country_0.txt")),
        );
        let tg = ServiceCode::from("tg");
        let table = c.get_prices(Some(&tg), None).unwrap();
        assert!(table.len() > 50);
        assert_eq!(table[&CountryRef::Id(14)][&tg].physical_count, Some(721));
        assert_eq!(table[&CountryRef::Id(62)][&tg].cost, 1.0);
        assert!(
            c.get_prices(Some(&tg), Some(&CountryRef::Id(0)))
                .unwrap()
                .is_empty()
        );
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getPrices&service=tg"));
        assert!(reqs[1].ends_with("action=getPrices&service=tg&country=0"));
    }

    #[test]
    fn prices_bad_filter_json_is_an_error() {
        let c = client(
            FakeTransport::new()
                .push(200, r#"{"status":"false","msg":"service is incorrect"}"#)
                .push(200, r#"{"status":"false","msg":"country is incorrect"}"#),
        );
        assert!(matches!(
            c.get_prices(None, None),
            Err(ApiError::BadService)
        ));
        assert!(matches!(
            c.get_prices(None, None),
            Err(ApiError::BadCountry)
        ));
    }

    #[test]
    fn services_plain_and_filtered() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getServicesList.txt"))
                .push(200, fixture!("getServicesList_country_187_lang_ru.txt")),
        );
        let all = c.get_services().unwrap();
        assert!(all.len() > 500);
        assert!(
            all.iter()
                .any(|s| s.code.as_str() == "tg" && s.name == "Telegram")
        );
        let usa = c.get_services_for(Some(187), Some("ru")).unwrap();
        assert_eq!(usa.len(), 262);
        assert!(usa.iter().any(|s| s.code.as_str() == "aba"));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getServicesList"));
        assert!(reqs[1].ends_with("action=getServicesList&country=187&lang=ru"));
    }

    #[test]
    fn countries_object_and_documented_array_shape() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getCountries.txt"))
                .push(
                    200,
                    r#"[{"id":2,"rus":"Казахстан","eng":"Kazakhstan","chn":"哈萨克斯坦","visible":1,"retry":1},{"eng":"junk row without id"}]"#,
                ),
        );
        let countries = c.get_countries().unwrap();
        assert!(countries.len() > 100);
        let ua = countries.iter().find(|c| c.id() == Some(1)).unwrap();
        assert_eq!(ua.name_en, "Ukraine");
        assert_eq!(ua.name_ru.as_deref(), Some("Украина"));
        assert_eq!(ua.visible, Some(true));
        assert_eq!(ua.rent, Some(true));
        let china = countries.iter().find(|c| c.id() == Some(3)).unwrap();
        assert_eq!(china.rent, Some(false));
        assert!(countries.windows(2).all(|w| w[0].key < w[1].key));

        // the documented array shape goes through the shared parser: id-less rows are skipped,
        // not given their array index as id
        let documented = c.get_countries().unwrap();
        assert_eq!(documented.len(), 1);
        assert_eq!(documented[0].id(), Some(2));
        assert_eq!(documented[0].name_en, "Kazakhstan");
        assert_eq!(documented[0].rent, None);
    }

    #[test]
    fn top_countries_plain() {
        let c = client(
            FakeTransport::new().push(200, fixture!("getTopCountriesByService_service_tg.txt")),
        );
        let rows = c.get_top_countries(&ServiceCode::from("tg")).unwrap();
        assert!(rows.len() > 5);
        assert_eq!(rows[0].country, CountryRef::Id(73));
        assert_eq!(rows[0].price, 0.4);
        assert_eq!(rows[0].retail_price, Some(0.48));
        assert_eq!(rows[0].count, 1081284);
        assert_eq!(rows[3].country, CountryRef::Id(187));
        assert!(only_request(&c).ends_with("action=getTopCountriesByService&service=tg"));
    }

    #[test]
    fn top_countries_free_price_tiers() {
        let c = client(FakeTransport::new().push(
            200,
            fixture!("getTopCountriesByService_service_tg_freePrice_true.txt"),
        ));
        let rows = c
            .get_top_countries_free_price(&ServiceCode::from("tg"))
            .unwrap();
        assert!(
            only_request(&c).ends_with("action=getTopCountriesByService&service=tg&freePrice=true")
        );
        let br = &rows[0];
        assert_eq!(br.country, 73);
        assert_eq!(br.price, 0.2353);
        assert_eq!(br.retail_price, Some(0.48));
        assert_eq!(br.count, 1213663);
        assert!(br.free_price_map.len() > 30);
        // ascending by price; cheapest tier first, most expensive tier offers everything
        assert_eq!(br.free_price_map[0].price, 0.2353);
        assert_eq!(br.free_price_map[0].count, 4);
        let last = br.free_price_map.last().unwrap();
        assert_eq!(last.price, 2.9471);
        assert_eq!(last.count, 1213663);
        assert!(
            br.free_price_map
                .windows(2)
                .all(|w| w[0].price < w[1].price && w[0].count <= w[1].count)
        );
        assert_eq!(br.cheapest_tier_with(1_000_000).unwrap().price, 0.3958);
        assert!(br.physical_price_map.is_empty());
        assert_eq!(br.physical_total_count, None);
        assert!(rows.iter().all(|r| !r.free_price_map.is_empty()));
    }

    #[test]
    fn top_countries_free_price_documented_physical_fields() {
        let rows = parse_top_countries_free_price(
            r#"[{"physicalTotalCount":7933,"physicalCountForDefaultPrice":5198,"physicalPriceMap":{"0.04":24,"0.0415":439,"0.0419":3229},"retail_price":0.2,"country":6,"price":0.045,"count":5477}]"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].physical_total_count, Some(7933));
        assert_eq!(rows[0].physical_count_for_default_price, Some(5198));
        assert_eq!(rows[0].physical_price_map.len(), 3);
        assert_eq!(rows[0].physical_price_map[0].price, 0.04);
        assert!(rows[0].free_price_map.is_empty());
    }

    #[test]
    fn top_countries_all_services() {
        let c = client(
            FakeTransport::new().push(200, fixture!("getTopCountriesByService_excerpt.txt")),
        );
        let all = c.get_top_countries_all().unwrap();
        assert!(only_request(&c).ends_with("action=getTopCountriesByService"));
        assert_eq!(all.len(), 2);
        assert_eq!(all[&ServiceCode::from("bdp")].len(), 1);
        let aow = &all[&ServiceCode::from("aow")];
        assert_eq!(aow.len(), 13);
        assert_eq!(aow[0].country, CountryRef::Id(52));
        assert_eq!(aow[0].price, 0.0625);
        assert_eq!(aow[1].country, CountryRef::Id(6));

        // the documented shape wraps the map in a one-element array
        let documented = parse_top_countries_all(
            r#"[{"ig":[{"physicalTotalCount":7933,"physicalCountForDefaultPrice":5198,"physicalPriceMap":{"0.04":24},"retail_price":0.2,"country":6,"price":0.045,"count":5477}]}]"#,
        )
        .unwrap();
        assert_eq!(documented.len(), 1);
        let ig = &documented[&ServiceCode::from("ig")];
        assert_eq!(ig.len(), 1);
        assert_eq!(ig[0].country, CountryRef::Id(6));
        assert_eq!(ig[0].count, 5477);
        assert!(matches!(
            parse_top_countries_all("[]"),
            Err(ApiError::Parse(_))
        ));
    }

    #[test]
    fn numbers_status_variants() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getNumbersStatus_country_73.txt"))
                .push(200, fixture!("getNumbersStatus_country_187.txt"))
                .push(200, fixture!("getNumbersStatus_country_0.txt"))
                .push(422, fixture!("getNumbersStatus.txt")),
        );
        let br = c.get_numbers_status(&CountryRef::Id(73), None).unwrap();
        assert_eq!(br[&ServiceCode::from("bqp")], 284063);
        let us = c.get_numbers_status(&CountryRef::Id(187), None).unwrap();
        assert_eq!(us[&ServiceCode::from("bqp")], 586902);
        assert!(
            c.get_numbers_status(&CountryRef::Id(0), None)
                .unwrap()
                .is_empty()
        );
        match c.get_numbers_status(&CountryRef::Id(1), None) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "country");
                assert_eq!(message, "Param 'country' must be a number");
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(c.transport().requests()[0].ends_with("action=getNumbersStatus&country=73"));
    }

    #[test]
    fn operators_one_and_all() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getOperators_country_73.txt"))
                .push(200, fixture!("getOperators.txt"))
                .push(200, fixture!("getOperators_country_73.txt"))
                .push(200, "OPERATORS_NOT_FOUND"),
        );
        let br = c.get_operators_in(73).unwrap();
        assert_eq!(br.len(), 12);
        assert!(br.iter().any(|o| o == "vivo"));
        let all = c.get_all_operators().unwrap();
        assert_eq!(all.len(), 137);
        assert!(all[&56].iter().any(|o| o == "movistar"));
        // asking for a country that is not in the answer yields an empty list, not an error
        assert!(c.get_operators_in(1).unwrap().is_empty());
        assert!(matches!(
            c.get_operators_in(1),
            Err(ApiError::Other(t)) if t == "OPERATORS_NOT_FOUND"
        ));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getOperators&country=73"));
        assert!(reqs[1].ends_with("action=getOperators"));
    }

    #[test]
    fn active_activations_empty_and_documented_rows() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getActiveActivations.txt"))
                .push(
                    200,
                    r#"{"status":"success","data":[{"activationId":"635468021","serviceCode":"vk","phoneNumber":"79********1","activationCost":12.5,"activationStatus":"4","smsCode":"12345","smsText":"Your code is 12345","activationTime":"2022-06-01 16:59:16","countryCode":"2","countryName":"Kazakhstan","canGetAnotherSms":"1","currency":840,"verificationType":"sms","subtype":1}],"activeActivations":{"affected_rows":0,"num_rows":0,"row":[],"rows":[]}}"#,
                ),
        );
        assert!(c.get_active_activations().unwrap().is_empty());
        let rows = c.get_active_activations_page(0, 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id.as_str(), "635468021");
        assert_eq!(rows[0].service, Some(ServiceCode::from("vk")));
        assert_eq!(rows[0].cost, Some(12.5));
        assert_eq!(rows[0].sms_code.as_deref(), Some("12345"));
        assert_eq!(rows[0].country, Some(CountryRef::Id(2)));
        assert_eq!(rows[0].can_get_another_sms, Some(true));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getActiveActivations"));
        assert!(reqs[1].ends_with("action=getActiveActivations&start=0&limit=1"));
    }

    #[test]
    fn status_and_set_status_tokens() {
        let c = client(
            FakeTransport::new()
                .push(200, "STATUS_WAIT_CODE")
                .push(200, "STATUS_OK:100001")
                .push(200, "ACCESS_CANCEL")
                .push(200, "ACCESS_RETRY_GET"),
        );
        let id = ActivationId::from("42");
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::WaitCode);
        assert_eq!(c.get_status(&id).unwrap().code(), Some("100001"));
        assert_eq!(c.cancel(&id).unwrap(), StatusAck::Cancel);
        assert_eq!(c.request_another_code(&id).unwrap(), StatusAck::RetryGet);
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getStatus&id=42"));
        assert!(reqs[2].ends_with("action=setStatus&id=42&status=8"));
        assert!(reqs[3].ends_with("action=setStatus&id=42&status=3"));
    }

    #[test]
    fn status_v2_shapes() {
        let c = client(
            FakeTransport::new()
                .push(
                    200,
                    r#"{"verificationType":"sms","data":{"id":"3416693217","phoneFrom":"Telegram","code":"123456","text":"Telegram code 123456","service":"tg","date":"2026-02-16T12:36:59+03:00","type":"sms"}}"#,
                )
                .push(200, "STATUS_CANCEL")
                .push(200, r#"{"verificationType":"sms","data":null}"#)
                .push(404, fixture!("getStatusV2_id_1.txt")),
        );
        let id = ActivationId::from("7");
        let s = c.get_status_v2(&id).unwrap();
        assert_eq!(s.code(), Some("123456"));
        match &s {
            StatusV2::Otp {
                verification_type,
                otp: Some(otp),
            } => {
                assert_eq!(verification_type.as_deref(), Some("sms"));
                assert_eq!(otp.id, "3416693217");
                assert_eq!(otp.phone_from.as_deref(), Some("Telegram"));
                assert_eq!(otp.service, Some(ServiceCode::from("tg")));
                assert_eq!(otp.kind.as_deref(), Some("sms"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            c.get_status_v2(&id).unwrap(),
            StatusV2::Plain(ActivationStatus::Cancelled)
        );
        let pending = c.get_status_v2(&id).unwrap();
        assert!(matches!(pending, StatusV2::Otp { otp: None, .. }));
        assert_eq!(pending.code(), None);
        assert!(matches!(c.get_status_v2(&id), Err(ApiError::NoActivation)));
        assert!(c.transport().requests()[0].ends_with("action=getStatusV2&id=7"));
    }

    #[test]
    fn all_sms_page_and_not_found() {
        let c = client(
            FakeTransport::new()
                .push(
                    200,
                    // documented `successfulOtpListExample` rows + the documented `meta` schema
                    r#"{"data":[{"id":"3416693217","phoneFrom":"Telegram","code":"123456","text":"Telegram code 123456","service":"tg","date":"2026-02-16T12:36:59+03:00","type":"sms"},{"id":"3416693218","phoneFrom":"213421421431","code":null,"text":null,"service":"tg","date":"2026-02-16T12:36:59+03:00","type":"call"}],"meta":{"total":2,"service":"tg"}}"#,
                )
                .push(404, fixture!("getAllSms_id_1.txt"))
                .push(200, r#"{"data":[]}"#),
        );
        let page = c
            .get_all_sms(&ActivationId::from("9"), Some(10), Some(1))
            .unwrap();
        assert_eq!(page.data.len(), 2);
        assert_eq!(page.data[0].code.as_deref(), Some("123456"));
        assert_eq!(page.data[1].code, None);
        assert_eq!(page.data[1].kind.as_deref(), Some("call"));
        assert_eq!(page.meta.total, Some(2));
        assert_eq!(page.meta.service, Some(ServiceCode::from("tg")));
        assert!(matches!(
            c.get_all_sms(&ActivationId::from("1"), None, None),
            Err(ApiError::NoActivation)
        ));
        // `meta` absent (the example without it) → defaults
        let empty = c.get_all_sms(&ActivationId::from("1"), None, None).unwrap();
        assert!(empty.data.is_empty());
        assert_eq!(empty.meta, PageMeta::default());
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getAllSms&id=9&size=10&page=1"));
        assert!(reqs[1].ends_with("action=getAllSms&id=1"));
    }

    #[test]
    fn history_rows() {
        // Shape from a live probe with the personal data replaced by the documented example values.
        let c = client(FakeTransport::new().push(
            200,
            r#"[{"id":"635468024","date":"2026-08-29 12:00:36","phone":"7*********0","sms":"Your code is ****","cost":0.1275,"status":"6","currency":840},{"id":"635468025","date":"2026-08-29 12:17:19","phone":"7*********1","sms":null,"cost":0,"status":"8","currency":840}]"#,
        ));
        let rows = c
            .get_history(&HistoryQuery::new().size(2).offset(0).start(1))
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id.as_str(), "635468024");
        assert_eq!(rows[0].cost, Some(0.1275));
        assert_eq!(rows[0].status.as_deref(), Some("6"));
        assert_eq!(rows[0].currency, Some(840));
        assert_eq!(rows[1].sms, None);
        assert_eq!(rows[1].cost, Some(0.0));
        assert!(only_request(&c).ends_with("action=getHistory&start=1&offset=0&size=2"));
    }

    #[test]
    fn finish_and_cancel_activation() {
        let c = client(
            FakeTransport::new()
                .push(204, "")
                .push(404, fixture!("cancelActivation_id_1.txt")),
        );
        c.finish_activation(&ActivationId::from("5")).unwrap();
        assert!(matches!(
            c.cancel_activation(&ActivationId::from("1")),
            Err(ApiError::NoActivation)
        ));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=finishActivation&id=5"));
        assert!(reqs[1].ends_with("action=cancelActivation&id=1"));
    }

    #[test]
    fn get_number_uses_v2_and_encodes_params() {
        let c = client(FakeTransport::new().push(
            200,
            r#"{"activationId":"635468024","phoneNumber":"79584000000","activationCost":12.5,"currency":840,"countryCode":6,"countryPhoneCode":62,"canGetAnotherSms":true,"activationTime":"2026-02-18T16:11:33+00:00","activationEndTime":"2026-02-18T18:11:23+00:00","activationOperator":"any","verificationType":"sms","subtype":1,"serviceCode":"vk","status":4}"#,
        ));
        let req = NumberRequest::new("tg", 187)
            .operator("tmobile,verizon")
            .max_price(0.5);
        let a = c.get_number(&req).unwrap();
        assert_eq!(a.id.as_str(), "635468024");
        assert_eq!(a.phone, "79584000000");
        assert_eq!(a.cost, Some(12.5));
        assert_eq!(a.country, Some(CountryRef::Id(6)));
        assert_eq!(a.operator.as_deref(), Some("any"));
        assert_eq!(
            only_request(&c),
            format!(
                "{ENDPOINT}?api_key=KEY&action=getNumberV2&service=tg&country=187&operator=tmobile%2Cverizon&maxPrice=0.5"
            )
        );
    }

    #[test]
    fn get_number_with_hero_options() {
        let c = client(FakeTransport::new().push(200, "ACCESS_NUMBER:1:2"));
        let req = NumberRequest::new("wa", 73).max_price(0.25);
        let opts = HeroNumberOptions::new()
            .fixed_price(true)
            .referral("123456789")
            .exclude_prefix("7934")
            .exclude_prefix("7900");
        let a = c.get_number_with(&req, &opts).unwrap();
        assert_eq!(a.phone, "2");
        assert!(only_request(&c).ends_with(
            "action=getNumberV2&service=wa&country=73&maxPrice=0.25&fixedPrice=true&ref=123456789&phoneException=7934%2C7900"
        ));
        // options never mutate the caller's request
        assert!(req.extra.is_empty());
    }

    #[test]
    fn get_number_error_shapes() {
        // getNumber (v1) answers the probe as plain-text `UNPROCESSABLE_ENTITY:service:INVALID`
        // with HTTP 200; the client never sends v1 (capability), so exercise the token directly.
        match classify(&HttpResponse::new(
            200,
            fixture!("getNumber_service___probe___country_187.txt"),
        )) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "service");
                assert_eq!(message, "INVALID");
            }
            other => panic!("unexpected: {other:?}"),
        }

        let c = client(
            FakeTransport::new()
                .push(422, fixture!("getNumberV2_service___probe___country_187.txt"))
                .push(200, "NO_NUMBERS")
                .push(
                    400,
                    r#"{"title":"WRONG_MAX_PRICE","details":"The maximum price is less than the permitted price","info":{"min":0.1234}}"#,
                )
                .push(402, r#"{"title":"NO_BALANCE","details":"Payment Required"}"#)
                .push(
                    403,
                    r#"{"title":"BANNED","details":"Your account is temporarily suspended from making any purchases.","info":{"scope":"global","banned_until":1739448000,"retry_after_seconds":3600,"readable_date":"2026-02-13T12:00:00+00:00"}}"#,
                )
                .push(
                    403,
                    r#"{"title":"CHANNELS_LIMIT","details":"You have reached the maximum number of concurrent threads (purchases) allowed for your account. Contact the technical support.","info":{"current_threads":0,"max_allowed":-1}}"#,
                )
                .push(500, r#"{"title":"SERVER_ERROR","details":"Server Gone"}"#),
        );
        let req = NumberRequest::new("__probe__", 187);
        // getNumberV2 → HTTP 422 JSON validation envelope
        match c.get_number(&req) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "service");
                assert_eq!(message, "INVALID");
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(c.transport().requests()[0].contains("action=getNumberV2&service=__probe__"));
        assert!(matches!(c.get_number(&req), Err(ApiError::NoNumbers)));
        assert!(matches!(
            c.get_number(&req),
            Err(ApiError::WrongMaxPrice { min: Some(m) }) if m == 0.1234
        ));
        assert!(matches!(c.get_number(&req), Err(ApiError::NoBalance)));
        assert!(matches!(
            c.get_number(&req),
            Err(ApiError::Banned { until }) if until == "2026-02-13T12:00:00+00:00"
        ));
        assert!(matches!(
            c.get_number(&req),
            Err(ApiError::Other(t)) if t.starts_with("CHANNELS_LIMIT: You have reached")
        ));
        assert!(matches!(
            c.get_number(&req),
            Err(ApiError::Other(t)) if t == "SERVER_ERROR: Server Gone"
        ));
        assert!(
            c.transport()
                .requests()
                .iter()
                .all(|u| u.contains("action=getNumberV2&"))
        );
    }

    #[test]
    fn price_bounds_only_max_price() {
        // the trait path strips `minPrice` (not part of the API) but keeps `maxPrice`
        let c = client(FakeTransport::new().push(200, "ACCESS_NUMBER:1:2"));
        let req = NumberRequest::new("tg", 187).min_price(0.1).max_price(0.5);
        c.get_number(&req).unwrap();
        let url = only_request(&c);
        assert!(url.ends_with("action=getNumberV2&service=tg&country=187&maxPrice=0.5"));
        assert!(!url.contains("minPrice"));

        // the typed path refuses a floor it cannot enforce, without a request
        let c = client(FakeTransport::new());
        assert!(matches!(
            c.get_number_with(&req, &HeroNumberOptions::new()),
            Err(ApiError::Unsupported("minPrice"))
        ));
        // fixedPrice without maxPrice is rejected before any request …
        let bare = NumberRequest::new("tg", 187);
        match c.get_number_with(&bare, &HeroNumberOptions::new().fixed_price(true)) {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "maxPrice");
                assert!(message.contains("fixedPrice"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(c.transport().requests().is_empty());
        // … and `apply` never emits the dangling flag either
        let applied = HeroNumberOptions::new().fixed_price(true).apply(&bare);
        assert!(applied.extra.is_empty());
        let applied = HeroNumberOptions::new()
            .fixed_price(true)
            .apply(&bare.clone().max_price(0.3));
        assert_eq!(
            applied.extra,
            vec![("fixedPrice".to_owned(), "true".to_owned())]
        );
    }

    #[test]
    fn error_envelopes_from_fixtures() {
        let c = client(
            FakeTransport::new()
                .push(401, fixture!("badkey_getBalance.txt"))
                .push(422, fixture!("nokey_getBalance.txt"))
                .push(404, fixture!("getStatus_id_1.txt"))
                .push(404, fixture!("getStatus_id_999999999999.txt"))
                .push(404, fixture!("setStatus_id_1_status_8.txt"))
                .push(422, fixture!("getStatus_id_abc.txt"))
                .push(422, fixture!("getStatus.txt"))
                .push(404, fixture!("getServices.txt"))
                .push(404, fixture!("getBalanceX.txt"))
                .push(404, fixture!("getFullSms_id_1.txt"))
                .push(404, fixture!("getPricesV2_service_tg_country_73.txt"))
                .push(404, fixture!("getPricesV3_service_tg_country_73.txt"))
                .push(429, fixture!("ratelimit_429.txt")),
        );
        assert!(matches!(c.get_balance(), Err(ApiError::BadKey)));
        match c.get_balance() {
            Err(ApiError::Validation { field, message }) => {
                assert_eq!(field, "api_key");
                assert_eq!(message, "REQUIRED");
            }
            other => panic!("unexpected: {other:?}"),
        }
        let id = ActivationId::from("1");
        assert!(matches!(c.get_status(&id), Err(ApiError::NoActivation)));
        assert!(matches!(c.get_status(&id), Err(ApiError::NoActivation)));
        assert!(matches!(c.cancel(&id), Err(ApiError::NoActivation)));
        assert!(matches!(
            c.get_status(&ActivationId::from("abc")),
            Err(ApiError::Validation { field, .. }) if field == "id"
        ));
        assert!(matches!(
            c.get_status(&ActivationId::from("")),
            Err(ApiError::Validation { field, .. }) if field == "id"
        ));
        // the envelope's `details` ("Method Not Found") is not an action name: empty payload,
        // exactly like the plain-text `BAD_ACTION` token
        for _ in 0..5 {
            assert!(matches!(
                c.get_balance(),
                Err(ApiError::BadAction(d)) if d.is_empty()
            ));
        }
        assert!(matches!(
            c.get_balance(),
            Err(ApiError::RateLimited { retry_after: None })
        ));
    }

    #[test]
    fn provider_only_methods_name_the_unknown_action() {
        let c = client(
            FakeTransport::new()
                .push(404, fixture!("getFullSms_id_1.txt"))
                .push(404, fixture!("getServices.txt"))
                .push(200, "BAD_ACTION"),
        );
        let id = ActivationId::from("1");
        assert!(matches!(
            c.get_status_v2(&id),
            Err(ApiError::BadAction(a)) if a == "getStatusV2"
        ));
        assert!(matches!(
            c.get_history(&HistoryQuery::new()),
            Err(ApiError::BadAction(a)) if a == "getHistory"
        ));
        assert!(matches!(
            c.get_all_sms(&id, None, None),
            Err(ApiError::BadAction(a)) if a == "getAllSms"
        ));
    }

    #[test]
    fn large_data_bodies_skip_the_error_shape_parse() {
        // > 4 KB bodies are only deserialised for an envelope / status when the key is up front
        let prices = fixture!("getPrices_service_tg.txt");
        assert!(prices.len() > ERROR_SHAPE_SCAN_LIMIT);
        assert!(!may_contain_top_level_key(prices, "\"title\""));
        assert!(!may_contain_top_level_key(prices, "\"status\""));
        assert!(classify(&HttpResponse::new(200, prices)).is_ok());
        let countries = fixture!("getCountries.txt");
        assert!(!may_contain_top_level_key(countries, "\"title\""));
        // … but a big envelope with the key first is still recognised
        let big_envelope = format!(
            r#"{{"title":"BAD_KEY","details":"{}"}}"#,
            "x".repeat(ERROR_SHAPE_SCAN_LIMIT)
        );
        assert!(matches!(
            classify(&HttpResponse::new(401, &big_envelope)),
            Err(ApiError::BadKey)
        ));
        let big_status = format!(
            r#"{{"status":"false","msg":"service is incorrect","pad":"{}"}}"#,
            "x".repeat(ERROR_SHAPE_SCAN_LIMIT)
        );
        assert!(matches!(
            classify(&HttpResponse::new(200, &big_status)),
            Err(ApiError::BadService)
        ));
        // small bodies are always inspected; multi-byte prefixes never split a char
        assert!(may_contain_top_level_key(r#"{"a":1}"#, "\"title\""));
        let cyrillic = format!(r#"{{"{}":1}}"#, "я".repeat(ERROR_SHAPE_SCAN_LIMIT));
        assert!(!may_contain_top_level_key(&cyrillic, "\"title\""));
    }

    #[test]
    fn legacy_plain_text_errors_with_http_200() {
        let c = client(
            FakeTransport::new()
                .push(200, "NO_KEY")
                .push(200, "BAD_KEY")
                .push(200, "ERROR_SQL")
                .push(200, "BAD_ACTION"),
        );
        assert!(matches!(c.get_balance(), Err(ApiError::BadKey)));
        assert!(matches!(c.get_balance(), Err(ApiError::BadKey)));
        assert!(matches!(c.get_balance(), Err(ApiError::Other(t)) if t == "ERROR_SQL"));
        assert!(matches!(c.get_balance(), Err(ApiError::BadAction(_))));
    }

    #[test]
    fn classify_edge_cases() {
        // non-JSON bodies with error statuses fall back to the HTTP error
        let html = HttpResponse::new(502, "<html>Bad Gateway</html>");
        assert!(matches!(
            classify(&html),
            Err(ApiError::Http { status: 502, .. })
        ));
        // 429 without an envelope is still a rate limit
        assert!(matches!(
            classify(&HttpResponse::new(429, "")),
            Err(ApiError::RateLimited { .. })
        ));
        // retry_after_seconds is honoured when the envelope carries it
        assert!(matches!(
            classify(&HttpResponse::new(
                429,
                r#"{"title":"RATE_LIMIT","details":"","info":{"retry_after_seconds":7}}"#
            )),
            Err(ApiError::RateLimited {
                retry_after: Some(7)
            })
        ));
        // data shapes are never mistaken for envelopes
        for body in [
            fixture!("getActiveActivations.txt"),
            fixture!("getOperators_country_73.txt"),
            fixture!("getNumbersStatus_country_0.txt"),
            r#"{"status":"success","services":[]}"#,
            "ACCESS_BALANCE:1",
            "",
        ] {
            assert!(classify(&HttpResponse::new(200, body)).is_ok(), "{body}");
        }
        assert!(classify(&HttpResponse::new(204, "")).is_ok());
        // an envelope with a non-error title is not an error
        assert!(error_from_envelope(r#"{"title":"hello","details":"x"}"#).is_none());
        assert!(matches!(
            error_from_envelope(
                r#"{"title":"EARLY_CANCEL_DENIED","details":"…","info":{"minActivationTime":120}}"#
            ),
            Some(ApiError::EarlyCancelDenied)
        ));
        assert!(matches!(
            error_from_envelope(r#"{"title":"BAD_STATUS","details":"Wrong status code"}"#),
            Some(ApiError::BadStatus)
        ));
    }

    #[test]
    fn truncation_detection_balances_brackets_outside_strings() {
        assert!(looks_truncated(r#"{"a":{"b":1"#));
        assert!(looks_truncated("[1,2"));
        assert!(!looks_truncated(r#"{"a":1}"#));
        assert!(!looks_truncated("ACCESS_BALANCE:1"));
        assert!(!looks_truncated(""));
        // cut right after an inner `}`: the last byte looks fine, the depth does not
        assert!(looks_truncated(r#"{"a":{"b":1}"#));
        assert!(looks_truncated(
            r#"{"170":{"tg":{"cost":0.45,"count":2369,"physicalCount":0}}"#
        ));
        // cut inside a string literal
        assert!(looks_truncated(r#"{"a":"abc"#));
        // brackets inside strings (and escaped quotes) do not count
        assert!(!looks_truncated(r#"{"a":"}","b":"[","c":"\"{"}"#));
        assert!(!looks_truncated(r#"["{", "\\"]"#));
        // the exact live cut (18 323 bytes of the full price table) and a few other cut points
        let full = fixture!("getPrices.txt");
        assert!(!looks_truncated(full));
        assert!(looks_truncated(&full[..18323]));
        for cut in [100usize, 4096, 65536, full.len() - 1] {
            let cut = (cut..full.len())
                .find(|&i| full.is_char_boundary(i))
                .unwrap();
            assert!(looks_truncated(&full[..cut]), "cut at {cut}");
        }
        assert!(!looks_truncated(fixture!("getCountries.txt")));
        assert!(!looks_truncated(fixture!("getServicesList.txt")));
        assert!(!looks_truncated(fixture!("getNumbersStatus_country_0.txt")));
    }

    #[test]
    fn retry_transport_action_allow_list() {
        assert_eq!(
            action_of(&format!("{ENDPOINT}?api_key=K&action=getBalance")),
            Some("getBalance")
        );
        assert_eq!(
            action_of(&format!(
                "{ENDPOINT}?api_key=K&action=getNumberV2&service=tg"
            )),
            Some("getNumberV2")
        );
        assert_eq!(action_of(ENDPOINT), None);
        assert_eq!(action_of(&format!("{ENDPOINT}?api_key=K")), None);
        for a in RetryTransport::<FakeTransport>::RETRYABLE_ACTIONS {
            assert!(RetryTransport::<FakeTransport>::is_retryable_url(&format!(
                "{ENDPOINT}?api_key=K&action={a}&x=1"
            )));
        }
        for a in [
            "getNumber",
            "getNumberV2",
            "getRentNumber",
            "setStatus",
            "finishActivation",
            "cancelActivation",
            "reactivate",
            "prolong",
            "somethingNew",
        ] {
            assert!(
                !RetryTransport::<FakeTransport>::is_retryable_url(&format!(
                    "{ENDPOINT}?api_key=K&action={a}&id=1"
                )),
                "{a}"
            );
        }
        assert!(!RetryTransport::<FakeTransport>::is_retryable_url(ENDPOINT));
    }

    #[test]
    fn retry_transport_never_repeats_a_purchase_or_state_change() {
        let req = NumberRequest::new("tg", 187);
        let id = ActivationId::from("5");

        // transport error on getNumberV2: exactly one request, the error comes back unchanged
        let t = RetryTransport::new(FakeTransport::new().push_error("read timeout"))
            .attempts(3)
            .retry_delay(Duration::ZERO);
        let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
        assert!(matches!(
            c.get_number(&req),
            Err(ApiError::Transport(e)) if e.0 == "read timeout"
        ));
        let reqs = c.transport().inner().requests();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].contains("action=getNumberV2&"));

        // truncated getNumberV2 body: returned as-is (surfaces as a parse error), not re-sent
        let t = RetryTransport::new(
            FakeTransport::new()
                .push(200, r#"{"activationId":"1","phoneNumber":"7"#)
                .push(200, r#"{"activationId":"2","phoneNumber":"79000000000"}"#),
        )
        .attempts(3)
        .retry_delay(Duration::ZERO);
        let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
        assert!(matches!(c.get_number(&req), Err(ApiError::Parse(_))));
        assert_eq!(c.transport().inner().requests().len(), 1);

        // the same for setStatus / finishActivation / cancelActivation
        type Retrying = HeroSms<RetryTransport<FakeTransport>>;
        type StateChange<'a> = Box<dyn Fn(&Retrying) -> ApiResult<()> + 'a>;
        let state_changes: Vec<(&str, StateChange<'_>)> = vec![
            ("setStatus", Box::new(|c| c.cancel(&id).map(|_| ()))),
            ("finishActivation", Box::new(|c| c.finish_activation(&id))),
            ("cancelActivation", Box::new(|c| c.cancel_activation(&id))),
        ];
        for (action, call) in state_changes {
            let t = RetryTransport::new(FakeTransport::new().push_error("reset"))
                .attempts(3)
                .retry_delay(Duration::ZERO);
            let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
            assert!(matches!(call(&c), Err(ApiError::Transport(_))), "{action}");
            let reqs = c.transport().inner().requests();
            assert_eq!(reqs.len(), 1, "{action}");
            assert!(reqs[0].contains(&format!("action={action}&")), "{action}");
        }

        // HTTP 429 was rejected before processing: retried even for a purchase
        let t = RetryTransport::new(
            FakeTransport::new()
                .push(429, fixture!("ratelimit_429.txt"))
                .push(200, "ACCESS_NUMBER:9:79000000000"),
        )
        .attempts(3)
        .rate_limit_delay(Duration::ZERO);
        let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
        assert_eq!(c.get_number(&req).unwrap().id.as_str(), "9");
        assert_eq!(c.transport().inner().requests().len(), 2);
    }

    #[test]
    fn retry_transport_recovers_from_truncation_errors_and_429() {
        let inner = FakeTransport::new()
            .push_error("connection reset")
            .push(200, r#"{"0":{"country":73,"pr"#)
            .push(429, fixture!("ratelimit_429.txt"))
            .push(200, fixture!("getTopCountriesByService_service_tg.txt"));
        let t = RetryTransport::new(inner)
            .attempts(4)
            .retry_delay(Duration::ZERO)
            .rate_limit_delay(Duration::ZERO);
        let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
        let rows = c.get_top_countries(&ServiceCode::from("tg")).unwrap();
        assert_eq!(rows[0].country, CountryRef::Id(73));
        assert_eq!(c.transport().inner().requests().len(), 4);

        // exhausted on 429 → the 429 is classified, not swallowed
        let t = RetryTransport::new(
            FakeTransport::new()
                .push(429, fixture!("ratelimit_429.txt"))
                .push(429, fixture!("ratelimit_429.txt")),
        )
        .attempts(2)
        .rate_limit_delay(Duration::ZERO);
        let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
        assert!(matches!(c.get_balance(), Err(ApiError::RateLimited { .. })));
        assert_eq!(c.transport().inner().requests().len(), 2);

        // exhausted on truncation → a transport error naming the cause
        let t = RetryTransport::new(FakeTransport::new().push(200, "{").push(200, "{"))
            .attempts(2)
            .retry_delay(Duration::ZERO);
        let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
        assert!(matches!(
            c.get_balance(),
            Err(ApiError::Transport(e)) if e.0.contains("truncated")
        ));

        // a healthy response passes through untouched
        let t = RetryTransport::new(FakeTransport::new().push(200, "ACCESS_BALANCE:2"));
        let c: HeroSms<RetryTransport<FakeTransport>> = Client::new(t, HeroSmsDialect, "KEY");
        assert_eq!(c.get_balance().unwrap(), 2.0);
        assert_eq!(c.transport().inner().requests().len(), 1);
    }

    #[test]
    fn trait_object() {
        let boxed: Box<dyn SmsActivateApi> = Box::new(client(
            FakeTransport::new().push(200, fixture!("getBalance.txt")),
        ));
        assert_eq!(boxed.provider(), "Hero SMS");
        assert_eq!(boxed.get_balance().unwrap(), 12.5085);
    }
}
