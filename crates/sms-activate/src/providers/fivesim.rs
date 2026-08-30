//! 5SIM — <https://5sim.net/docs>
//!
//! 5SIM is **not** an sms-activate clone: it has its own JSON REST API, so this module implements
//! [`SmsActivateApi`] directly on [`FiveSim`] — there is no [`Dialect`](crate::Dialect) and no
//! [`Client`](crate::Client). Everything below was verified against live responses captured on
//! 2026-08-30 (`fixtures/fivesim/`, PII redacted) and the rendered documentation.
//!
//! # Endpoint and authentication
//!
//! * Base URL `https://5sim.net/v1` ([`BASE_URL`]); every call is a `GET` except the price-limit
//!   endpoints (`POST` / `DELETE /user/max-prices` with a JSON body).
//! * `/user/*` needs `Authorization: Bearer <token>` (the API key is a JWT) plus
//!   `Accept: application/json`. A missing or bad token answers HTTP 401 with an **empty** body.
//! * `/guest/*` needs no key and ignores a bogus bearer (verified live); this client sends only
//!   `Accept: application/json` there, so a [`FiveSim`] built with an empty token can still read
//!   the public catalogue (countries, products, prices).
//! * Error bodies are **plain text** (`order not found`), never JSON — see *Errors*.
//! * A URL segment that does not match 5SIM's route patterns (a product such as `__probe__`,
//!   the documented `/user/sms/inbox/{id}` route) answers HTTP 302 → `/404.html`; a
//!   redirect-following transport such as `ureq` then ends on the HTML 404 page. Both surface as
//!   [`ApiError::Http`] (body cut to [`HTTP_BODY_LIMIT`] bytes at a char boundary). A 2xx answer whose body is
//!   neither JSON nor a known error text (a maintenance page, say) is [`ApiError::Unexpected`],
//!   truncated the same way.
//!
//! # Keys are names, not ids
//!
//! Countries, operators and products are **names** (`england`, `vodafone`, `telegram`); 5SIM has
//! no numeric ids. Every country-taking method accepts only [`CountryRef::Slug`] and answers
//! [`ApiError::BadCountry`] for a [`CountryRef::Id`] *without sending a request*. Take the slugs
//! from [`SmsActivateApi::get_countries`] (`Country::key`). Phone numbers keep the leading `+`
//! (`+44…`) exactly as 5SIM sends them — the sms-activate family omits it.
//!
//! # Capability matrix
//!
//! | capability | value | how |
//! | --- | --- | --- |
//! | `get_number_v2` | ✘ | not applicable: `get_number` always returns the full order (cost, operator, time) |
//! | `numbers_status` | ✔ | `GET /guest/products/{country}/{operator\|any}` → `Qty` per activation product |
//! | `active_activations` | ✔ | `GET /user/orders?category=activation&limit=100…` filtered to `PENDING` / `RECEIVED` |
//! | `operators` | ✔ | operator keys of each country in `GET /guest/countries` |
//! | `prices_v2` / `prices_v3` | ✘ | no equivalent; [`FiveSim::prices_by_operator`] exposes the per-operator detail |
//! | `price_bounds` | ✔ (`maxPrice` only) | `maxPrice` is documented and honoured **only with operator `any`**, so `get_number` refuses `max_price` combined with any other operator ([`ApiError::Validation`]` { field: "maxPrice" }`, no request) rather than let 5SIM silently ignore the cap; `min_price` has no 5SIM counterpart and is ignored |
//! | `provider_filters` | ✘ | operators (`vodafone`, `virtual53`) are the only routing filter — pass one as `NumberRequest::operator` |
//!
//! # Mapping onto the trait
//!
//! | trait method | 5SIM call | notes |
//! | --- | --- | --- |
//! | `get_balance` | `GET /user/profile` | `balance` (USD, JSON number). [`FiveSim::profile`] returns the whole profile |
//! | `get_number` | `GET /user/buy/activation/{country}/{operator}/{product}` | operator defaults to `any`; `?maxPrice=` from `max_price` (only with operator `any`, see above); `extra` pairs (`forwarding`, `number`, `reuse=1`, `voice=1`, `ref`) appended verbatim; `min_price` ignored |
//! | `get_status` | `GET /user/check/{id}` | see *Order statuses* |
//! | `set_status(Cancel)` | `GET /user/cancel/{id}` | → [`StatusAck::Cancel`] |
//! | `set_status(Complete)` | `GET /user/finish/{id}` | → [`StatusAck::Activation`] |
//! | `set_status(Ready)` | — | [`ApiError::Unsupported`]`("setStatus 1")`: 5SIM has no "SMS sent" hint |
//! | `set_status(RequestAnotherCode)` | — | [`ApiError::Unsupported`]`("setStatus 3")`: a `RECEIVED` order keeps collecting further SMS on the same number until it is finished, so there is nothing to request — just poll `get_status` / [`FiveSim::check`] again |
//! | `get_prices(service, country)` | `GET /guest/prices?country=&product=` | both nestings normalised (see below); **no filter at all → `Unsupported("getPrices without filters")`** because the unfiltered answer is ≈ 9 MB |
//! | `get_services` | `GET /guest/products/any/any` | activation products; `name` = `code` = product name |
//! | `get_countries` | `GET /guest/countries` | `key` = slug, `iso` / `prefix` = first key of the `iso` / `prefix` maps, `name_ru` = `text_ru` |
//! | `get_top_countries(service)` | `GET /guest/prices?product=` | one row per country, `count` descending |
//! | `get_numbers_status(country, op)` | `GET /guest/products/{country}/{op\|any}` | `Qty` of every activation product |
//! | `get_active_activations` | `GET /user/orders?category=activation&limit=100&offset=0&order=id&reverse=true` | rows with status `PENDING` / `RECEIVED`; `status` carries the raw 5SIM status |
//! | `get_operators(country)` | `GET /guest/countries` | keys other than `iso` / `prefix` / `text_en` / `text_ru`; unknown slug → `BadCountry` |
//!
//! ## Prices
//!
//! `GET /guest/prices` nests differently depending on the filter:
//! `?country=X&product=Y` and `?country=X` → `{country:{product:{operator:{cost,count,rate…}}}}`,
//! but `?product=Y` → `{product:{country:{operator:{…}}}}`. Both are folded into a [`PriceTable`]
//! keyed `CountryRef::Slug(country)` → `ServiceCode(product)` where `cost` is the **cheapest
//! operator that has stock** (`count > 0`; the cheapest overall when nothing is in stock),
//! `count` is the **sum** over operators and `physical_count` is `None`.
//! [`FiveSim::prices_by_operator`] / [`FiveSim::offers_by_operator`] keep the per-operator detail
//! including the delivery-rate columns.
//!
//! # Order statuses
//!
//! | 5SIM | [`ActivationStatus`] |
//! | --- | --- |
//! | `PENDING` (5SIM: "Preparation") | `WaitCode` |
//! | `RECEIVED` (5SIM: "Waiting of receipt of SMS" — number allocated, collecting SMS) | `Ok { code }` when `sms` is non-empty (see *Which code* below); `WaitCode` otherwise |
//! | `CANCELED` | `Cancelled` |
//! | `BANNED` | `Cancelled` (refunded like a cancel; the number is blacklisted) |
//! | `TIMEOUT` | `Expired` |
//! | `FINISHED` | `Finished { code }` with the code chosen as below, if any |
//!
//! **Which code:** an order keeps every SMS it received and services routinely send a second,
//! code-less message after the code (Telegram, Google), so the reported code is that of the
//! **newest SMS whose `code` is non-empty** ([`Order::latest_coded_sms`]); only when no SMS
//! carries a code does the newest SMS's `text` stand in. The same rule fills `sms_code` /
//! `sms_text` of [`ActiveActivation`].
//!
//! `TIMEOUT` and `FINISHED` are terminal (5SIM completes an order on its own once the code
//! window has elapsed), and [`ActivationStatus::is_final`] treats `Expired` / `Finished` as
//! final accordingly, so a generic `while !status.is_final()` loop terminates on 5SIM orders.
//!
//! Order lifetime per the docs: **maximum waiting time 15 minutes** (`expires` = `created_at` +
//! 15 min), **"timeout no sms" 5 minutes** — an order without SMS goes to `TIMEOUT`; one with an
//! SMS that is never finished is completed automatically. Cancelling refunds; `cancel` / `ban`
//! are refused with `order has sms` once a code has arrived. The rating (96 max) drops by 0.15
//! per timeout and 0.1 per cancel/ban; at 0 purchases are blocked for 24 h.
//!
//! # Errors
//!
//! | HTTP / body | mapped to |
//! | --- | --- |
//! | 401 (empty body) | [`ApiError::BadKey`] |
//! | 429, 503 | [`ApiError::RateLimited`] — 503 is 5SIM's documented answer to the per-IP and per-buy limits (see *Rate limits*); it has no other documented use, so a genuine outage would look like throttling too |
//! | `no free phones` (documented with HTTP 200 on buy, otherwise 400) | [`ApiError::NoNumbers`] |
//! | `not enough user balance` | [`ApiError::NoBalance`] |
//! | `bad country`, `select country`, `country is incorrect` | [`ApiError::BadCountry`] |
//! | `no product`, `bad product`, `product is incorrect` | [`ApiError::BadService`] |
//! | `bad operator`, `select operator` | [`ApiError::Validation`]` { field: "operator" }` |
//! | `order not found` (400 on cancel/finish/ban, 404 on check) | [`ApiError::NoActivation`] |
//! | `order expired`, `order has sms`, `hosting order`, `not enough rating`, `server offline`, `internal error`, `record not found`, `product_name is empty`, `any error` (the docs' placeholder for other `max-prices` failures), `reuse not possible` / `reuse false` / `reuse expired` | [`ApiError::Other`]`(<text>)` |
//! | any other 4xx, 3xx, 5xx | [`ApiError::Http`] |
//!
//! # Rate limits
//!
//! Documented: 100 requests/s per IP (HTTP 503), 100 requests/s per key (HTTP 429), 100 buys/s
//! (503); hitting a limit 5 times within 10 minutes bans the client for 10 minutes. Both 429 and
//! 503 therefore map to [`ApiError::RateLimited`] (`retry_after: None` — 5SIM sends no
//! `Retry-After`) so a caller that backs off on that variant never escalates into the ban. This
//! crate's live tests stay at ≤ 1 request/s regardless.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::api::{SmsActivateApi, encode};
use crate::error::{ApiError, ApiResult};
use crate::protocol::{as_object, value_to_string};
use crate::transport::{HttpRequest, HttpResponse, Method, Transport};
use crate::types::*;

/// `https://5sim.net/v1`
pub const BASE_URL: &str = "https://5sim.net/v1";

/// Longest body kept inside [`ApiError::Http`] (the HTML 404 page is ≈ 42 KB).
pub const HTTP_BODY_LIMIT: usize = 256;

/// Page size used by [`SmsActivateApi::get_active_activations`].
pub const ACTIVE_ORDERS_LIMIT: u64 = 100;

/// What 5SIM implements (see the module docs for the evidence).
pub const CAPABILITIES: Capabilities = Capabilities {
    get_number_v2: false,
    numbers_status: true,
    active_activations: true,
    operators: true,
    prices_v2: false,
    prices_v3: false,
    price_bounds: true,
    provider_filters: false,
};

/// 5SIM client. `T` is the HTTP transport (`ureq` by default).
#[cfg(feature = "ureq")]
pub struct FiveSim<T = crate::transport::UreqTransport> {
    transport: T,
    token: String,
    base_url: String,
}

/// 5SIM client. `T` is the HTTP transport.
#[cfg(not(feature = "ureq"))]
pub struct FiveSim<T> {
    transport: T,
    token: String,
    base_url: String,
}

#[cfg(feature = "ureq")]
impl FiveSim {
    /// Client over the default `ureq` transport.
    pub fn with_api_key(token: impl Into<String>) -> Self {
        Self::new(crate::transport::UreqTransport::new(), token)
    }
}

impl<T: Transport> FiveSim<T> {
    /// Client over a custom transport. `token` may be empty when only `/guest/*` calls are needed.
    pub fn new(transport: T, token: impl Into<String>) -> Self {
        Self {
            transport,
            token: token.into(),
            base_url: BASE_URL.to_owned(),
        }
    }

    /// Replaces the base URL (`https://5sim.net/v1`); a trailing `/` is stripped.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        self.base_url = url.trim_end_matches('/').to_owned();
        self
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // -- HTTP plumbing -------------------------------------------------------

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn send(&self, req: HttpRequest) -> ApiResult<String> {
        let resp = self.transport.request(&req)?;
        classify(&resp)?;
        Ok(resp.body)
    }

    /// `GET /guest/…` — no bearer (verified unnecessary; a bogus one is ignored).
    fn guest_get(&self, path: &str) -> ApiResult<String> {
        self.send(HttpRequest::get(self.url(path)).accept_json())
    }

    /// `GET /user/…` with the bearer token.
    fn user_get(&self, path: &str) -> ApiResult<String> {
        self.send(
            HttpRequest::get(self.url(path))
                .bearer(&self.token)
                .accept_json(),
        )
    }

    /// `POST` / `DELETE /user/…` with a JSON body.
    fn user_json(&self, method: Method, path: &str, body: String) -> ApiResult<String> {
        self.send(
            HttpRequest::new(method, self.url(path))
                .bearer(&self.token)
                .accept_json()
                .json_body(body),
        )
    }

    /// `GET /user/{action}/{id}` — `check`, `finish`, `cancel`, `ban`.
    fn order_action(&self, action: &str, id: &ActivationId) -> ApiResult<Order> {
        let body = self.user_get(&format!("/user/{action}/{}", encode(id.as_str())))?;
        parse_json(&body)
    }

    /// Raw `GET /guest/prices?…` answer: outer key → inner key → operator → offer.
    fn raw_prices(&self, query: &str) -> ApiResult<NestedOffers> {
        parse_json(&self.guest_get(&format!("/guest/prices{query}"))?)
    }

    // -- typed extras ----------------------------------------------------------

    /// `GET /user/profile` — balance, rating, defaults and the undocumented order counters.
    pub fn profile(&self) -> ApiResult<Profile> {
        parse_json(&self.user_get("/user/profile")?)
    }

    /// `GET /user/check/{id}` — the full order including every SMS received so far.
    pub fn check(&self, id: &ActivationId) -> ApiResult<Order> {
        self.order_action("check", id)
    }

    /// `GET /user/finish/{id}` — mark the order used (status `FINISHED`); same as
    /// [`SmsActivateApi::complete`] but returns the order.
    pub fn finish(&self, id: &ActivationId) -> ApiResult<Order> {
        self.order_action("finish", id)
    }

    /// `GET /user/cancel/{id}` — cancel and refund (status `CANCELED`); refused with
    /// `order has sms` once a code has arrived. Same as [`SmsActivateApi::cancel`] but returns the order.
    pub fn cancel_order(&self, id: &ActivationId) -> ApiResult<Order> {
        self.order_action("cancel", id)
    }

    /// `GET /user/ban/{id}` — report the number as already used (status `BANNED`, refunded,
    /// number blacklisted for you). Refused with `order has sms` once a code has arrived.
    pub fn ban(&self, id: &ActivationId) -> ApiResult<Order> {
        self.order_action("ban", id)
    }

    /// `GET /user/reuse/{product}/{number}` — buy the same number again for `product` (only
    /// orders bought with `reuse=1`). `number` is sent without the leading `+` that 5SIM itself
    /// puts on `Order::phone`, so the value from an earlier order can be passed straight through.
    pub fn reuse(&self, product: &str, number: &str) -> ApiResult<Order> {
        let digits = number.trim().trim_start_matches('+');
        parse_json(&self.user_get(&format!(
            "/user/reuse/{}/{}",
            encode(product),
            encode(digits)
        ))?)
    }

    /// `GET /user/orders?category=…&limit=…&offset=…&order=…&reverse=…` — order history.
    pub fn orders(&self, category: OrderCategory, page: &Page) -> ApiResult<OrdersPage> {
        parse_json(&self.user_get(&format!(
            "/user/orders?category={}&{}",
            category.as_str(),
            page.query()
        ))?)
    }

    /// `GET /user/payments?limit=…&offset=…&order=…&reverse=…` — balance history.
    pub fn payments(&self, page: &Page) -> ApiResult<PaymentsPage> {
        parse_json(&self.user_get(&format!("/user/payments?{}", page.query()))?)
    }

    /// `GET /user/sms/inbox/{id}` — SMS list of a **rented** (hosting) number. Documented, but the
    /// route answered HTTP 302 → `/404.html` for every id tried on 2026-08-30, i.e. [`ApiError::Http`].
    pub fn sms_inbox(&self, id: &ActivationId) -> ApiResult<SmsInbox> {
        parse_json(&self.user_get(&format!("/user/sms/inbox/{}", encode(id.as_str())))?)
    }

    /// `GET /user/max-prices` — per-product purchase price limits (`[]` when none are set).
    pub fn max_prices(&self) -> ApiResult<Vec<MaxPrice>> {
        parse_json(&self.user_get("/user/max-prices")?)
    }

    /// `POST /user/max-prices {"product_name","price"}` — create or update a price limit.
    /// Not exercised live; the request shape follows the docs.
    pub fn set_max_price(&self, product: &str, price: f64) -> ApiResult<()> {
        let body = serde_json::json!({ "product_name": product, "price": price }).to_string();
        self.user_json(Method::Post, "/user/max-prices", body)?;
        Ok(())
    }

    /// `DELETE /user/max-prices {"product_name"}` — remove a price limit. Not exercised live.
    pub fn delete_max_price(&self, product: &str) -> ApiResult<()> {
        let body = serde_json::json!({ "product_name": product }).to_string();
        self.user_json(Method::Delete, "/user/max-prices", body)?;
        Ok(())
    }

    /// `GET /guest/products/{country}/{operator}` — every product with its category, stock and
    /// price. `any` is accepted for both.
    pub fn products(
        &self,
        country: &str,
        operator: &str,
    ) -> ApiResult<BTreeMap<String, ProductInfo>> {
        parse_json(&self.guest_get(&format!(
            "/guest/products/{}/{}",
            encode(country),
            encode(operator)
        ))?)
    }

    /// `GET /guest/countries` — countries with their ISO codes, prefixes and operators, keyed by slug.
    pub fn countries(&self) -> ApiResult<BTreeMap<String, CountryInfo>> {
        parse_countries(&self.guest_get("/guest/countries")?)
    }

    /// `GET /guest/prices?country=…&product=…` — operator → offer (cost, stock, delivery rates)
    /// for one product in one country; empty when the product is not sold there.
    pub fn prices_by_operator(
        &self,
        country: &str,
        product: &str,
    ) -> ApiResult<BTreeMap<String, OperatorOffer>> {
        let mut raw = self.raw_prices(&format!(
            "?country={}&product={}",
            encode(country),
            encode(product)
        ))?;
        Ok(raw
            .remove(country)
            .and_then(|mut products| products.remove(product))
            .unwrap_or_default())
    }

    /// `GET /guest/prices?product=…` — country → operator → offer for one product everywhere.
    pub fn offers_by_operator(
        &self,
        product: &str,
    ) -> ApiResult<BTreeMap<String, BTreeMap<String, OperatorOffer>>> {
        let mut raw = self.raw_prices(&format!("?product={}", encode(product)))?;
        Ok(raw.remove(product).unwrap_or_default())
    }

    /// `GET /guest/prices?country=…` — product → operator → offer for everything sold in one
    /// country (≈ 600 KB for a large country).
    pub fn offers_in_country(
        &self,
        country: &str,
    ) -> ApiResult<BTreeMap<String, BTreeMap<String, OperatorOffer>>> {
        let mut raw = self.raw_prices(&format!("?country={}", encode(country)))?;
        Ok(raw.remove(country).unwrap_or_default())
    }
}

impl<T: Transport> SmsActivateApi for FiveSim<T> {
    fn provider(&self) -> &'static str {
        "5SIM"
    }

    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn get_balance(&self) -> ApiResult<f64> {
        Ok(self.profile()?.balance)
    }

    /// `GET /user/buy/activation/{country}/{operator}/{product}`.
    ///
    /// # Errors
    ///
    /// * [`ApiError::BadCountry`] (no request) for a [`CountryRef::Id`] — 5SIM keys are names.
    /// * [`ApiError::Validation`]` { field: "maxPrice" }` (no request) when `max_price` is set
    ///   together with an operator other than `any`: 5SIM documents that `maxPrice` "shall work
    ///   only if the operator value is set as any", so sending it would silently buy at whatever
    ///   the operator charges. Drop the operator or the cap.
    /// * `min_price` is ignored (no 5SIM counterpart).
    fn get_number(&self, request: &NumberRequest) -> ApiResult<Activation> {
        let country = slug(&request.country)?;
        let operator = request.operator.as_deref().unwrap_or("any");
        if request.max_price.is_some() && operator != "any" {
            return Err(ApiError::Validation {
                field: "maxPrice".to_owned(),
                message: format!(
                    "5SIM honours maxPrice only with operator `any`, not `{operator}`"
                ),
            });
        }
        let mut path = format!(
            "/user/buy/activation/{}/{}/{}",
            encode(country),
            encode(operator),
            encode(request.service.as_str())
        );
        let mut query: Vec<(String, String)> = Vec::new();
        if let Some(p) = request.max_price {
            query.push(("maxPrice".to_owned(), fmt_price(p)));
        }
        query.extend(request.extra.iter().cloned());
        append_query(&mut path, &query);
        let order: Order = parse_json(&self.user_get(&path)?)?;
        Ok(order.into_activation(country))
    }

    fn get_status(&self, id: &ActivationId) -> ApiResult<ActivationStatus> {
        self.check(id)?.activation_status()
    }

    fn set_status(&self, id: &ActivationId, action: StatusAction) -> ApiResult<StatusAck> {
        match action {
            StatusAction::Cancel => {
                self.cancel_order(id)?;
                Ok(StatusAck::Cancel)
            }
            StatusAction::Complete => {
                self.finish(id)?;
                Ok(StatusAck::Activation)
            }
            StatusAction::Ready => Err(ApiError::Unsupported("setStatus 1")),
            StatusAction::RequestAnotherCode => Err(ApiError::Unsupported("setStatus 3")),
        }
    }

    fn get_prices(
        &self,
        service: Option<&ServiceCode>,
        country: Option<&CountryRef>,
    ) -> ApiResult<PriceTable> {
        let country = country.map(slug).transpose()?;
        let product = service.map(ServiceCode::as_str);
        let (query, swapped) = match (country, product) {
            (None, None) => return Err(ApiError::Unsupported("getPrices without filters")),
            (Some(c), Some(p)) => (
                format!("?country={}&product={}", encode(c), encode(p)),
                false,
            ),
            (Some(c), None) => (format!("?country={}", encode(c)), false),
            (None, Some(p)) => (format!("?product={}", encode(p)), true),
        };
        let raw = self.raw_prices(&query)?;
        let mut table = PriceTable::new();
        for (outer, inner_map) in raw {
            for (inner, offers) in inner_map {
                let (country, product) = if swapped {
                    (inner, outer.clone())
                } else {
                    (outer.clone(), inner)
                };
                table
                    .entry(CountryRef::Slug(country))
                    .or_default()
                    .insert(ServiceCode(product), aggregate(&offers));
            }
        }
        Ok(table)
    }

    fn get_services(&self) -> ApiResult<Vec<Service>> {
        Ok(self
            .products("any", "any")?
            .into_iter()
            .filter(|(_, p)| p.category == ProductCategory::Activation)
            .map(|(name, _)| Service {
                code: ServiceCode(name.clone()),
                name,
            })
            .collect())
    }

    fn get_countries(&self) -> ApiResult<Vec<Country>> {
        Ok(self
            .countries()?
            .into_iter()
            .map(|(slug, info)| Country {
                key: CountryRef::Slug(slug),
                name_en: info.text_en,
                name_ru: Some(info.text_ru).filter(|s| !s.is_empty()),
                name_cn: None,
                iso: info.iso.into_iter().next(),
                prefix: info.prefix.into_iter().next(),
                visible: None,
                retry: None,
                rent: None,
            })
            .collect())
    }

    fn get_top_countries(&self, service: &ServiceCode) -> ApiResult<Vec<TopCountry>> {
        let mut rows: Vec<TopCountry> = self
            .offers_by_operator(service.as_str())?
            .into_iter()
            .map(|(country, offers)| {
                let price = aggregate(&offers);
                TopCountry {
                    country: CountryRef::Slug(country),
                    price: price.cost,
                    retail_price: None,
                    count: price.count,
                    provider_id: None,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.country.cmp(&b.country))
        });
        Ok(rows)
    }

    fn get_numbers_status(
        &self,
        country: &CountryRef,
        operator: Option<&str>,
    ) -> ApiResult<BTreeMap<ServiceCode, u64>> {
        let country = slug(country)?;
        Ok(self
            .products(country, operator.unwrap_or("any"))?
            .into_iter()
            .filter(|(_, p)| p.category == ProductCategory::Activation)
            .map(|(name, p)| (ServiceCode(name), p.qty))
            .collect())
    }

    fn get_active_activations(&self) -> ApiResult<Vec<ActiveActivation>> {
        let page = Page::new(ACTIVE_ORDERS_LIMIT, 0);
        Ok(self
            .orders(OrderCategory::Activation, &page)?
            .data
            .into_iter()
            .filter(|o| o.status.is_active())
            .map(Order::into_active_activation)
            .collect())
    }

    fn get_operators(
        &self,
        country: Option<&CountryRef>,
    ) -> ApiResult<BTreeMap<CountryRef, Vec<String>>> {
        let wanted = country.map(slug).transpose()?;
        let mut all = self.countries()?;
        let selected: Vec<(String, CountryInfo)> = match wanted {
            Some(name) => {
                let info = all.remove(name).ok_or(ApiError::BadCountry)?;
                vec![(name.to_owned(), info)]
            }
            None => all.into_iter().collect(),
        };
        Ok(selected
            .into_iter()
            .map(|(name, info)| {
                (
                    CountryRef::Slug(name),
                    info.operators.into_keys().collect::<Vec<_>>(),
                )
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Response classification

/// Turns a raw 5SIM response into `Ok(())` (JSON data follows) or the mapped error. Public so a
/// custom transport wrapper can reuse the mapping.
pub fn classify(resp: &HttpResponse) -> ApiResult<()> {
    let body = resp.body.trim();
    match resp.status {
        401 => Err(ApiError::BadKey),
        // 429 = per-key limit, 503 = per-IP / per-buy limit (documented; no other 503 use).
        429 | 503 => Err(ApiError::RateLimited { retry_after: None }),
        200..=299 => {
            if body.is_empty() || body.starts_with('{') || body.starts_with('[') {
                Ok(())
            } else {
                // The docs list `no free phones` under HTTP 200 for the buy endpoint.
                Err(error_from_text(body)
                    .unwrap_or_else(|| ApiError::Unexpected(truncate_body(body).to_owned())))
            }
        }
        400..=499 => Err(error_from_text(body).unwrap_or_else(|| http_error(resp.status, body))),
        _ => Err(http_error(resp.status, body)),
    }
}

/// Maps 5SIM's plain-text error strings (case-insensitive, trimmed); `None` for unknown text.
pub fn error_from_text(text: &str) -> Option<ApiError> {
    let t = text.trim().to_ascii_lowercase();
    Some(match t.as_str() {
        "no free phones" => ApiError::NoNumbers,
        "not enough user balance" => ApiError::NoBalance,
        "bad country" | "select country" | "country is incorrect" => ApiError::BadCountry,
        "no product" | "bad product" | "product is incorrect" => ApiError::BadService,
        "bad operator" | "select operator" => ApiError::Validation {
            field: "operator".to_owned(),
            message: t.clone(),
        },
        "order not found" => ApiError::NoActivation,
        "order expired"
        | "order has sms"
        | "hosting order"
        | "not enough rating"
        | "server offline"
        | "internal error"
        | "record not found"
        | "product_name is empty"
        | "any error" => ApiError::Other(t.clone()),
        other if other.starts_with("reuse ") => ApiError::Other(t.clone()),
        _ => return None,
    })
}

/// The first [`HTTP_BODY_LIMIT`] bytes of `body`, cut at a char boundary.
fn truncate_body(body: &str) -> &str {
    let mut end = body.len().min(HTTP_BODY_LIMIT);
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    &body[..end]
}

fn http_error(status: u16, body: &str) -> ApiError {
    ApiError::Http {
        status,
        body: truncate_body(body).to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Provider-only types

/// `GET /user/profile`. Fields beyond the documented ones (`did_order`, `is_totp`, `last_order`,
/// `last_top_idx`, `last_top_orders` — a *string*, `total_active_orders`) are undocumented and
/// therefore optional.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Profile {
    pub id: u64,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub default_forwarding_number: Option<String>,
    /// USD.
    pub balance: f64,
    #[serde(default)]
    pub rating: f64,
    #[serde(default)]
    pub default_country: DefaultCountry,
    #[serde(default)]
    pub default_operator: DefaultOperator,
    #[serde(default)]
    pub frozen_balance: f64,
    #[serde(default)]
    pub total_active_orders: Option<u64>,
    /// `country:product:operator:qty:price` of the most recent order.
    #[serde(default)]
    pub last_order: Option<String>,
    #[serde(default)]
    pub last_top_idx: Option<u64>,
    #[serde(default)]
    pub last_top_orders: Option<String>,
    #[serde(default)]
    pub did_order: Option<bool>,
    #[serde(default)]
    pub is_totp: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct DefaultCountry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub iso: String,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct DefaultOperator {
    #[serde(default)]
    pub name: String,
}

/// 5SIM order status. Unknown strings land in `Other` so a new status cannot break parsing.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum OrderStatus {
    /// `PENDING` — 5SIM: "Preparation". No SMS yet.
    Pending,
    /// `RECEIVED` — 5SIM: "Waiting of receipt of SMS". The number is allocated and collecting
    /// SMS; `sms` may still be empty, and further SMS keep arriving until the order is finished.
    Received,
    Canceled,
    /// No SMS within the allowed time (terminal).
    Timeout,
    /// Completed by the client or automatically after the code window (terminal).
    Finished,
    /// Reported as already used; refunded and blacklisted.
    Banned,
    Other(String),
}

impl From<String> for OrderStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "PENDING" => OrderStatus::Pending,
            "RECEIVED" => OrderStatus::Received,
            "CANCELED" => OrderStatus::Canceled,
            "TIMEOUT" => OrderStatus::Timeout,
            "FINISHED" => OrderStatus::Finished,
            "BANNED" => OrderStatus::Banned,
            _ => OrderStatus::Other(s),
        }
    }
}

impl OrderStatus {
    /// The 5SIM spelling (`PENDING` …).
    pub fn as_str(&self) -> &str {
        match self {
            OrderStatus::Pending => "PENDING",
            OrderStatus::Received => "RECEIVED",
            OrderStatus::Canceled => "CANCELED",
            OrderStatus::Timeout => "TIMEOUT",
            OrderStatus::Finished => "FINISHED",
            OrderStatus::Banned => "BANNED",
            OrderStatus::Other(s) => s,
        }
    }

    /// `PENDING` or `RECEIVED` — the order can still receive SMS.
    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::Pending | OrderStatus::Received)
    }
}

/// One SMS on an order (`Order::sms`). Hosting orders also carry an `id`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Sms {
    #[serde(default)]
    pub id: Option<u64>,
    /// When 5SIM stored it (RFC 3339).
    #[serde(default)]
    pub created_at: String,
    /// When the SMS was received.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub text: String,
    /// The extracted code; may be empty when 5SIM could not find one in `text`.
    #[serde(default)]
    pub code: String,
}

impl Sms {
    /// `code`, or `text` when no code was extracted; `None` when both are empty.
    pub fn code_or_text(&self) -> Option<&str> {
        [&self.code, &self.text]
            .into_iter()
            .map(|s| s.trim())
            .find(|s| !s.is_empty())
    }
}

/// An order as returned by buy / check / finish / cancel / ban / reuse and listed by
/// [`FiveSim::orders`]. `operator`, `forwarding*` and `country` are absent on some endpoints.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Order {
    pub id: u64,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub product: String,
    #[serde(default)]
    pub price: f64,
    pub status: OrderStatus,
    #[serde(default)]
    pub expires: String,
    /// `null` on a fresh order; every SMS received so far otherwise.
    #[serde(default, deserialize_with = "null_to_default")]
    pub sms: Vec<Sms>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub forwarding: Option<bool>,
    #[serde(default)]
    pub forwarding_number: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
}

impl Order {
    pub fn activation_id(&self) -> ActivationId {
        ActivationId::from(self.id)
    }

    /// The most recently created SMS (later position wins on equal timestamps).
    pub fn latest_sms(&self) -> Option<&Sms> {
        self.sms
            .iter()
            .enumerate()
            .max_by(|(ia, a), (ib, b)| a.created_at.cmp(&b.created_at).then(ia.cmp(ib)))
            .map(|(_, s)| s)
    }

    /// The SMS whose code should be reported: the most recently created SMS with a non-empty
    /// `code`; when no SMS carries a code, the most recent one with a non-empty `text`
    /// (a later code-less "welcome" message must not hide an earlier real code).
    pub fn latest_coded_sms(&self) -> Option<&Sms> {
        let newest = |keep: fn(&Sms) -> bool| {
            self.sms
                .iter()
                .enumerate()
                .filter(|(_, s)| keep(s))
                .max_by(|(ia, a), (ib, b)| a.created_at.cmp(&b.created_at).then(ia.cmp(ib)))
                .map(|(_, s)| s)
        };
        newest(|s| !s.code.trim().is_empty()).or_else(|| newest(|s| !s.text.trim().is_empty()))
    }

    /// Code of [`Order::latest_coded_sms`] (its text when no SMS has a code).
    pub fn latest_code(&self) -> Option<String> {
        self.latest_coded_sms()
            .and_then(Sms::code_or_text)
            .map(str::to_owned)
    }

    /// The trait-level status (see the module docs for the table).
    pub fn activation_status(&self) -> ApiResult<ActivationStatus> {
        Ok(match &self.status {
            OrderStatus::Pending => ActivationStatus::WaitCode,
            OrderStatus::Received => match self.latest_code() {
                Some(code) => ActivationStatus::Ok { code },
                None => ActivationStatus::WaitCode,
            },
            OrderStatus::Canceled | OrderStatus::Banned => ActivationStatus::Cancelled,
            OrderStatus::Timeout => ActivationStatus::Expired,
            OrderStatus::Finished => ActivationStatus::Finished {
                code: self.latest_code(),
            },
            OrderStatus::Other(s) => {
                return Err(ApiError::Unexpected(format!(
                    "unknown 5SIM order status `{s}`"
                )));
            }
        })
    }

    fn into_activation(self, requested_country: &str) -> Activation {
        let country = self
            .country
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| requested_country.to_owned());
        Activation {
            id: ActivationId::from(self.id),
            phone: self.phone,
            cost: Some(self.price),
            country: Some(CountryRef::Slug(country)),
            can_get_another_sms: Some(true),
            activation_time: Some(self.created_at),
            operator: self.operator,
        }
    }

    fn into_active_activation(self) -> ActiveActivation {
        let latest = self.latest_coded_sms().cloned();
        ActiveActivation {
            id: ActivationId::from(self.id),
            service: Some(ServiceCode(self.product)),
            phone: Some(self.phone),
            cost: Some(self.price),
            status: Some(self.status.as_str().to_owned()),
            sms_code: latest
                .as_ref()
                .map(|s| s.code.trim().to_owned())
                .filter(|c| !c.is_empty()),
            sms_text: latest
                .map(|s| s.text.trim().to_owned())
                .filter(|t| !t.is_empty()),
            activation_time: Some(self.created_at),
            country: self.country.filter(|c| !c.is_empty()).map(CountryRef::Slug),
            can_get_another_sms: Some(true),
        }
    }
}

/// `category` of `GET /user/orders`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderCategory {
    Activation,
    Hosting,
}

impl OrderCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            OrderCategory::Activation => "activation",
            OrderCategory::Hosting => "hosting",
        }
    }
}

/// Pagination of the history endpoints: `limit`, `offset`, sort field (`order`, default `id`)
/// and `reverse` (default `true` = newest first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub limit: u64,
    pub offset: u64,
    pub order: String,
    pub reverse: bool,
}

impl Default for Page {
    fn default() -> Self {
        Self::new(15, 0)
    }
}

impl Page {
    pub fn new(limit: u64, offset: u64) -> Self {
        Self {
            limit,
            offset,
            order: "id".to_owned(),
            reverse: true,
        }
    }

    pub fn order(mut self, field: impl Into<String>) -> Self {
        self.order = field.into();
        self
    }

    pub fn reverse(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    fn query(&self) -> String {
        format!(
            "limit={}&offset={}&order={}&reverse={}",
            self.limit,
            self.offset,
            encode(&self.order),
            self.reverse
        )
    }
}

/// `GET /user/orders`. `ProductNames` / `Statuses` (documented as lists, only ever seen empty)
/// are not exposed. `Total` was `999999` live — treat it as "unknown" rather than a count.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OrdersPage {
    #[serde(rename = "Data", default, deserialize_with = "null_to_default")]
    pub data: Vec<Order>,
    #[serde(rename = "Total", default)]
    pub total: u64,
}

/// One row of `GET /user/payments`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Payment {
    #[serde(rename = "ID")]
    pub id: u64,
    /// `charge` (top-up) or `buy`.
    #[serde(rename = "TypeName", default)]
    pub type_name: String,
    /// Payment system for charges, product name for purchases.
    #[serde(rename = "ProviderName", default)]
    pub provider_name: String,
    /// Negative for purchases.
    #[serde(rename = "Amount", default)]
    pub amount: f64,
    /// Balance after this entry.
    #[serde(rename = "Balance", default)]
    pub balance: f64,
    #[serde(rename = "CreatedAt", default)]
    pub created_at: String,
}

/// `{"Name":"…"}` rows of `PaymentTypes` / `PaymentProviders`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Named {
    #[serde(rename = "Name")]
    pub name: String,
}

/// `GET /user/payments`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PaymentsPage {
    #[serde(rename = "Data", default, deserialize_with = "null_to_default")]
    pub data: Vec<Payment>,
    #[serde(rename = "PaymentTypes", default, deserialize_with = "null_to_default")]
    pub payment_types: Vec<Named>,
    #[serde(
        rename = "PaymentProviders",
        default,
        deserialize_with = "null_to_default"
    )]
    pub payment_providers: Vec<Named>,
    #[serde(rename = "Total", default)]
    pub total: u64,
}

/// One row of `GET /user/max-prices`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct MaxPrice {
    pub id: u64,
    pub product: String,
    pub price: f64,
    /// The docs name this `created_at`; the example (and Go-style casing elsewhere) says `CreatedAt`.
    #[serde(rename = "CreatedAt", alias = "created_at", default)]
    pub created_at: String,
}

/// One row of `GET /user/sms/inbox/{id}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct InboxSms {
    #[serde(rename = "ID")]
    pub id: u64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub is_wave: bool,
    #[serde(default)]
    pub wave_uuid: String,
}

/// `GET /user/sms/inbox/{id}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SmsInbox {
    #[serde(rename = "Data", default, deserialize_with = "null_to_default")]
    pub data: Vec<InboxSms>,
    #[serde(rename = "Total", default)]
    pub total: u64,
}

/// `Category` of a product.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum ProductCategory {
    Activation,
    Hosting,
    Other(String),
}

impl From<String> for ProductCategory {
    fn from(s: String) -> Self {
        match s.as_str() {
            "activation" => ProductCategory::Activation,
            "hosting" => ProductCategory::Hosting,
            _ => ProductCategory::Other(s),
        }
    }
}

/// One product of `GET /guest/products/{country}/{operator}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ProductInfo {
    #[serde(rename = "Category")]
    pub category: ProductCategory,
    /// Numbers available.
    #[serde(rename = "Qty", default)]
    pub qty: u64,
    #[serde(rename = "Price", default)]
    pub price: f64,
}

/// Operator flags inside `GET /guest/countries` (`{"activation":1}` — `hosting` has not been observed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperatorSupport {
    pub activation: bool,
    pub hosting: bool,
}

/// One country of `GET /guest/countries`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CountryInfo {
    /// ISO 3166-1 alpha-2 codes (lowercase); one per country so far.
    pub iso: Vec<String>,
    /// Dialling prefixes such as `+44`; one per country so far.
    pub prefix: Vec<String>,
    pub text_en: String,
    pub text_ru: String,
    /// Operator name → what it sells.
    pub operators: BTreeMap<String, OperatorSupport>,
}

/// One operator's offer in `GET /guest/prices`: price, stock and delivery percentages over the
/// last hour / 3 h / 24 h / 72 h / week / month (omitted by 5SIM when below 20 % or too few orders).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct OperatorOffer {
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub rate: Option<f64>,
    #[serde(default)]
    pub rate1: Option<f64>,
    #[serde(default)]
    pub rate3: Option<f64>,
    #[serde(default)]
    pub rate24: Option<f64>,
    #[serde(default)]
    pub rate72: Option<f64>,
    #[serde(default)]
    pub rate168: Option<f64>,
    #[serde(default)]
    pub rate720: Option<f64>,
}

type NestedOffers = BTreeMap<String, BTreeMap<String, BTreeMap<String, OperatorOffer>>>;

// ---------------------------------------------------------------------------
// Helpers

fn slug(country: &CountryRef) -> ApiResult<&str> {
    match country {
        CountryRef::Slug(s) if !s.trim().is_empty() => Ok(s.as_str()),
        _ => Err(ApiError::BadCountry),
    }
}

fn parse_json<'a, D: Deserialize<'a>>(body: &'a str) -> ApiResult<D> {
    serde_json::from_str(body.trim()).map_err(ApiError::from)
}

fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Cheapest in-stock operator (cheapest overall when nothing is in stock) and the summed stock.
fn aggregate(offers: &BTreeMap<String, OperatorOffer>) -> Price {
    let in_stock = offers
        .values()
        .filter(|o| o.count > 0)
        .map(|o| o.cost)
        .fold(None, min_f64);
    let cost = in_stock
        .or_else(|| offers.values().map(|o| o.cost).fold(None, min_f64))
        .unwrap_or(0.0);
    Price {
        cost,
        count: offers.values().map(|o| o.count).sum(),
        physical_count: None,
    }
}

fn min_f64(acc: Option<f64>, x: f64) -> Option<f64> {
    Some(match acc {
        Some(a) if a <= x => a,
        _ => x,
    })
}

/// Prices are sent with up to 4 decimals and no trailing zeros (`0.25`, `1`, `0.1569`).
fn fmt_price(p: f64) -> String {
    let s = format!("{p:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

fn append_query(path: &mut String, params: &[(String, String)]) {
    for (i, (k, v)) in params.iter().enumerate() {
        path.push(if i == 0 && !path.contains('?') {
            '?'
        } else {
            '&'
        });
        path.push_str(&encode(k));
        path.push('=');
        path.push_str(&encode(v));
    }
}

/// `{"england":{"iso":{"gb":1},"prefix":{"+44":1},"text_en":"England","text_ru":"…","<operator>":{"activation":1},…},…}`
fn parse_countries(body: &str) -> ApiResult<BTreeMap<String, CountryInfo>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let mut out = BTreeMap::new();
    for (name, entry) in as_object(&v, "countries")? {
        let mut info = CountryInfo::default();
        for (key, val) in as_object(entry, "country")? {
            match key.as_str() {
                "iso" => info.iso = object_keys(val),
                "prefix" => info.prefix = object_keys(val),
                "text_en" => info.text_en = value_to_string(Some(val)).unwrap_or_default(),
                "text_ru" => info.text_ru = value_to_string(Some(val)).unwrap_or_default(),
                _ => {
                    if let Some(flags) = val.as_object() {
                        let on = |k: &str| flags.get(k).and_then(Value::as_i64).unwrap_or(0) != 0;
                        info.operators.insert(
                            key.clone(),
                            OperatorSupport {
                                activation: on("activation"),
                                hosting: on("hosting"),
                            },
                        );
                    }
                }
            }
        }
        out.insert(name.clone(), info);
    }
    Ok(out)
}

fn object_keys(v: &Value) -> Vec<String> {
    v.as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../fixtures/fivesim/", $name))
        };
    }

    // Docs examples (https://5sim.net/docs), one per status.
    const ORDER_PENDING: &str = r#"{"id":11631253,"phone":"+447350690992","operator":"vodafone","product":"facebook","price":21,"status":"PENDING","expires":"2018-10-13T08:28:38.809469028Z","sms":null,"created_at":"2018-10-13T08:13:38.809469028Z","forwarding":false,"forwarding_number":"","country":"england"}"#;
    const ORDER_RECEIVED: &str = r#"{"id":11631253,"created_at":"2018-10-13T08:13:38.809469028Z","phone":"+447350690992","product":"facebook","price":21,"status":"RECEIVED","expires":"2018-10-13T08:28:38.809469028Z","sms":[{"created_at":"2018-10-13T08:20:38.809469028Z","date":"2018-10-13T08:19:38Z","sender":"Facebook","text":"Facebook: 09363 - use this code to reclaim your suspended profile.","code":"09363"}],"forwarding":false,"forwarding_number":"","country":"england"}"#;
    const ORDER_RECEIVED_NO_SMS: &str =
        r#"{"id":5,"phone":"+1","product":"facebook","price":1,"status":"RECEIVED","sms":[]}"#;
    const ORDER_RECEIVED_TWO: &str = r#"{"id":6,"phone":"+1","product":"facebook","price":1,"status":"RECEIVED","sms":[{"created_at":"2026-01-01T00:00:01Z","text":"first 1111","code":"1111"},{"created_at":"2026-01-01T00:00:02Z","text":"no code here","code":""}]}"#;
    const ORDER_RECEIVED_NO_CODES: &str = r#"{"id":9,"phone":"+1","product":"facebook","price":1,"status":"RECEIVED","sms":[{"created_at":"2026-01-01T00:00:01Z","text":"first text","code":""},{"created_at":"2026-01-01T00:00:02Z","text":" second text ","code":" "}]}"#;
    const ORDER_CANCELED: &str = r#"{"id":7,"phone":"+1","product":"facebook","price":1,"status":"CANCELED","sms":[],"created_at":"2020-06-28T16:17:43.307041Z"}"#;
    const ORDER_TIMEOUT: &str =
        r#"{"id":8,"phone":"+1","product":"facebook","price":1,"status":"TIMEOUT","sms":[]}"#;
    const ORDER_FINISHED: &str = r#"{"id":11631253,"created_at":"2018-10-13T08:13:38.809469028Z","phone":"+447350690992","product":"facebook","price":21,"status":"FINISHED","expires":"2018-10-13T08:28:38.809469028Z","sms":[{"created_at":"2018-10-13T08:20:38.809469028Z","date":"2018-10-13T08:19:38Z","sender":"Facebook","text":"Facebook: 09363 - use this code to reclaim your suspended profile.","code":"09363"}],"forwarding":false,"forwarding_number":"","country":"england"}"#;
    const ORDER_BANNED: &str = r#"{"id":53533933,"phone":"+447350690992","operator":"vodafone","product":"facebook","price":2,"status":"BANNED","expires":"2020-06-28T16:32:43.307041Z","sms":[],"created_at":"2020-06-28T16:17:43.307041Z","country":"england"}"#;

    fn client(t: FakeTransport) -> FiveSim<FakeTransport> {
        FiveSim::new(t, "KEY")
    }

    fn only_request(c: &FiveSim<FakeTransport>) -> HttpRequest {
        let reqs = c.transport().recorded();
        assert_eq!(reqs.len(), 1, "expected exactly one request: {reqs:?}");
        reqs[0].clone()
    }

    fn assert_user_get(req: &HttpRequest, url: &str) {
        assert_eq!(req.method, Method::Get);
        assert_eq!(req.url, url);
        assert_eq!(req.header_value("Authorization"), Some("Bearer KEY"));
        assert_eq!(req.header_value("Accept"), Some("application/json"));
        assert_eq!(req.body, None);
    }

    fn assert_guest_get(req: &HttpRequest, url: &str) {
        assert_eq!(req.method, Method::Get);
        assert_eq!(req.url, url);
        assert_eq!(
            req.header_value("Authorization"),
            None,
            "guest calls carry no bearer"
        );
        assert_eq!(req.header_value("Accept"), Some("application/json"));
        assert_eq!(req.headers.len(), 1);
    }

    #[test]
    fn identity_capabilities_and_base_url() {
        let c = client(FakeTransport::new());
        assert_eq!(c.provider(), "5SIM");
        assert_eq!(c.base_url(), BASE_URL);
        let caps = c.capabilities();
        assert!(!caps.get_number_v2);
        assert!(caps.numbers_status);
        assert!(caps.active_activations);
        assert!(caps.operators);
        assert!(!caps.prices_v2);
        assert!(!caps.prices_v3);
        assert!(caps.price_bounds);
        assert!(!caps.provider_filters);
        let c = client(FakeTransport::new().push(200, fixture!("user_profile.txt")))
            .with_base_url("http://localhost:8080/v1/");
        c.get_balance().unwrap();
        assert_eq!(
            only_request(&c).url,
            "http://localhost:8080/v1/user/profile"
        );
        let boxed: Box<dyn SmsActivateApi> = Box::new(client(FakeTransport::new()));
        assert_eq!(boxed.provider(), "5SIM");
    }

    #[test]
    fn balance_and_profile() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("user_profile.txt"))
                .push(200, fixture!("user_profile.txt")),
        );
        assert_eq!(c.get_balance().unwrap(), 21.6408);
        let p = c.profile().unwrap();
        assert_eq!(p.rating, 95.866);
        assert_eq!(p.frozen_balance, 0.0);
        assert_eq!(p.total_active_orders, Some(0));
        assert_eq!(p.did_order, Some(true));
        assert_eq!(p.vendor, None);
        assert_eq!(p.default_country.iso, "");
        assert_eq!(
            p.last_order.as_deref(),
            Some("england:google:virtual53:42:0.20")
        );
        for req in c.transport().recorded() {
            assert_user_get(&req, "https://5sim.net/v1/user/profile");
        }
    }

    #[test]
    fn countries_with_iso_and_prefix() {
        let c = client(FakeTransport::new().push(200, fixture!("guest_countries.json")));
        let list = c.get_countries().unwrap();
        assert_guest_get(&only_request(&c), "https://5sim.net/v1/guest/countries");
        assert_eq!(list.len(), 153);
        assert!(list.windows(2).all(|w| w[0].key < w[1].key));
        assert_eq!(list[0].key, CountryRef::Slug("afghanistan".into()));
        let en = list
            .iter()
            .find(|c| c.key.slug() == Some("england"))
            .unwrap();
        assert_eq!(en.name_en, "England");
        assert_eq!(en.name_ru.as_deref(), Some("Великобритания"));
        assert_eq!(en.iso.as_deref(), Some("gb"));
        assert_eq!(en.prefix.as_deref(), Some("+44"));
        assert_eq!(en.name_cn, None);
        assert_eq!(en.id(), None);
        assert!(en.visible.is_none() && en.retry.is_none() && en.rent.is_none());
    }

    #[test]
    fn operators_per_country() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("guest_countries.json"))
                .push(200, fixture!("guest_countries.json"))
                .push(200, fixture!("guest_countries.json")),
        );
        let all = c.get_operators(None).unwrap();
        assert_eq!(all.len(), 153);
        let england = CountryRef::Slug("england".into());
        assert_eq!(
            all[&england],
            vec![
                "virtual2",
                "virtual26",
                "virtual27",
                "virtual34",
                "virtual51",
                "virtual53",
                "virtual58",
                "virtual59",
                "virtual60",
                "virtual63",
                "virtual66",
                "virtual67",
                "virtual8"
            ]
        );
        let one = c.get_operators(Some(&england)).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[&england].len(), 13);
        assert!(matches!(
            c.get_operators(Some(&CountryRef::Slug("narnia".into()))),
            Err(ApiError::BadCountry)
        ));
        assert_eq!(c.transport().recorded().len(), 3);
        // A numeric id never reaches the network.
        assert!(matches!(
            c.get_operators(Some(&CountryRef::Id(187))),
            Err(ApiError::BadCountry)
        ));
        assert_eq!(c.transport().recorded().len(), 3);

        let info = &client(FakeTransport::new().push(200, fixture!("guest_countries.json")))
            .countries()
            .unwrap()["england"];
        assert_eq!(info.iso, vec!["gb"]);
        assert_eq!(info.prefix, vec!["+44"]);
        assert_eq!(
            info.operators["virtual53"],
            OperatorSupport {
                activation: true,
                hosting: false
            }
        );
    }

    #[test]
    fn services_and_products() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("guest_products_england_any.json"))
                .push(200, fixture!("guest_products_england_any.json")),
        );
        let services = c.get_services().unwrap();
        assert_eq!(services.len(), 780);
        assert_eq!(services[0].code.as_str(), "115com");
        assert!(
            services
                .iter()
                .any(|s| s.code.as_str() == "telegram" && s.name == "telegram")
        );
        let products = c.products("england", "any").unwrap();
        assert_eq!(
            products["telegram"],
            ProductInfo {
                category: ProductCategory::Activation,
                qty: 455354,
                price: 0.32
            }
        );
        let reqs = c.transport().recorded();
        assert_guest_get(&reqs[0], "https://5sim.net/v1/guest/products/any/any");
        assert_guest_get(&reqs[1], "https://5sim.net/v1/guest/products/england/any");
        // Hosting products are dropped from the service list.
        let c = client(FakeTransport::new().push(
            200,
            r#"{"amazon":{"Category":"hosting","Qty":1,"Price":80},"facebook":{"Category":"activation","Qty":133,"Price":21}}"#,
        ));
        let services = c.get_services().unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].code.as_str(), "facebook");
    }

    #[test]
    fn prices_country_and_product() {
        let c = client(FakeTransport::new().push(
            200,
            fixture!("guest_prices_country_england_product_telegram.json"),
        ));
        let tg = ServiceCode::from("telegram");
        let england = CountryRef::Slug("england".into());
        let table = c.get_prices(Some(&tg), Some(&england)).unwrap();
        assert_guest_get(
            &only_request(&c),
            "https://5sim.net/v1/guest/prices?country=england&product=telegram",
        );
        assert_eq!(table.len(), 1);
        let price = &table[&england][&tg];
        // Cheapest operator *with stock* is virtual59 (0.8); ee/o2/virtual2… are cheaper but empty.
        assert_eq!(price.cost, 0.8);
        assert_eq!(price.count, 455727);
        assert_eq!(price.physical_count, None);
    }

    #[test]
    fn prices_product_only_uses_swapped_nesting() {
        let c =
            client(FakeTransport::new().push(200, fixture!("guest_prices_product_telegram.json")));
        let tg = ServiceCode::from("telegram");
        let table = c.get_prices(Some(&tg), None).unwrap();
        assert_guest_get(
            &only_request(&c),
            "https://5sim.net/v1/guest/prices?product=telegram",
        );
        assert_eq!(table.len(), 119);
        assert!(table.keys().all(|k| k.slug().is_some()));
        let england = &table[&CountryRef::Slug("england".into())];
        assert_eq!(england.len(), 1);
        assert_eq!(england[&tg].cost, 0.8);
        assert_eq!(england[&tg].count, 451603);
    }

    #[test]
    fn prices_country_only() {
        let c =
            client(FakeTransport::new().push(200, fixture!("guest_prices_country_england.json")));
        let england = CountryRef::Slug("england".into());
        let table = c.get_prices(None, Some(&england)).unwrap();
        assert_guest_get(
            &only_request(&c),
            "https://5sim.net/v1/guest/prices?country=england",
        );
        let row = &table[&england];
        assert_eq!(row.len(), 12);
        assert_eq!(row[&ServiceCode::from("115com")].cost, 0.05);
        assert_eq!(row[&ServiceCode::from("115com")].count, 139386);
        assert_eq!(row[&ServiceCode::from("1xbet")].cost, 0.0641);
        assert_eq!(row[&ServiceCode::from("1xbet")].count, 5297);
    }

    #[test]
    fn prices_aggregation_rule_and_refusals() {
        // Nothing in stock → cheapest overall; count 0.
        let c = client(FakeTransport::new().push(
            200,
            r#"{"x":{"p":{"a":{"cost":2,"count":0},"b":{"cost":1.5,"count":0,"rate":99.99}}}}"#,
        ));
        let table = c
            .get_prices(
                Some(&ServiceCode::from("p")),
                Some(&CountryRef::Slug("x".into())),
            )
            .unwrap();
        let price = &table[&CountryRef::Slug("x".into())][&ServiceCode::from("p")];
        assert_eq!(price.cost, 1.5);
        assert_eq!(price.count, 0);
        // Empty answer → empty table.
        let c = client(FakeTransport::new().push(200, "{}"));
        assert!(
            c.get_prices(Some(&ServiceCode::from("p")), None)
                .unwrap()
                .is_empty()
        );
        // No filter and numeric countries are refused before any request.
        let c = client(FakeTransport::new());
        assert!(matches!(
            c.get_prices(None, None),
            Err(ApiError::Unsupported("getPrices without filters"))
        ));
        assert!(matches!(
            c.get_prices(None, Some(&CountryRef::Id(187))),
            Err(ApiError::BadCountry)
        ));
        assert!(c.transport().recorded().is_empty());
        // 5SIM's own 400 texts.
        let c = client(
            FakeTransport::new()
                .push(
                    400,
                    fixture!("guest_prices_country_england_product___probe__.txt"),
                )
                .push(
                    400,
                    fixture!("guest_prices_country___probe___product_telegram.txt"),
                ),
        );
        let tg = ServiceCode::from("telegram");
        assert!(matches!(
            c.get_prices(Some(&tg), Some(&CountryRef::Slug("england".into()))),
            Err(ApiError::BadService)
        ));
        assert!(matches!(
            c.get_prices(Some(&tg), Some(&CountryRef::Slug("__probe__".into()))),
            Err(ApiError::BadCountry)
        ));
    }

    #[test]
    fn per_operator_price_extras() {
        let c = client(
            FakeTransport::new()
                .push(
                    200,
                    fixture!("guest_prices_country_england_product_telegram.json"),
                )
                .push(200, fixture!("guest_prices_product_telegram.json"))
                .push(200, fixture!("guest_prices_country_england.json")),
        );
        let ops = c.prices_by_operator("england", "telegram").unwrap();
        assert_eq!(ops.len(), 17);
        assert_eq!(ops["virtual66"].cost, 0.9);
        assert_eq!(ops["virtual66"].count, 163074);
        assert_eq!(ops["virtual66"].rate, Some(13.04));
        assert_eq!(ops["virtual66"].rate720, Some(10.42));
        assert_eq!(ops["virtual67"].rate, None);
        let by_country = c.offers_by_operator("telegram").unwrap();
        assert_eq!(by_country.len(), 119);
        assert_eq!(by_country["england"]["virtual34"].count, 288943);
        let in_country = c.offers_in_country("england").unwrap();
        assert_eq!(in_country.len(), 12);
        assert_eq!(in_country["115com"]["virtual51"].cost, 0.05);
        let reqs = c.transport().requests();
        assert_eq!(
            reqs,
            vec![
                "https://5sim.net/v1/guest/prices?country=england&product=telegram",
                "https://5sim.net/v1/guest/prices?product=telegram",
                "https://5sim.net/v1/guest/prices?country=england",
            ]
        );
    }

    #[test]
    fn top_countries_sorted_by_stock() {
        let c =
            client(FakeTransport::new().push(200, fixture!("guest_prices_product_telegram.json")));
        let rows = c.get_top_countries(&ServiceCode::from("telegram")).unwrap();
        assert_guest_get(
            &only_request(&c),
            "https://5sim.net/v1/guest/prices?product=telegram",
        );
        assert_eq!(rows.len(), 119);
        assert!(rows.windows(2).all(|w| w[0].count >= w[1].count));
        assert_eq!(rows[0].country, CountryRef::Slug("laos".into()));
        assert_eq!(rows[0].count, 6969551);
        assert_eq!(rows[0].price, 0.3256);
        assert_eq!(rows[0].retail_price, None);
        assert_eq!(rows[0].provider_id, None);
        let england = rows
            .iter()
            .find(|r| r.country.slug() == Some("england"))
            .unwrap();
        assert_eq!(england.count, 451603);
        assert_eq!(england.price, 0.8);
        // Countries without stock come last, alphabetically.
        let last = rows.last().unwrap();
        assert_eq!(last.count, 0);
        assert_eq!(last.country.slug(), Some("ukraine"));
    }

    #[test]
    fn numbers_status_from_products() {
        let c = client(
            FakeTransport::new()
                .push(200, fixture!("guest_products_england_any.json"))
                .push(200, fixture!("guest_products_england_any.json"))
                .push(400, fixture!("guest_products___probe___any.txt")),
        );
        let england = CountryRef::Slug("england".into());
        let counts = c.get_numbers_status(&england, None).unwrap();
        assert_eq!(counts.len(), 780);
        assert_eq!(counts[&ServiceCode::from("telegram")], 455354);
        c.get_numbers_status(&england, Some("vodafone")).unwrap();
        assert!(matches!(
            c.get_numbers_status(&CountryRef::Slug("__probe__".into()), None),
            Err(ApiError::BadCountry)
        ));
        let reqs = c.transport().recorded();
        assert_guest_get(&reqs[0], "https://5sim.net/v1/guest/products/england/any");
        assert_guest_get(
            &reqs[1],
            "https://5sim.net/v1/guest/products/england/vodafone",
        );
        assert_guest_get(&reqs[2], "https://5sim.net/v1/guest/products/__probe__/any");
        assert!(matches!(
            c.get_numbers_status(&CountryRef::Id(187), None),
            Err(ApiError::BadCountry)
        ));
        assert_eq!(c.transport().recorded().len(), 3);
    }

    #[test]
    fn active_activations_from_orders() {
        let c = client(FakeTransport::new().push(
            200,
            fixture!("user_orders_category_activation_limit_5_offset_0_order_id_reverse_true.txt"),
        ));
        // Every order in the fixture is FINISHED or CANCELED.
        assert!(c.get_active_activations().unwrap().is_empty());
        assert_user_get(
            &only_request(&c),
            "https://5sim.net/v1/user/orders?category=activation&limit=100&offset=0&order=id&reverse=true",
        );

        let body = format!(
            r#"{{"Data":[{ORDER_PENDING},{ORDER_RECEIVED},{ORDER_CANCELED},{ORDER_FINISHED}],"ProductNames":[],"Statuses":[],"Total":4}}"#
        );
        let c = client(FakeTransport::new().push(200, body));
        let rows = c.get_active_activations().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id.as_str(), "11631253");
        assert_eq!(rows[0].service, Some(ServiceCode::from("facebook")));
        assert_eq!(rows[0].phone.as_deref(), Some("+447350690992"));
        assert_eq!(rows[0].cost, Some(21.0));
        assert_eq!(rows[0].status.as_deref(), Some("PENDING"));
        assert_eq!(rows[0].sms_code, None);
        assert_eq!(rows[0].sms_text, None);
        assert_eq!(
            rows[0].activation_time.as_deref(),
            Some("2018-10-13T08:13:38.809469028Z")
        );
        assert_eq!(rows[0].country, Some(CountryRef::Slug("england".into())));
        assert_eq!(rows[0].can_get_another_sms, Some(true));
        assert_eq!(rows[1].status.as_deref(), Some("RECEIVED"));
        assert_eq!(rows[1].sms_code.as_deref(), Some("09363"));
        assert!(
            rows[1]
                .sms_text
                .as_deref()
                .unwrap()
                .starts_with("Facebook:")
        );

        // The code of the newest *coded* SMS is reported (with its own text), a later code-less
        // SMS does not mask it; an empty `country` is dropped rather than becoming `Slug("")`.
        let empty_country =
            ORDER_RECEIVED_TWO.replace(r#""product":"#, r#""country":"","product":"#);
        let body = format!(
            r#"{{"Data":[{empty_country},{ORDER_RECEIVED_NO_CODES}],"ProductNames":[],"Statuses":[],"Total":2}}"#
        );
        let c = client(FakeTransport::new().push(200, body));
        let rows = c.get_active_activations().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].sms_code.as_deref(), Some("1111"));
        assert_eq!(rows[0].sms_text.as_deref(), Some("first 1111"));
        assert_eq!(rows[0].country, None);
        assert_eq!(rows[1].sms_code, None);
        assert_eq!(rows[1].sms_text.as_deref(), Some("second text"));
        assert_eq!(rows[1].country, None);
    }

    #[test]
    fn orders_and_payments_pages() {
        let c = client(
            FakeTransport::new()
                .push(
                    200,
                    fixture!(
                        "user_orders_category_activation_limit_5_offset_0_order_id_reverse_true.txt"
                    ),
                )
                .push(
                    200,
                    fixture!("user_payments_limit_3_offset_0_order_id_reverse_true.txt"),
                )
                .push(200, fixture!("user_max-prices.txt")),
        );
        let page = c
            .orders(OrderCategory::Activation, &Page::new(5, 0))
            .unwrap();
        assert_eq!(page.data.len(), 5);
        assert_eq!(page.total, 999999);
        let first = &page.data[0];
        assert_eq!(first.id, 1081142764);
        assert_eq!(first.status, OrderStatus::Finished);
        assert_eq!(first.operator.as_deref(), Some("virtual53"));
        assert_eq!(first.country.as_deref(), Some("england"));
        assert_eq!(first.price, 0.2);
        assert_eq!(first.sms.len(), 1);
        assert_eq!(first.sms[0].sender, "Google");
        assert_eq!(first.latest_code().as_deref(), Some("459260"));
        assert_eq!(page.data[1].status, OrderStatus::Canceled);
        assert!(page.data[1].sms.is_empty());

        let pay = c.payments(&Page::new(3, 0)).unwrap();
        assert_eq!(pay.data.len(), 3);
        assert_eq!(pay.total, 114);
        assert_eq!(pay.data[0].id, 306145889);
        assert_eq!(pay.data[0].type_name, "buy");
        assert_eq!(pay.data[0].provider_name, "google");
        assert_eq!(pay.data[0].amount, -0.2);
        assert_eq!(pay.data[0].balance, 21.6408);
        assert_eq!(pay.payment_types.len(), 2);
        assert_eq!(pay.payment_providers[0].name, "crypto5");

        assert!(c.max_prices().unwrap().is_empty());

        let reqs = c.transport().recorded();
        assert_user_get(
            &reqs[0],
            "https://5sim.net/v1/user/orders?category=activation&limit=5&offset=0&order=id&reverse=true",
        );
        assert_user_get(
            &reqs[1],
            "https://5sim.net/v1/user/payments?limit=3&offset=0&order=id&reverse=true",
        );
        assert_user_get(&reqs[2], "https://5sim.net/v1/user/max-prices");

        let q = Page::new(10, 20).order("created_at").reverse(false).query();
        assert_eq!(q, "limit=10&offset=20&order=created_at&reverse=false");
        assert_eq!(Page::default(), Page::new(15, 0));
    }

    #[test]
    fn status_for_every_order_status() {
        let c = client(
            FakeTransport::new()
                .push(200, ORDER_PENDING)
                .push(200, ORDER_RECEIVED)
                .push(200, ORDER_RECEIVED_NO_SMS)
                .push(200, ORDER_RECEIVED_TWO)
                .push(200, ORDER_RECEIVED_NO_CODES)
                .push(200, ORDER_CANCELED)
                .push(200, ORDER_TIMEOUT)
                .push(200, ORDER_FINISHED)
                .push(200, ORDER_BANNED)
                .push(200, ORDER_TIMEOUT.replace("TIMEOUT", "SOMETHING_NEW"))
                .push(404, fixture!("user_check_1.txt")),
        );
        let id = ActivationId::from("11631253");
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::WaitCode);
        assert_eq!(
            c.get_status(&id).unwrap(),
            ActivationStatus::Ok {
                code: "09363".into()
            }
        );
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::WaitCode);
        // The newest SMS *with a code* wins; a later code-less SMS must not hide it.
        assert_eq!(
            c.get_status(&id).unwrap(),
            ActivationStatus::Ok {
                code: "1111".into()
            }
        );
        // Only when no SMS carries a code does the newest text stand in (trimmed).
        assert_eq!(
            c.get_status(&id).unwrap(),
            ActivationStatus::Ok {
                code: "second text".into()
            }
        );
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::Cancelled);
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::Expired);
        assert_eq!(
            c.get_status(&id).unwrap(),
            ActivationStatus::Finished {
                code: Some("09363".into())
            }
        );
        assert_eq!(c.get_status(&id).unwrap(), ActivationStatus::Cancelled);
        assert!(matches!(
            c.get_status(&id),
            Err(ApiError::Unexpected(m)) if m.contains("SOMETHING_NEW")
        ));
        assert!(matches!(
            c.get_status(&ActivationId::from("1")),
            Err(ApiError::NoActivation)
        ));
        let reqs = c.transport().recorded();
        assert_eq!(reqs.len(), 11);
        assert_user_get(&reqs[0], "https://5sim.net/v1/user/check/11631253");
        assert_user_get(&reqs[10], "https://5sim.net/v1/user/check/1");
    }

    #[test]
    fn terminal_statuses_are_final() {
        for order in [ORDER_TIMEOUT, ORDER_FINISHED, ORDER_CANCELED, ORDER_BANNED] {
            let status = serde_json::from_str::<Order>(order)
                .unwrap()
                .activation_status()
                .unwrap();
            assert!(status.is_final(), "{status:?} must be final");
        }
        for order in [ORDER_PENDING, ORDER_RECEIVED_NO_SMS] {
            let status = serde_json::from_str::<Order>(order)
                .unwrap()
                .activation_status()
                .unwrap();
            assert!(!status.is_final(), "{status:?} must not be final");
        }
        let received: Order = serde_json::from_str(ORDER_RECEIVED).unwrap();
        assert!(received.activation_status().unwrap().is_final());
    }

    #[test]
    fn latest_coded_sms_prefers_a_code_over_a_newer_text() {
        let two: Order = serde_json::from_str(ORDER_RECEIVED_TWO).unwrap();
        assert_eq!(two.latest_sms().unwrap().text, "no code here");
        assert_eq!(two.latest_coded_sms().unwrap().code, "1111");
        assert_eq!(two.latest_code().as_deref(), Some("1111"));
        let none: Order = serde_json::from_str(ORDER_RECEIVED_NO_CODES).unwrap();
        assert_eq!(none.latest_coded_sms().unwrap().text, " second text ");
        assert_eq!(none.latest_code().as_deref(), Some("second text"));
        let empty: Order = serde_json::from_str(ORDER_RECEIVED_NO_SMS).unwrap();
        assert_eq!(empty.latest_coded_sms(), None);
    }

    #[test]
    fn check_returns_the_full_order() {
        let c = client(FakeTransport::new().push(200, ORDER_RECEIVED));
        let order = c.check(&ActivationId::from("11631253")).unwrap();
        assert_eq!(order.activation_id().as_str(), "11631253");
        assert_eq!(order.status, OrderStatus::Received);
        assert_eq!(order.status.as_str(), "RECEIVED");
        assert!(order.status.is_active());
        assert_eq!(order.operator, None);
        assert_eq!(order.forwarding, Some(false));
        assert_eq!(order.sms[0].date, "2018-10-13T08:19:38Z");
        assert_eq!(order.latest_sms().unwrap().code, "09363");
        let fresh: Order = serde_json::from_str(ORDER_PENDING).unwrap();
        assert!(fresh.sms.is_empty());
        assert_eq!(fresh.latest_code(), None);
        assert_eq!(fresh.operator.as_deref(), Some("vodafone"));
    }

    #[test]
    fn set_status_cancel_finish_and_unsupported() {
        let c = client(
            FakeTransport::new()
                .push(200, ORDER_CANCELED)
                .push(200, ORDER_FINISHED)
                .push(400, fixture!("user_cancel_1.txt"))
                .push(400, fixture!("user_finish_1.txt"))
                .push(400, "order has sms"),
        );
        let id = ActivationId::from("7");
        assert_eq!(c.cancel(&id).unwrap(), StatusAck::Cancel);
        assert_eq!(c.complete(&id).unwrap(), StatusAck::Activation);
        assert!(matches!(
            c.set_status(&ActivationId::from("1"), StatusAction::Cancel),
            Err(ApiError::NoActivation)
        ));
        assert!(matches!(
            c.set_status(&ActivationId::from("1"), StatusAction::Complete),
            Err(ApiError::NoActivation)
        ));
        assert!(matches!(
            c.cancel(&id),
            Err(ApiError::Other(t)) if t == "order has sms"
        ));
        assert!(matches!(
            c.set_status(&id, StatusAction::Ready),
            Err(ApiError::Unsupported("setStatus 1"))
        ));
        assert!(matches!(
            c.request_another_code(&id),
            Err(ApiError::Unsupported("setStatus 3"))
        ));
        let reqs = c.transport().recorded();
        assert_eq!(
            reqs.len(),
            5,
            "unsupported actions must not hit the network"
        );
        assert_user_get(&reqs[0], "https://5sim.net/v1/user/cancel/7");
        assert_user_get(&reqs[1], "https://5sim.net/v1/user/finish/7");
        assert_user_get(&reqs[2], "https://5sim.net/v1/user/cancel/1");
        assert_user_get(&reqs[3], "https://5sim.net/v1/user/finish/1");
    }

    #[test]
    fn ban_reuse_and_inbox_extras() {
        let c = client(
            FakeTransport::new()
                .push(200, ORDER_BANNED)
                .push(400, fixture!("user_ban_1.txt"))
                .push(200, ORDER_PENDING)
                .push(
                    200,
                    r#"{"Data":[{"ID":844928,"created_at":"2017-09-05T15:48:33.763297Z","date":"2017-09-05T15:48:27Z","sender":"+447350690992","text":"12345","code":"","is_wave":false,"wave_uuid":""}],"Total":1}"#,
                )
                .push(302, fixture!("user_sms_inbox_1.txt")),
        );
        assert_eq!(
            c.ban(&ActivationId::from("53533933")).unwrap().status,
            OrderStatus::Banned
        );
        assert!(matches!(
            c.ban(&ActivationId::from("1")),
            Err(ApiError::NoActivation)
        ));
        assert_eq!(
            c.reuse("facebook", "+447350690992").unwrap().status,
            OrderStatus::Pending
        );
        let inbox = c.sms_inbox(&ActivationId::from("844928")).unwrap();
        assert_eq!(inbox.total, 1);
        assert_eq!(inbox.data[0].id, 844928);
        assert_eq!(inbox.data[0].text, "12345");
        assert!(!inbox.data[0].is_wave);
        // The live route redirects to the 404 page.
        assert!(matches!(
            c.sms_inbox(&ActivationId::from("1")),
            Err(ApiError::Http { status: 302, body }) if body.contains("404.html")
        ));
        let reqs = c.transport().recorded();
        assert_user_get(&reqs[0], "https://5sim.net/v1/user/ban/53533933");
        assert_user_get(&reqs[1], "https://5sim.net/v1/user/ban/1");
        assert_user_get(
            &reqs[2],
            "https://5sim.net/v1/user/reuse/facebook/447350690992",
        );
        assert_user_get(&reqs[3], "https://5sim.net/v1/user/sms/inbox/844928");
    }

    #[test]
    fn max_price_post_and_delete_bodies() {
        let c = client(
            FakeTransport::new()
                .push(200, "")
                .push(200, "")
                .push(400, "product_name is empty"),
        );
        c.set_max_price("facebook", 30.0).unwrap();
        c.delete_max_price("facebook").unwrap();
        assert!(matches!(
            c.set_max_price("", 1.0),
            Err(ApiError::Other(t)) if t == "product_name is empty"
        ));
        let reqs = c.transport().recorded();
        assert_eq!(reqs[0].method, Method::Post);
        assert_eq!(reqs[0].url, "https://5sim.net/v1/user/max-prices");
        assert_eq!(reqs[0].header_value("Authorization"), Some("Bearer KEY"));
        assert_eq!(reqs[0].header_value("Accept"), Some("application/json"));
        assert_eq!(
            reqs[0].header_value("Content-Type"),
            Some("application/json")
        );
        let body: Value = serde_json::from_str(reqs[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["product_name"], "facebook");
        assert_eq!(body["price"], 30.0);
        assert_eq!(reqs[1].method, Method::Delete);
        assert_eq!(reqs[1].url, "https://5sim.net/v1/user/max-prices");
        assert_eq!(
            reqs[1].body.as_deref(),
            Some(r#"{"product_name":"facebook"}"#)
        );
    }

    #[test]
    fn get_number_builds_the_buy_url() {
        let c = client(
            FakeTransport::new()
                .push(200, ORDER_PENDING)
                .push(200, ORDER_PENDING)
                .push(200, ORDER_PENDING)
                .push(
                    400,
                    fixture!("user_buy_activation_england_any_zzprobezz.txt"),
                )
                .push(
                    302,
                    fixture!("user_buy_activation_england_any___probe__.txt"),
                ),
        );
        let a = c
            .get_number(&NumberRequest::new("facebook", "england"))
            .unwrap();
        assert_eq!(a.id.as_str(), "11631253");
        assert_eq!(a.phone, "+447350690992");
        assert_eq!(a.cost, Some(21.0));
        assert_eq!(a.country, Some(CountryRef::Slug("england".into())));
        assert_eq!(a.operator.as_deref(), Some("vodafone"));
        assert_eq!(
            a.activation_time.as_deref(),
            Some("2018-10-13T08:13:38.809469028Z")
        );
        assert_eq!(a.can_get_another_sms, Some(true));

        let req = NumberRequest::new("facebook", "england")
            .operator("any")
            .max_price(1.25)
            .min_price(0.5)
            .extra("reuse", "1")
            .extra("ref", "a b");
        c.get_number(&req).unwrap();
        // 5SIM ignores `maxPrice` unless the operator is `any`: refused without a request.
        let capped = NumberRequest::new("facebook", "england")
            .operator("vodafone")
            .max_price(1.25);
        assert!(matches!(
            c.get_number(&capped),
            Err(ApiError::Validation { field, message })
                if field == "maxPrice" && message.contains("vodafone")
        ));
        // Without a cap an explicit operator is sent as-is.
        c.get_number(&NumberRequest::new("facebook", "england").operator("vodafone"))
            .unwrap();
        assert!(matches!(
            c.get_number(&NumberRequest::new("zzprobezz", "england")),
            Err(ApiError::BadService)
        ));
        assert!(matches!(
            c.get_number(&NumberRequest::new("__probe__", "england")),
            Err(ApiError::Http { status: 302, .. })
        ));
        // Numeric country ids are refused without a request.
        assert!(matches!(
            c.get_number(&NumberRequest::new("facebook", 187)),
            Err(ApiError::BadCountry)
        ));
        let reqs = c.transport().recorded();
        assert_eq!(reqs.len(), 5);
        assert_user_get(
            &reqs[0],
            "https://5sim.net/v1/user/buy/activation/england/any/facebook",
        );
        // `minPrice` has no 5SIM counterpart and is dropped; extras are appended verbatim.
        assert_user_get(
            &reqs[1],
            "https://5sim.net/v1/user/buy/activation/england/any/facebook?maxPrice=1.25&reuse=1&ref=a%20b",
        );
        assert_user_get(
            &reqs[2],
            "https://5sim.net/v1/user/buy/activation/england/vodafone/facebook",
        );
        assert_user_get(
            &reqs[3],
            "https://5sim.net/v1/user/buy/activation/england/any/zzprobezz",
        );
        assert_eq!(fmt_price(0.25), "0.25");
        assert_eq!(fmt_price(1.0), "1");
        assert_eq!(fmt_price(0.15689), "0.1569");
    }

    #[test]
    fn get_number_falls_back_to_the_requested_country() {
        // Hosting-style order without `country`.
        let c = client(FakeTransport::new().push(
            200,
            r#"{"id":1,"phone":"+447350690992","product":"facebook","price":1,"status":"PENDING","expires":"1970-12-01T03:00:00.000000Z","sms":[{"id":3027531,"created_at":"1970-12-01T17:23:25.106597Z","date":"1970-12-01T17:23:15Z","sender":"Facebook","text":"Use 415127 as your login code","code":"415127"}],"created_at":"1970-12-01T00:00:00.000000Z"}"#,
        ));
        let a = c
            .get_number(&NumberRequest::new("facebook", "england"))
            .unwrap();
        assert_eq!(a.country, Some(CountryRef::Slug("england".into())));
        assert_eq!(a.operator, None);
    }

    #[test]
    fn every_error_mapping() {
        let resp = |status: u16, body: &str| HttpResponse::new(status, body);
        assert!(matches!(classify(&resp(401, "")), Err(ApiError::BadKey)));
        assert!(matches!(
            classify(&resp(429, "")),
            Err(ApiError::RateLimited { retry_after: None })
        ));
        assert!(classify(&resp(200, "{}")).is_ok());
        assert!(classify(&resp(200, "[]")).is_ok());
        assert!(classify(&resp(200, "")).is_ok());
        assert!(matches!(
            classify(&resp(200, "no free phones")),
            Err(ApiError::NoNumbers)
        ));
        assert!(matches!(
            classify(&resp(200, "something odd")),
            Err(ApiError::Unexpected(t)) if t == "something odd"
        ));
        assert!(matches!(
            classify(&resp(400, "no free phones\n")),
            Err(ApiError::NoNumbers)
        ));
        assert!(matches!(
            classify(&resp(400, "not enough user balance")),
            Err(ApiError::NoBalance)
        ));
        for text in ["bad country", "select country", "country is incorrect"] {
            assert!(matches!(
                classify(&resp(400, text)),
                Err(ApiError::BadCountry)
            ));
        }
        for text in ["no product", "bad product", "product is incorrect"] {
            assert!(matches!(
                classify(&resp(400, text)),
                Err(ApiError::BadService)
            ));
        }
        for text in ["bad operator", "select operator"] {
            assert!(matches!(
                classify(&resp(400, text)),
                Err(ApiError::Validation { field, message }) if field == "operator" && message == text
            ));
        }
        assert!(matches!(
            classify(&resp(400, fixture!("user_cancel_1.txt"))),
            Err(ApiError::NoActivation)
        ));
        assert!(matches!(
            classify(&resp(404, fixture!("user_check_1.txt"))),
            Err(ApiError::NoActivation)
        ));
        for text in [
            "order expired",
            "order has sms",
            "hosting order",
            "not enough rating",
            "server offline",
            "internal error",
            "record not found",
            "product_name is empty",
            "reuse not possible",
            "reuse false",
            "reuse expired",
        ] {
            assert!(matches!(
                classify(&resp(400, text)),
                Err(ApiError::Other(t)) if t == text
            ));
        }
        assert!(matches!(
            classify(&resp(400, "Bad Country")),
            Err(ApiError::BadCountry)
        ));
        assert!(matches!(
            classify(&resp(400, "unknown text")),
            Err(ApiError::Http { status: 400, body }) if body == "unknown text"
        ));
        assert!(matches!(
            classify(&resp(
                302,
                fixture!("user_buy_activation_england_any___probe__.txt")
            )),
            Err(ApiError::Http { status: 302, .. })
        ));
        // 503 is the documented per-IP / per-buy limit answer; other 5xx stay `Http`.
        assert!(matches!(
            classify(&resp(503, "")),
            Err(ApiError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            classify(&resp(500, "internal error")),
            Err(ApiError::Http { status: 500, body }) if body == "internal error"
        ));
        let html = format!("<!DOCTYPE html>{}", "x".repeat(50_000));
        assert!(matches!(
            classify(&resp(404, &html)),
            Err(ApiError::Http { status: 404, body }) if body.len() == HTTP_BODY_LIMIT
        ));
        assert!(matches!(
            classify(&resp(200, &html)),
            Err(ApiError::Unexpected(body)) if body.len() == HTTP_BODY_LIMIT
        ));
        // Truncation never splits a multi-byte character.
        let cyrillic = "я".repeat(HTTP_BODY_LIMIT);
        assert!(matches!(
            classify(&resp(200, &cyrillic)),
            Err(ApiError::Unexpected(body)) if body.chars().count() == HTTP_BODY_LIMIT / 2
        ));
        assert!(error_from_text("").is_none());
        assert!(error_from_text("{}").is_none());
        // Transport failures and unparsable bodies keep their own variants.
        let c = client(
            FakeTransport::new()
                .push_error("dns")
                .push(200, "<html>")
                .push(200, "{\"balance\":\"x\"}"),
        );
        assert!(matches!(c.get_balance(), Err(ApiError::Transport(_))));
        assert!(matches!(c.get_balance(), Err(ApiError::Unexpected(_))));
        assert!(matches!(c.get_balance(), Err(ApiError::Parse(_))));
    }
}
