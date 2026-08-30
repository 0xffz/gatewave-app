//! SMSBower — <https://smsbower.app/api/?page=client>
//!
//! # Endpoint and authentication
//!
//! * Docs live on `smsbower.app`, the API on **`smsbower.page`** (different TLD):
//!   [`ENDPOINT`] = `https://smsbower.page/stubs/handler_api.php`.
//! * Every request is `GET <endpoint>?api_key=<key>&action=<action>&…`. The docs claim "POST or
//!   GET", but a POST is rejected with HTTP 405 and the JSON envelope
//!   `{"status":0,"message":"The POST method is not supported for route stubs/handler_api.php.
//!   Supported methods: GET, HEAD.","data":[]}` (fixture `post_getBalance.txt`).
//! * A wrong key is **not** the classic `BAD_KEY` token: it is HTTP 401 with
//!   `{"status":0,"message":"No access","data":[]}` (fixture `badkey_getBalance.txt`). The dialect
//!   maps that envelope to [`ApiError::BadKey`].
//! * Balance and prices are in USD.
//!
//! # Capability matrix (evidence: live probes of 2026-08-30 in `fixtures/smsbower/` + docs)
//!
//! | action | status | notes |
//! | --- | --- | --- |
//! | `getBalance` | yes | `ACCESS_BALANCE:18.739` |
//! | `getNumber` | yes | documented; `service=__probe__` probe → `WRONG_SERVICE` (exists) |
//! | `getNumberV2` | **yes** | documented; same probe → `WRONG_SERVICE` (exists); JSON activation |
//! | `getStatus` / `setStatus` | yes | plain tokens; unknown **and non-numeric** ids → `NO_ACTIVATION` |
//! | `getPrices` | yes | `{country:{service:{cost,count}}}` — no `physicalCount` |
//! | `getPricesV2` | **yes** | price → count histogram, see [`SmsBower::get_prices_v2`] |
//! | `getPricesV3` | **yes** | per upstream partner, see [`SmsBower::get_prices_v3`] |
//! | `getServicesList` | yes | `{status:"success",services:[{code,name}]}` |
//! | `getCountries` | yes | `{id:{id:"<string>",rus,eng,chn}}` — ids are strings, no `visible`/`retry`/`rent` |
//! | `getTopCountriesByService` | yes | **country slug** keys, partner-id inner keys (own parser) |
//! | `getOperators` | yes | undocumented but live: `{status:"success",countryOperators:{"187":[]}}` |
//! | `getNumbersStatus` | no | `BAD_ACTION` |
//! | `getActiveActivations` | no | `BAD_ACTION` |
//! | `getFullSms` | no | `BAD_ACTION` |
//! | `maxPrice` / `minPrice` | yes | documented for `getNumber` and `getNumberV2` |
//! | `providerIds` / `exceptProviderIds` / `phoneException` / `ref` / `userID` | yes | see [`SmsBowerRequestExt`] |
//!
//! # Quirks confirmed against live responses
//!
//! * Country-indexed tables (`getCountries`, `getPrices`, and by extension `getPricesV2/V3`) carry
//!   one row keyed `""`: the Faroe Islands (`"id":null`). It is not junk data — the row has real
//!   stock (`{"tg":{"cost":0.306,"count":1052}}` in `getPrices_service_tg.txt`) — but without a
//!   country id it cannot be purchased, so the standard country/price parsers and
//!   [`parse_prices_v2`] / [`parse_prices_v3`] skip it. In [`parse_prices_v2`] / [`parse_prices_v3`]
//!   any *other* non-numeric country key is a parse error, so an error envelope can never be
//!   mistaken for an empty table (the standard `getPrices` parser keeps such keys as
//!   [`CountryRef::Slug`]; its `{"error":…}` envelope still fails to parse because the value is
//!   not a service map).
//! * `getTopCountriesByService` returns `{"<country-slug>":{"<partnerId>":{"price","count"}}}`
//!   (e.g. `"united-states"`, `"united-states-virtual"`), not the indexed
//!   `{idx:{country,price,…}}` shape. Rows become [`TopCountry`] with
//!   [`CountryRef::Slug`] and `provider_id = Some(partnerId)` **in the provider's order**: the
//!   docs describe it as "top 10 countries sorted by internal priority; for each country the
//!   Gold-ranked partners sorted by sales count from best to worst", so `rows[0]` is the best
//!   offer. (JSON object order is read with an order-preserving serde visitor, `OrderedMap`.)
//! * The error token depends on the action. `getNumber`/`getNumberV2` answer `WRONG_SERVICE` for
//!   an unknown service (the docs list `BAD_SERVICE`); `getPricesV3` answers the documented plain
//!   `BAD_SERVICE` / `BAD_COUNTRY`; `getPrices`, `getPricesV2` and `getTopCountriesByService`
//!   answer an HTTP 200 JSON envelope `{"error":"…"}` (see *Error shapes*). All of them map to
//!   [`ApiError::BadService`] / [`ApiError::BadCountry`].
//! * `getOperators` with an unknown country answers `{"status":"success","countryOperators":[]}`
//!   — PHP's empty array where an object is expected (fixture `getOperators_country_9999.txt`).
//!   The dialect returns an empty map for it.
//! * `getStatus`/`setStatus` answer `NO_ACTIVATION` for any unknown id, numeric or not (no
//!   validation envelope).
//! * Unknown actions answer the plain token `BAD_ACTION` with HTTP 200.
//! * `getCountries` ids are JSON strings (`"id":"187"`); the country numbering is sms-activate's
//!   (187 = USA, 0 = Russia).
//! * `getPricesV2`/`getPricesV3` are documented with `service` and `country` as plain parameters
//!   (not marked optional); they are sent only when given.
//! * The `getNumberV2` doc template omits `phoneException` and `ref`, but states that it "takes
//!   the same parameters" as `getNumber`; the request helpers add them regardless.
//!
//! # Error shapes
//!
//! Both JSON envelopes are recognised by their **first key** (`status` or `error`), so large data
//! bodies such as an unfiltered `getPrices` are never parsed twice.
//!
//! | observed | mapped to |
//! | --- | --- |
//! | HTTP 401 `{"status":0,"message":"No access","data":[]}` | [`ApiError::BadKey`] |
//! | HTTP 405 `{"status":0,"message":"The POST method is not supported…","data":[]}` | [`ApiError::Http`] `{status: 405, body: message}` |
//! | any other HTTP ≥ 300 `{"status":0,"message":…}` | [`ApiError::Http`] with the message as body |
//! | HTTP 200 `{"status":0,"message":…}` | the token in `message` if it is one, else [`ApiError::Other`] |
//! | HTTP 200 `{"error":"Bad service"}` (`getPrices`, `getPricesV2`, unknown service) | [`ApiError::BadService`] |
//! | HTTP 200 `{"error":"Bad country"}` (`getPrices`, `getPricesV2`, unknown country) | [`ApiError::BadCountry`] |
//! | HTTP 200 `{"error":"BAD_SERVICE"}` (`getTopCountriesByService`, unknown service) | [`ApiError::BadService`] |
//! | HTTP 200 `{"error":"<other>"}` | the token if it is one, the normalised message (`Bad country` → `BAD_COUNTRY`) if *that* is a known token, else [`ApiError::Other`] |
//! | HTTP 429 | [`ApiError::RateLimited`] |
//! | `NO_ACTIVATION` | [`ApiError::NoActivation`] |
//! | `BAD_ACTION` | [`ApiError::BadAction`] |
//! | `WRONG_SERVICE` (`getNumber`, `getNumberV2`) / `BAD_SERVICE` (`getPricesV3`) | [`ApiError::BadService`] |
//! | `BAD_COUNTRY` (`getPricesV3`) | [`ApiError::BadCountry`] |
//! | `BAD_STATUS` | [`ApiError::BadStatus`] |
//! | `EARLY_CANCEL_DENIED` | [`ApiError::EarlyCancelDenied`] — see [`CANCEL_GRACE_PERIOD`] |
//! | `BAD_KEY` (documented, never observed) | [`ApiError::BadKey`] |
//! | `{"status":"success","countryOperators":[]}` (`getOperators`, unknown country) | not an error: `Ok(empty map)` |
//!
//! # Activation state machine (from the docs)
//!
//! After `getNumber(V2)`: `setStatus` 8 cancels, 1 (optional) reports "SMS sent". After status 1:
//! only 8. Once a code arrived: 3 requests another SMS (free), 6 completes. After 3: 6.
//! **Cancelling is refused with `EARLY_CANCEL_DENIED` until 2 minutes after the purchase**
//! ([`CANCEL_GRACE_PERIOD`]).
//!
//! # Rate limits
//!
//! Not documented and no 429 was observed. Keep traffic ≤ 1 request/second and back off ≥ 5 s
//! on [`ApiError::RateLimited`].
//!
//! # Webhook (documentation only — not implemented here)
//!
//! With a webhook URL configured in the profile, SMSBower POSTs a JSON body when an SMS arrives,
//! from IP `167.235.198.205`:
//!
//! ```json
//! {
//!   "activationId": 123456,
//!   "service": "go",
//!   "text": "Sms text",
//!   "code": "12345",
//!   "country": 2,
//!   "receivedAt": "2023-01-01 12:00:00"
//! }
//! ```
//!
//! The receiver must answer HTTP 200; otherwise two retries follow (after 1 min and 5 min) and a
//! failure notification is shown in the profile after three failed attempts.
//!
//! Out of scope: the static-wallet payment endpoint
//! (`/api/payment/getActualWalletAddress?api_key=…&coin=…&network=…`).
//!
//! # Example
//!
//! ```no_run
//! use sms_activate::providers::smsbower::{SmsBower, SmsBowerRequestExt};
//! use sms_activate::{NumberRequest, ServiceCode, SmsActivateApi};
//!
//! let bower = SmsBower::with_api_key(std::env::var("SMSBOWER_API_KEY").unwrap());
//! let offers = bower.get_prices_v3(Some(&ServiceCode::from("tg")), Some(187)).unwrap();
//! let cheapest = &offers[&187][&ServiceCode::from("tg")][0];
//! let req = NumberRequest::new("tg", 187)
//!     .max_price(cheapest.price)
//!     .provider_ids([&cheapest.provider_id]);
//! let activation = bower.get_number(&req).unwrap();
//! println!("{} costs {:?}", activation.phone, activation.cost);
//! ```

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::marker::PhantomData;
use std::time::Duration;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

use crate::api::{Client, Dialect};
use crate::error::{ApiError, ApiResult};
use crate::protocol::{self, as_object, value_to_f64, value_to_string, value_to_u64};
use crate::transport::{HttpResponse, Transport};
use crate::types::*;

/// The API host is `smsbower.page`, not the `smsbower.app` documentation host.
pub const ENDPOINT: &str = "https://smsbower.page/stubs/handler_api.php";

/// `setStatus` 8 (cancel) is refused with `EARLY_CANCEL_DENIED` until this long after the purchase.
pub const CANCEL_GRACE_PERIOD: Duration = Duration::from_secs(2 * 60);

/// Query-parameter names of the SMSBower-specific `getNumber`/`getNumberV2` filters.
pub mod params {
    /// Comma-separated upstream partner ids to buy from (`1,2,3`).
    pub const PROVIDER_IDS: &str = "providerIds";
    /// Comma-separated upstream partner ids to exclude.
    pub const EXCEPT_PROVIDER_IDS: &str = "exceptProviderIds";
    /// Comma-separated number prefixes to exclude: country code + 3–6 mask digits (`7918,7900111`).
    pub const PHONE_EXCEPTION: &str = "phoneException";
    /// Referral id.
    pub const REF: &str = "ref";
    /// Reseller sub-user id (contact support).
    pub const USER_ID: &str = "userID";
}

#[derive(Clone, Debug, Default)]
pub struct SmsBowerDialect;

impl Dialect for SmsBowerDialect {
    fn name(&self) -> &'static str {
        "SMSBower"
    }

    fn endpoint(&self) -> &str {
        ENDPOINT
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            get_number_v2: true,
            numbers_status: false,
            active_activations: false,
            operators: true,
            prices_v2: true,
            prices_v3: true,
            price_bounds: true,
            provider_filters: true,
        }
    }

    fn classify(&self, resp: &HttpResponse) -> ApiResult<()> {
        if let Some(err) = envelope_error(resp) {
            return Err(err);
        }
        protocol::classify_standard(resp)
    }

    fn parse_top_countries(&self, body: &str) -> ApiResult<Vec<TopCountry>> {
        parse_top_countries_by_slug(body)
    }

    fn parse_operators(&self, body: &str) -> ApiResult<BTreeMap<CountryRef, Vec<String>>> {
        // An unknown country answers `{"status":"success","countryOperators":[]}` — PHP's empty
        // array where the standard parser expects an object (fixture `getOperators_country_9999.txt`).
        let v: Value = serde_json::from_str(body.trim())?;
        match v.get("countryOperators") {
            Some(Value::Array(a)) if a.is_empty() => Ok(BTreeMap::new()),
            _ => protocol::parse_operators(body),
        }
    }
}

#[cfg(feature = "ureq")]
pub type SmsBower<T = crate::transport::UreqTransport> = Client<T, SmsBowerDialect>;
#[cfg(not(feature = "ureq"))]
pub type SmsBower<T> = Client<T, SmsBowerDialect>;

#[cfg(feature = "ureq")]
impl SmsBower {
    /// Client over the default `ureq` transport.
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Client::new(
            crate::transport::UreqTransport::new(),
            SmsBowerDialect,
            api_key,
        )
    }
}

// ---------------------------------------------------------------------------
// Provider-only actions

/// One bucket of the `getPricesV2` histogram: `count` numbers are on offer at exactly `price`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PriceBucket {
    pub price: f64,
    pub count: u64,
}

/// `getPricesV2` result: country → service → buckets sorted by ascending price.
pub type PriceHistogramTable = BTreeMap<CountryId, BTreeMap<ServiceCode, Vec<PriceBucket>>>;

/// One upstream partner's offer from `getPricesV3`. `provider_id` is the value to pass to
/// [`SmsBowerRequestExt::provider_ids`] / [`SmsBowerRequestExt::except_provider_ids`].
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderOffer {
    pub provider_id: String,
    pub price: f64,
    pub count: u64,
}

/// `getPricesV3` result: country → service → offers sorted by ascending price, then provider id.
pub type ProviderOfferTable = BTreeMap<CountryId, BTreeMap<ServiceCode, Vec<ProviderOffer>>>;

impl<T: Transport> SmsBower<T> {
    /// `getPricesV2` — how many numbers are available at each distinct price.
    ///
    /// Both filters are documented as parameters; omit them to get every country/service
    /// (a large response). An unknown service/country is [`ApiError::BadService`] /
    /// [`ApiError::BadCountry`] (HTTP 200 `{"error":…}` envelope), never an empty table.
    pub fn get_prices_v2(
        &self,
        service: Option<&ServiceCode>,
        country: Option<CountryId>,
    ) -> ApiResult<PriceHistogramTable> {
        let body = self.call("getPricesV2", price_params(service, country))?;
        parse_prices_v2(&body)
    }

    /// `getPricesV3` — price and stock per upstream partner (the ids usable with
    /// `providerIds` / `exceptProviderIds`). An unknown service/country is the plain token
    /// `BAD_SERVICE` / `BAD_COUNTRY`.
    pub fn get_prices_v3(
        &self,
        service: Option<&ServiceCode>,
        country: Option<CountryId>,
    ) -> ApiResult<ProviderOfferTable> {
        let body = self.call("getPricesV3", price_params(service, country))?;
        parse_prices_v3(&body)
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

/// `{"187":{"tg":{"0.683":22,"0.697":157,…}}}` (fixture `getPricesV2_service_tg_country_187.txt`).
pub fn parse_prices_v2(body: &str) -> ApiResult<PriceHistogramTable> {
    walk_country_service(body, "getPricesV2", |cell| {
        let mut buckets = Vec::new();
        for (price, count) in as_object(cell, "getPricesV2 histogram")? {
            let price: f64 = price
                .trim()
                .parse()
                .map_err(|_| ApiError::Parse(format!("bad price key `{price}`")))?;
            buckets.push(PriceBucket {
                price,
                count: value_to_u64(Some(count)).unwrap_or(0),
            });
        }
        buckets.sort_by(|a, b| cmp_f64(a.price, b.price));
        Ok(buckets)
    })
}

/// `{"187":{"tg":{"2263":{"count":98,"price":2.592,"provider_id":2263},…}}}`
/// (fixture `getPricesV3_service_tg_country_187.txt`).
pub fn parse_prices_v3(body: &str) -> ApiResult<ProviderOfferTable> {
    walk_country_service(body, "getPricesV3", |cell| {
        let mut offers = Vec::new();
        for (key, offer) in as_object(cell, "getPricesV3 offers")? {
            offers.push(ProviderOffer {
                provider_id: value_to_string(offer.get("provider_id"))
                    .unwrap_or_else(|| key.clone()),
                price: value_to_f64(offer.get("price"))
                    .ok_or_else(|| ApiError::Parse(format!("missing price for provider {key}")))?,
                count: value_to_u64(offer.get("count")).unwrap_or(0),
            });
        }
        offers.sort_by(|a, b| {
            cmp_f64(a.price, b.price).then_with(|| cmp_numeric_str(&a.provider_id, &b.provider_id))
        });
        Ok(offers)
    })
}

/// Walks a `{country:{service:<cell>}}` table. The id-less `""` (Faroe Islands) row is skipped;
/// any other non-numeric country key is a parse error, so an error envelope that slipped past
/// classification is never reported as "no data". An empty array (`[]`) is accepted as "no data".
fn walk_country_service<C>(
    body: &str,
    what: &str,
    mut cell: impl FnMut(&Value) -> ApiResult<C>,
) -> ApiResult<BTreeMap<CountryId, BTreeMap<ServiceCode, C>>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let mut table = BTreeMap::new();
    if matches!(&v, Value::Array(a) if a.is_empty()) {
        return Ok(table);
    }
    for (country, services) in as_object(&v, what)? {
        if country.is_empty() {
            continue;
        }
        let country_id: CountryId = country
            .parse()
            .map_err(|_| ApiError::Parse(format!("bad country key `{country}` in {what}")))?;
        let mut row = BTreeMap::new();
        for (service, value) in as_object(services, what)? {
            row.insert(ServiceCode(service.clone()), cell(value)?);
        }
        table.insert(country_id, row);
    }
    Ok(table)
}

/// `{"united-states":{"3170":{"price":0.765,"count":1000},…},"canada":{…}}`
/// (fixture `getTopCountriesByService_service_tg.txt`).
///
/// Rows keep the provider's order — countries by internal priority, partners by sales count,
/// best first — so `rows[0]` is the top offer. `[]` is accepted as "no data".
pub fn parse_top_countries_by_slug(body: &str) -> ApiResult<Vec<TopCountry>> {
    let body = body.trim();
    if !(body.starts_with('{') || body.starts_with('[')) {
        return Err(ApiError::Unexpected(body.to_owned()));
    }
    let countries: OrderedMap<OrderedMap<Value>> = serde_json::from_str(body)?;
    let mut rows = Vec::new();
    for (slug, partners) in countries.0 {
        for (partner, offer) in partners.0 {
            rows.push(TopCountry {
                country: CountryRef::Slug(slug.clone()),
                price: value_to_f64(offer.get("price")).unwrap_or(0.0),
                retail_price: None,
                count: value_to_u64(offer.get("count")).unwrap_or(0),
                provider_id: Some(partner),
            });
        }
    }
    Ok(rows)
}

/// A JSON object read in document order.
///
/// `serde_json::Map` sorts keys unless the crate's `preserve_order` feature is enabled, but
/// `MapAccess` always yields entries as written, so this keeps the provider's ranking without a
/// Cargo change. An empty array (PHP's encoding of an empty associative array) is accepted as an
/// empty object.
struct OrderedMap<V>(Vec<(String, V)>);

impl<'de, V: Deserialize<'de>> Deserialize<'de> for OrderedMap<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor<V>(PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for MapVisitor<V> {
            type Value = OrderedMap<V>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(OrderedMap(entries))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_type(de::Unexpected::Seq, &self));
                }
                Ok(OrderedMap(Vec::new()))
            }
        }

        deserializer.deserialize_any(MapVisitor(PhantomData))
    }
}

/// SMSBower has two JSON error envelopes, both recognised by their **first key** so that large
/// data bodies (`getPrices`, `getCountries`, …) are not parsed here at all:
///
/// * framework failures `{"status":0,"message":"…","data":[]}` (401 bad key, 405 wrong method);
/// * action-level failures `{"error":"…"}` with HTTP 200 (`getPrices`, `getPricesV2`,
///   `getTopCountriesByService` with an unknown service or country).
///
/// Returns `None` for anything that is not such an envelope.
fn envelope_error(resp: &HttpResponse) -> Option<ApiError> {
    let body = resp.body.trim();
    let message = match first_json_key(body)? {
        "status" => {
            let v: Value = serde_json::from_str(body).ok()?;
            let status = v.get("status")?;
            if !(status.as_i64() == Some(0) || status.as_str() == Some("0")) {
                return None;
            }
            value_to_string(v.get("message")).unwrap_or_default()
        }
        "error" => {
            let v: Value = serde_json::from_str(body).ok()?;
            v.get("error")?.as_str()?.to_owned()
        }
        _ => return None,
    };
    Some(match resp.status {
        401 | 403 => ApiError::BadKey,
        429 => ApiError::RateLimited { retry_after: None },
        _ if message.eq_ignore_ascii_case("no access") => ApiError::BadKey,
        status if !(200..300).contains(&status) => ApiError::Http {
            status,
            body: message,
        },
        _ => error_from_message(&message),
    })
}

/// Key of the first member of a JSON object (`{"status":…` → `status`) without parsing the rest.
fn first_json_key(body: &str) -> Option<&str> {
    let rest = body.strip_prefix('{')?.trim_start().strip_prefix('"')?;
    rest.split_once('"').map(|(key, _)| key)
}

/// Maps an envelope message: a classic token as is (`BAD_SERVICE`), a human-readable variant of
/// one via normalisation (`Bad country` → `BAD_COUNTRY`), anything else to [`ApiError::Other`]
/// carrying the original text.
fn error_from_message(message: &str) -> ApiError {
    if let Some(err) = ApiError::from_code(message) {
        return err;
    }
    let token = message.trim().to_ascii_uppercase().replace(' ', "_");
    match ApiError::from_code(&token) {
        Some(ApiError::Other(_)) | None => ApiError::Other(message.to_owned()),
        Some(err) => err,
    }
}

fn cmp_f64(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// Orders numeric ids numerically and falls back to string order for anything else.
fn cmp_numeric_str(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

// ---------------------------------------------------------------------------
// Request helpers

/// Builder helpers for the SMSBower-specific `getNumber` / `getNumberV2` parameters. Each call
/// replaces a previously set value for the same parameter; an empty list (or empty string)
/// *removes* the parameter instead of sending `providerIds=` — the docs give no meaning to an
/// empty filter and this is a paid request, so "empty" is treated as "no filter".
///
/// ```
/// use sms_activate::NumberRequest;
/// use sms_activate::providers::smsbower::SmsBowerRequestExt;
///
/// let req = NumberRequest::new("tg", 187)
///     .max_price(1.0)
///     .provider_ids([2263, 3170])
///     .phone_exception(["1212", "1917"]);
/// assert!(req.extra.contains(&("providerIds".to_owned(), "2263,3170".to_owned())));
/// assert!(req.provider_ids(Vec::<u64>::new()).extra.iter().all(|(k, _)| k != "providerIds"));
/// ```
pub trait SmsBowerRequestExt: Sized {
    /// `providerIds` — only buy from these upstream partners (see [`ProviderOffer::provider_id`]).
    fn provider_ids<I>(self, ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display;

    /// `exceptProviderIds` — never buy from these upstream partners.
    fn except_provider_ids<I>(self, ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display;

    /// `phoneException` — number prefixes to avoid: country code followed by 3–6 digits of the
    /// mask (`7918`, `7900111`).
    fn phone_exception<I>(self, prefixes: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display;

    /// `ref` — referral id.
    fn referral(self, referral_id: impl Display) -> Self;

    /// `userID` — reseller sub-user id (contact SMSBower support for details).
    fn user_id(self, user_id: impl Display) -> Self;
}

impl SmsBowerRequestExt for NumberRequest {
    fn provider_ids<I>(self, ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display,
    {
        set_extra(self, params::PROVIDER_IDS, join_csv(ids))
    }

    fn except_provider_ids<I>(self, ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display,
    {
        set_extra(self, params::EXCEPT_PROVIDER_IDS, join_csv(ids))
    }

    fn phone_exception<I>(self, prefixes: I) -> Self
    where
        I: IntoIterator,
        I::Item: Display,
    {
        set_extra(self, params::PHONE_EXCEPTION, join_csv(prefixes))
    }

    fn referral(self, referral_id: impl Display) -> Self {
        set_extra(self, params::REF, referral_id.to_string())
    }

    fn user_id(self, user_id: impl Display) -> Self {
        set_extra(self, params::USER_ID, user_id.to_string())
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

/// Replaces `key` in `extra`; an empty value only removes it (never sends `key=`).
fn set_extra(mut req: NumberRequest, key: &str, value: String) -> NumberRequest {
    req.extra.retain(|(k, _)| k != key);
    if !value.is_empty() {
        req.extra.push((key.to_owned(), value));
    }
    req
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SmsActivateApi;
    use crate::transport::FakeTransport;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../fixtures/smsbower/", $name))
        };
    }

    fn client(t: FakeTransport) -> SmsBower<FakeTransport> {
        Client::new(t, SmsBowerDialect, "KEY")
    }

    fn tg() -> ServiceCode {
        ServiceCode::from("tg")
    }

    fn probe() -> ServiceCode {
        ServiceCode::from("__probe__")
    }

    #[test]
    fn identity_and_capabilities() {
        let c = client(FakeTransport::new());
        assert_eq!(c.provider(), "SMSBower");
        assert_eq!(c.dialect().endpoint(), ENDPOINT);
        assert!(ENDPOINT.starts_with("https://smsbower.page/"));
        let caps = c.capabilities();
        assert!(caps.get_number_v2);
        assert!(caps.operators);
        assert!(caps.prices_v2 && caps.prices_v3);
        assert!(caps.price_bounds && caps.provider_filters);
        assert!(!caps.numbers_status);
        assert!(!caps.active_activations);
        assert_eq!(CANCEL_GRACE_PERIOD, Duration::from_secs(120));
    }

    #[test]
    fn unsupported_actions_do_not_hit_the_network() {
        let c = client(FakeTransport::new());
        assert!(matches!(
            c.get_numbers_status(&CountryRef::Id(187), None),
            Err(ApiError::Unsupported("getNumbersStatus"))
        ));
        assert!(matches!(
            c.get_active_activations(),
            Err(ApiError::Unsupported("getActiveActivations"))
        ));
        assert!(c.transport().requests().is_empty());
    }

    #[test]
    fn balance() {
        let c = client(FakeTransport::new().push(200, fixture!("getBalance.txt")));
        assert_eq!(c.get_balance().unwrap(), 18.739);
        assert_eq!(
            c.transport().requests(),
            vec![format!("{ENDPOINT}?api_key=KEY&action=getBalance")]
        );
    }

    #[test]
    fn prices_skip_faroe_row_and_reject_unknown_service_or_country() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getPrices_service_tg.txt"))
                .push(200, fixture!("getPrices_service_tg_country_187.txt"))
                .push(200, fixture!("getPrices_service_tg_country_9999.txt"))
                .push(200, fixture!("getPrices_service___probe___country_187.txt")),
        );
        let all = c.get_prices(Some(&tg()), None).unwrap();
        // 194 keys in the fixture, one of them the id-less `""` Faroe row (which has stock).
        assert!(fixture!("getPrices_service_tg.txt").contains(r#""":{"tg":{"cost":0.306"#));
        assert_eq!(all.len(), 193);
        assert_eq!(all[&CountryRef::Id(187)][&tg()].cost, 2.592);
        assert_eq!(all[&CountryRef::Id(187)][&tg()].count, 213697);
        assert_eq!(all[&CountryRef::Id(31)][&tg()].cost, 0.239);
        assert!(all.values().all(|row| row.contains_key(&tg())));

        let usa = c
            .get_prices(Some(&tg()), Some(&CountryRef::Id(187)))
            .unwrap();
        assert_eq!(usa.len(), 1);
        assert_eq!(usa[&CountryRef::Id(187)][&tg()].physical_count, None);

        // An unknown country/service is an HTTP 200 `{"error":"…"}` envelope, never `Ok({})`.
        assert!(matches!(
            c.get_prices(Some(&tg()), Some(&CountryRef::Id(9999))),
            Err(ApiError::BadCountry)
        ));
        assert!(matches!(
            c.get_prices(Some(&probe()), Some(&CountryRef::Id(187))),
            Err(ApiError::BadService)
        ));
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getPrices&service=tg"));
        assert!(reqs[1].ends_with("action=getPrices&service=tg&country=187"));
        assert!(reqs[2].ends_with("action=getPrices&service=tg&country=9999"));
        assert!(reqs[3].ends_with("action=getPrices&service=__probe__&country=187"));
    }

    #[test]
    fn json_error_envelopes_on_price_and_top_country_actions() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getPricesV2_service_tg_country_9999.txt"))
                .push(
                    200,
                    fixture!("getPricesV2_service___probe___country_187.txt"),
                )
                .push(200, fixture!("getPricesV3_service_tg_country_9999.txt"))
                .push(
                    200,
                    fixture!("getPricesV3_service___probe___country_187.txt"),
                )
                .push(
                    200,
                    fixture!("getTopCountriesByService_service___probe__.txt"),
                ),
        );
        assert!(matches!(
            c.get_prices_v2(Some(&tg()), Some(9999)),
            Err(ApiError::BadCountry)
        ));
        assert!(matches!(
            c.get_prices_v2(Some(&probe()), Some(187)),
            Err(ApiError::BadService)
        ));
        // V3 alone answers the documented plain tokens.
        assert_eq!(
            fixture!("getPricesV3_service_tg_country_9999.txt"),
            "BAD_COUNTRY"
        );
        assert!(matches!(
            c.get_prices_v3(Some(&tg()), Some(9999)),
            Err(ApiError::BadCountry)
        ));
        assert!(matches!(
            c.get_prices_v3(Some(&probe()), Some(187)),
            Err(ApiError::BadService)
        ));
        // getTopCountriesByService uses the token inside the envelope.
        assert_eq!(
            fixture!("getTopCountriesByService_service___probe__.txt"),
            r#"{"error":"BAD_SERVICE"}"#
        );
        assert!(matches!(
            c.get_top_countries(&probe()),
            Err(ApiError::BadService)
        ));
        let reqs = c.transport().requests();
        assert!(reqs[1].ends_with("action=getPricesV2&service=__probe__&country=187"));
        assert!(reqs[4].ends_with("action=getTopCountriesByService&service=__probe__"));

        // Unknown messages in the same envelope are surfaced, never swallowed; whitespace and a
        // non-2xx status are handled too.
        let c = client(
            FakeTransport::new()
                .push(200, r#"{"error":"Something odd"}"#)
                .push(200, r#"{ "error" : "Wrong max price" }"#)
                .push(500, r#"{"error":"Bad country"}"#),
        );
        assert!(matches!(
            c.get_prices(None, None),
            Err(ApiError::Other(m)) if m == "Something odd"
        ));
        // A human-readable variant of any known token is normalised (`WRONG_MAX_PRICE`).
        assert!(matches!(
            c.get_prices(None, None),
            Err(ApiError::WrongMaxPrice { min: None })
        ));
        assert!(matches!(
            c.get_prices(None, None),
            Err(ApiError::Http { status: 500, body }) if body == "Bad country"
        ));

        // Even if an envelope slipped past classification, no parser turns it into "no data".
        assert!(matches!(
            parse_prices_v2(r#"{"error":"Bad country"}"#),
            Err(ApiError::Parse(_))
        ));
        assert!(matches!(
            parse_prices_v3(r#"{"error":"Bad country"}"#),
            Err(ApiError::Parse(_))
        ));
        assert!(matches!(
            parse_top_countries_by_slug(r#"{"error":"BAD_SERVICE"}"#),
            Err(ApiError::Parse(_))
        ));
        assert!(matches!(
            protocol::parse_prices(r#"{"error":"Bad country"}"#),
            Err(ApiError::Parse(_))
        ));
    }

    #[test]
    fn envelope_detection_uses_the_first_key() {
        assert_eq!(first_json_key(r#"{"status":0}"#), Some("status"));
        assert_eq!(first_json_key("{ \n \"error\" : \"x\" }"), Some("error"));
        assert_eq!(first_json_key(r#"{"187":{"tg":{}}}"#), Some("187"));
        assert_eq!(first_json_key("{}"), None);
        assert_eq!(first_json_key("[]"), None);
        assert_eq!(first_json_key("ACCESS_BALANCE:1"), None);

        // Data bodies whose first key is neither `status` nor `error` are never inspected.
        let ok = |body: &str| envelope_error(&HttpResponse::new(200, body)).is_none();
        assert!(ok(fixture!("getCountries.txt")));
        assert!(ok(fixture!("getPrices_service_tg.txt")));
        assert!(ok(fixture!("getTopCountriesByService_service_tg.txt")));
        assert!(ok(fixture!("getServicesList.txt")));
        assert!(ok(fixture!("getOperators_country_187.txt")));
        assert!(ok(r#"{"error":{"field":"x"}}"#));
        assert!(ok(r#"{"status":1,"message":"fine"}"#));

        assert!(matches!(
            error_from_message("BAD_SERVICE"),
            ApiError::BadService
        ));
        assert!(matches!(
            error_from_message("Bad service"),
            ApiError::BadService
        ));
        assert!(matches!(
            error_from_message("bad country"),
            ApiError::BadCountry
        ));
        assert!(matches!(
            error_from_message("No numbers"),
            ApiError::NoNumbers
        ));
        assert!(matches!(error_from_message("Oops"), ApiError::Other(m) if m == "Oops"));
        assert!(matches!(
            error_from_message("Something odd"),
            ApiError::Other(m) if m == "Something odd"
        ));
    }

    #[test]
    fn services() {
        let c = client(FakeTransport::new().push(200, fixture!("getServicesList.txt")));
        let services = c.get_services().unwrap();
        assert_eq!(services.len(), 1055);
        assert!(
            services
                .iter()
                .any(|s| s.code == tg() && s.name == "Telegram")
        );
        assert!(c.transport().requests()[0].ends_with("action=getServicesList"));
    }

    #[test]
    fn countries_have_string_ids_and_no_flags() {
        let c = client(FakeTransport::new().push(200, fixture!("getCountries.txt")));
        let countries = c.get_countries().unwrap();
        // 203 keys in the fixture, one junk `""` row with `"id":null` → skipped.
        assert_eq!(countries.len(), 202);
        assert!(countries.windows(2).all(|w| w[0].key < w[1].key));
        let usa = countries.iter().find(|c| c.id() == Some(187)).unwrap();
        assert_eq!(usa.name_en, "United States");
        assert_eq!(usa.name_ru.as_deref(), Some("США"));
        assert_eq!(usa.name_cn.as_deref(), Some("美国"));
        assert_eq!((usa.visible, usa.retry, usa.rent), (None, None, None));
        assert!(!countries.iter().any(|c| c.name_en == "Faroe Islands"));
    }

    #[test]
    fn top_countries_keep_provider_ranking() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getTopCountriesByService_service_tg.txt"))
                .push(200, "[]"),
        );
        let rows = c.get_top_countries(&tg()).unwrap();
        // 10 countries, 17 partner offers in the fixture.
        assert_eq!(rows.len(), 17);
        assert!(
            rows.iter()
                .all(|r| matches!(r.country, CountryRef::Slug(_)) && r.provider_id.is_some())
        );
        assert!(rows.iter().all(|r| r.retail_price.is_none()));

        // Countries appear in the provider's priority order, not alphabetically.
        let mut slugs: Vec<String> = Vec::new();
        for r in &rows {
            let s = r.country.to_string();
            if slugs.last() != Some(&s) {
                slugs.push(s);
            }
        }
        assert_eq!(
            slugs,
            [
                "united-states",
                "united-states-virtual",
                "colombia",
                "armenia",
                "united-kingdom",
                "canada",
                "brazil",
                "romania",
                "turkey",
                "france",
            ]
        );
        // Partners keep their sales rank, which is not the price order.
        assert_eq!(rows[0].country, CountryRef::Slug("united-states".into()));
        assert_eq!(rows[0].provider_id.as_deref(), Some("3170"));
        assert_eq!(rows[0].price, 0.765);
        assert_eq!(rows[0].count, 1000);
        let usa: Vec<&TopCountry> = rows
            .iter()
            .filter(|r| r.country == CountryRef::Slug("united-states".into()))
            .collect();
        assert_eq!(usa.len(), 3);
        assert_eq!(usa[1].provider_id.as_deref(), Some("3449"));
        assert_eq!(usa[1].price, 0.697);
        assert_eq!(usa[1].count, 159);
        assert_eq!(usa[2].provider_id.as_deref(), Some("2266"));
        assert_eq!(usa[2].price, 1.059);

        let virtual_usa = rows
            .iter()
            .find(|r| r.country == CountryRef::Slug("united-states-virtual".into()))
            .unwrap();
        assert_eq!(virtual_usa.provider_id.as_deref(), Some("2262"));
        assert_eq!(virtual_usa.count, 96636);
        // Turkey has three partners at the same integer price.
        assert_eq!(
            rows.iter()
                .filter(|r| r.country == CountryRef::Slug("turkey".into()) && r.price == 3.0)
                .count(),
            3
        );
        assert_eq!(rows[16].country, CountryRef::Slug("france".into()));
        assert_eq!(rows[16].provider_id.as_deref(), Some("3389"));

        assert!(c.get_top_countries(&tg()).unwrap().is_empty());
        assert!(
            c.transport().requests()[0].ends_with("action=getTopCountriesByService&service=tg")
        );
        assert!(matches!(
            parse_top_countries_by_slug("42"),
            Err(ApiError::Unexpected(_))
        ));
        assert!(parse_top_countries_by_slug("{}").unwrap().is_empty());
        // A country whose partner list is PHP's `[]` contributes no rows but does not fail.
        let rows =
            parse_top_countries_by_slug(r#"{"x":[],"y":{"1":{"price":1,"count":2}}}"#).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].country, CountryRef::Slug("y".into()));
        assert!(parse_top_countries_by_slug(r#"{"x":[1]}"#).is_err());
    }

    #[test]
    fn operators_standard_shape_and_unknown_country() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getOperators_country_187.txt"))
                .push(200, fixture!("getOperators_country_9999.txt")),
        );
        let ops = c.get_operators(Some(&CountryRef::Id(187))).unwrap();
        assert_eq!(ops.len(), 1);
        assert!(ops[&CountryRef::Id(187)].is_empty());
        // `countryOperators` is PHP's `[]` for a country SMSBower does not know → empty map.
        assert!(
            c.get_operators(Some(&CountryRef::Id(9999)))
                .unwrap()
                .is_empty()
        );
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getOperators&country=187"));
        assert!(reqs[1].ends_with("action=getOperators&country=9999"));
    }

    #[test]
    fn bad_key_is_a_401_json_envelope() {
        let c = client(FakeTransport::new().push(401, fixture!("badkey_getBalance.txt")));
        assert!(matches!(c.get_balance(), Err(ApiError::BadKey)));
        // The same envelope with a 200 would still be recognised by its message.
        let c = client(FakeTransport::new().push(200, fixture!("badkey_getBalance.txt")));
        assert!(matches!(c.get_balance(), Err(ApiError::BadKey)));
        // The classic token stays supported.
        let c = client(FakeTransport::new().push(200, "BAD_KEY"));
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
        assert!(reqs[0].ends_with("action=getStatus&id=1"));
        assert!(reqs[2].ends_with("action=setStatus&id=1&status=8"));
    }

    #[test]
    fn bad_action_tokens() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("nosuchaction.txt"))
                .push(200, fixture!("getNumbersStatus_country_187.txt"))
                .push(200, fixture!("getActiveActivations.txt"))
                .push(200, fixture!("getFullSms_id_1.txt")),
        );
        let country = vec![("country".to_owned(), "187".to_owned())];
        let id = vec![("id".to_owned(), "1".to_owned())];
        for (action, params) in [
            ("nosuchaction", Vec::new()),
            ("getNumbersStatus", country),
            ("getActiveActivations", Vec::new()),
            ("getFullSms", id),
        ] {
            assert!(
                matches!(c.call(action, params), Err(ApiError::BadAction(_))),
                "{action} should be BAD_ACTION"
            );
        }
        let reqs = c.transport().requests();
        assert_eq!(reqs.len(), 4);
        assert!(reqs[0].ends_with("?api_key=KEY&action=nosuchaction"));
        assert!(reqs[1].ends_with("&action=getNumbersStatus&country=187"));
        assert!(reqs[2].ends_with("&action=getActiveActivations"));
        assert!(reqs[3].ends_with("&action=getFullSms&id=1"));
    }

    #[test]
    fn post_is_rejected_with_405_envelope() {
        let c = client(FakeTransport::new().push(405, fixture!("post_getBalance.txt")));
        match c.get_balance() {
            Err(ApiError::Http { status, body }) => {
                assert_eq!(status, 405);
                assert!(body.starts_with("The POST method is not supported"));
                assert!(!body.contains('{'), "message should be unwrapped");
            }
            other => panic!("expected Http 405, got {other:?}"),
        }
    }

    #[test]
    fn other_envelopes_and_tokens() {
        // A 200 envelope with a non-token message.
        let c =
            client(FakeTransport::new().push(200, r#"{"status":0,"message":"Oops","data":[]}"#));
        assert!(matches!(c.get_balance(), Err(ApiError::Other(m)) if m == "Oops"));
        // A 200 envelope whose message is a known token.
        let c = client(FakeTransport::new().push(200, r#"{"status":"0","message":"NO_BALANCE"}"#));
        assert!(matches!(c.get_balance(), Err(ApiError::NoBalance)));
        // A 500 envelope.
        let c = client(
            FakeTransport::new().push(500, r#"{"status":0,"message":"Server Error","data":[]}"#),
        );
        assert!(matches!(
            c.get_balance(),
            Err(ApiError::Http { status: 500, body }) if body == "Server Error"
        ));
        // Success JSON with a string status is data, not an error.
        let c = client(FakeTransport::new().push(200, fixture!("getServicesList.txt")));
        assert!(c.get_services().is_ok());
        // Plain-text tokens still go through the standard mapping.
        let c = client(
            FakeTransport::new()
                .push(200, "EARLY_CANCEL_DENIED")
                .push(200, "BAD_STATUS")
                .push(200, "NO_NUMBERS")
                .push(429, "")
                .push(503, "<html>maintenance</html>"),
        );
        let id = ActivationId::from("1");
        assert!(matches!(c.cancel(&id), Err(ApiError::EarlyCancelDenied)));
        assert!(matches!(c.complete(&id), Err(ApiError::BadStatus)));
        assert!(matches!(
            c.get_number(&NumberRequest::new("tg", 187)),
            Err(ApiError::NoNumbers)
        ));
        assert!(matches!(c.get_balance(), Err(ApiError::RateLimited { .. })));
        assert!(matches!(
            c.get_balance(),
            Err(ApiError::Http { status: 503, .. })
        ));
    }

    #[test]
    fn get_number_v2_url_encoding_with_provider_params() {
        // Synthetic success body in the documented getNumberV2 shape (no purchase was made live).
        let body = r#"{"activationId":"123456","phoneNumber":"12025550123","activationCost":0.765,"countryCode":"187","canGetAnotherSms":true,"activationTime":"2026-08-30 12:00:00","activationOperator":"t-mobile"}"#;
        let c = client(FakeTransport::new().push(200, body).push(
            200,
            fixture!("getNumberV2_service___probe___country_187.txt"),
        ));
        let req = NumberRequest::new("tg", 187)
            .max_price(1.25)
            .min_price(0.5)
            .provider_ids([2263u64, 3170])
            .except_provider_ids(["2262"])
            .phone_exception(["7918", "7900111"])
            .referral("ref 1")
            .user_id("u-1");
        let a = c.get_number(&req).unwrap();
        assert_eq!(a.id.as_str(), "123456");
        assert_eq!(a.phone, "12025550123");
        assert_eq!(a.cost, Some(0.765));
        assert_eq!(a.country, Some(CountryRef::Id(187)));
        assert_eq!(a.can_get_another_sms, Some(true));
        assert_eq!(a.operator.as_deref(), Some("t-mobile"));
        assert_eq!(
            c.transport().requests()[0],
            format!(
                "{ENDPOINT}?api_key=KEY&action=getNumberV2&service=tg&country=187&maxPrice=1.25&minPrice=0.5\
                 &providerIds=2263%2C3170&exceptProviderIds=2262&phoneException=7918%2C7900111&ref=ref%201&userID=u-1"
            )
        );

        // The live probe with an invalid service answers WRONG_SERVICE, not BAD_SERVICE.
        assert_eq!(
            fixture!("getNumberV2_service___probe___country_187.txt"),
            "WRONG_SERVICE"
        );
        assert!(matches!(
            c.get_number(&NumberRequest::new("__probe__", 187)),
            Err(ApiError::BadService)
        ));
        assert!(
            c.transport().requests()[1]
                .ends_with("action=getNumberV2&service=__probe__&country=187")
        );
    }

    #[test]
    fn request_helpers_replace_previous_values() {
        let req = NumberRequest::new("tg", 187)
            .provider_ids([1, 2])
            .provider_ids([3])
            .referral("a")
            .referral("b");
        assert_eq!(
            req.extra,
            vec![
                ("providerIds".to_owned(), "3".to_owned()),
                ("ref".to_owned(), "b".to_owned()),
            ]
        );
        // An empty list means "no filter": the parameter is removed, never sent as `providerIds=`.
        let req = NumberRequest::new("tg", 187).provider_ids(Vec::<u64>::new());
        assert!(req.extra.is_empty());
        let req = NumberRequest::new("tg", 187)
            .provider_ids([1])
            .except_provider_ids([2])
            .phone_exception(["7918"])
            .provider_ids(Vec::<u64>::new())
            .phone_exception(Vec::<&str>::new());
        assert_eq!(
            req.extra,
            vec![("exceptProviderIds".to_owned(), "2".to_owned())]
        );
        let c = client(FakeTransport::new().push(200, "NO_NUMBERS"));
        let _ = c.get_number(&req);
        assert!(
            c.transport().requests()[0]
                .ends_with("action=getNumberV2&service=tg&country=187&exceptProviderIds=2")
        );
    }

    #[test]
    fn prices_v2_histogram() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("getPricesV2_service_tg_country_187.txt"))
                .push(200, "[]"),
        );
        let table = c.get_prices_v2(Some(&tg()), Some(187)).unwrap();
        assert_eq!(table.len(), 1);
        let buckets = &table[&187][&tg()];
        assert_eq!(buckets.len(), 14);
        assert_eq!(
            buckets[0],
            PriceBucket {
                price: 0.683,
                count: 22
            }
        );
        assert_eq!(
            buckets[13],
            PriceBucket {
                price: 3.684,
                count: 3967
            }
        );
        assert!(buckets.windows(2).all(|w| w[0].price < w[1].price));
        // The histogram adds up to the `count` reported by getPrices for the same country/service.
        assert_eq!(buckets.iter().map(|b| b.count).sum::<u64>(), 213697);
        assert!(c.get_prices_v2(None, None).unwrap().is_empty());
        let reqs = c.transport().requests();
        assert!(reqs[0].ends_with("action=getPricesV2&service=tg&country=187"));
        assert!(reqs[1].ends_with("action=getPricesV2"));
        assert!(matches!(
            parse_prices_v2(r#"{"187":{"tg":{"cheap":1}}}"#),
            Err(ApiError::Parse(_))
        ));
        // Only the id-less `""` country row is skipped; any other non-numeric key is an error.
        let t = parse_prices_v2(r#"{"":{"tg":{"0.3":1}},"187":{"tg":{"0.5":2}}}"#).unwrap();
        assert_eq!(t.keys().copied().collect::<Vec<_>>(), vec![187]);
        assert!(matches!(
            parse_prices_v2(r#"{"faroe":{"tg":{"0.3":1}}}"#),
            Err(ApiError::Parse(m)) if m.contains("faroe")
        ));
    }

    #[test]
    fn prices_v3_provider_offers() {
        let c = client(
            FakeTransport::new().push(200, fixture!("getPricesV3_service_tg_country_187.txt")),
        );
        let table = c.get_prices_v3(Some(&tg()), Some(187)).unwrap();
        let offers = &table[&187][&tg()];
        assert_eq!(offers.len(), 18);
        assert_eq!(
            offers[0],
            ProviderOffer {
                provider_id: "3237".into(),
                price: 0.683,
                count: 22
            }
        );
        let p2263 = offers.iter().find(|o| o.provider_id == "2263").unwrap();
        assert_eq!((p2263.price, p2263.count), (2.592, 98));
        // Ties on price are ordered by numeric provider id.
        let at_3684: Vec<&str> = offers
            .iter()
            .filter(|o| o.price == 3.684)
            .map(|o| o.provider_id.as_str())
            .collect();
        assert_eq!(at_3684, vec!["3330", "3370", "3435", "3447"]);
        assert!(offers.windows(2).all(|w| w[0].price <= w[1].price));
        // Partner stock adds up to the getPrices / getPricesV2 total for the same cell.
        assert_eq!(offers.iter().map(|o| o.count).sum::<u64>(), 213697);
        assert!(c.transport().requests()[0].ends_with("action=getPricesV3&service=tg&country=187"));
        assert!(matches!(
            parse_prices_v3(r#"{"187":{"tg":{"1":{"count":1}}}}"#),
            Err(ApiError::Parse(_))
        ));
    }

    #[test]
    fn dialect_errors_short_circuit_extra_actions() {
        let c = client(
            FakeTransport::new()
                .push(401, fixture!("badkey_getBalance.txt"))
                .push(200, "BAD_COUNTRY"),
        );
        assert!(matches!(c.get_prices_v2(None, None), Err(ApiError::BadKey)));
        assert!(matches!(
            c.get_prices_v3(Some(&tg()), Some(9999)),
            Err(ApiError::BadCountry)
        ));
    }
}
