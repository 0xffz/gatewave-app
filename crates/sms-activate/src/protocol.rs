//! Parsers for the *standard* sms-activate response shapes.
//!
//! Providers that deviate override the relevant [`crate::Dialect`] hook; everything here is
//! the common denominator observed on sms-activate itself, Hero-SMS and SMSBower.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::transport::HttpResponse;
use crate::types::*;

/// Standard classification: non-2xx is an HTTP error, a 2xx whose body is a bare error token is
/// a protocol error, anything else is data for the action-specific parser.
pub fn classify_standard(resp: &HttpResponse) -> ApiResult<()> {
    if resp.status == 429 {
        return Err(ApiError::RateLimited { retry_after: None });
    }
    if !(200..300).contains(&resp.status) {
        return Err(ApiError::Http {
            status: resp.status,
            body: resp.body.trim().to_owned(),
        });
    }
    if let Some(err) = error_from_body(&resp.body) {
        return Err(err);
    }
    Ok(())
}

/// Detects a plain-text error token in a response body (`NO_ACTIVATION`, `BAD_KEY`, …).
pub fn error_from_body(body: &str) -> Option<ApiError> {
    let body = body.trim();
    if body.starts_with('{') || body.starts_with('[') {
        return None;
    }
    ApiError::from_code(body)
}

/// `ACCESS_BALANCE:12.5085`
pub fn parse_balance(body: &str) -> ApiResult<f64> {
    let body = body.trim();
    let value = body
        .strip_prefix("ACCESS_BALANCE:")
        .ok_or_else(|| ApiError::Unexpected(body.to_owned()))?;
    value
        .trim()
        .parse()
        .map_err(|_| ApiError::Unexpected(body.to_owned()))
}

/// `ACCESS_NUMBER:<id>:<phone>`
pub fn parse_access_number(body: &str) -> ApiResult<Activation> {
    let body = body.trim();
    let rest = body
        .strip_prefix("ACCESS_NUMBER:")
        .ok_or_else(|| ApiError::Unexpected(body.to_owned()))?;
    let (id, phone) = rest
        .split_once(':')
        .ok_or_else(|| ApiError::Unexpected(body.to_owned()))?;
    if id.is_empty() || phone.is_empty() {
        return Err(ApiError::Unexpected(body.to_owned()));
    }
    Ok(Activation::new(id.trim(), phone.trim()))
}

/// `getNumberV2` JSON: `{"activationId","phoneNumber","activationCost","countryCode","canGetAnotherSms","activationTime","activationOperator"}`.
/// Falls back to the plain `ACCESS_NUMBER:` form when a provider answers that way.
pub fn parse_activation_v2(body: &str) -> ApiResult<Activation> {
    let trimmed = body.trim();
    if trimmed.starts_with("ACCESS_NUMBER:") {
        return parse_access_number(trimmed);
    }
    let v: Value = serde_json::from_str(trimmed)?;
    let id = value_to_string(v.get("activationId"))
        .ok_or_else(|| ApiError::Unexpected(trimmed.to_owned()))?;
    let phone = value_to_string(v.get("phoneNumber"))
        .ok_or_else(|| ApiError::Unexpected(trimmed.to_owned()))?;
    Ok(Activation {
        id: ActivationId(id),
        phone,
        cost: value_to_f64(v.get("activationCost")),
        country: value_to_country(v.get("countryCode")),
        can_get_another_sms: value_to_bool(v.get("canGetAnotherSms")),
        activation_time: value_to_string(v.get("activationTime")),
        operator: value_to_string(v.get("activationOperator")),
    })
}

/// `STATUS_WAIT_CODE` | `STATUS_WAIT_RETRY:<last>` | `STATUS_WAIT_RESEND` | `STATUS_CANCEL` | `STATUS_OK:<code>`
pub fn parse_status(body: &str) -> ApiResult<ActivationStatus> {
    let body = body.trim();
    let (head, tail) = match body.split_once(':') {
        Some((h, t)) => (h, t.trim()),
        None => (body, ""),
    };
    Ok(match head {
        "STATUS_WAIT_CODE" => ActivationStatus::WaitCode,
        "STATUS_WAIT_RETRY" => ActivationStatus::WaitRetry {
            last_code: tail.to_owned(),
        },
        "STATUS_WAIT_RESEND" => ActivationStatus::WaitResend,
        "STATUS_CANCEL" => ActivationStatus::Cancelled,
        "STATUS_OK" => ActivationStatus::Ok {
            code: tail.to_owned(),
        },
        _ => return Err(ApiError::Unexpected(body.to_owned())),
    })
}

/// `ACCESS_READY` | `ACCESS_RETRY_GET` | `ACCESS_ACTIVATION` | `ACCESS_CANCEL`
pub fn parse_set_status(body: &str) -> ApiResult<StatusAck> {
    Ok(match body.trim() {
        "ACCESS_READY" => StatusAck::Ready,
        "ACCESS_RETRY_GET" => StatusAck::RetryGet,
        "ACCESS_ACTIVATION" => StatusAck::Activation,
        "ACCESS_CANCEL" => StatusAck::Cancel,
        other => return Err(ApiError::Unexpected(other.to_owned())),
    })
}

/// `{"<country>":{"<service>":{"cost":0.45,"count":2369[,"physicalCount":0]}}}` — `{}` when nothing matches.
pub fn parse_prices(body: &str) -> ApiResult<PriceTable> {
    let v: Value = serde_json::from_str(body.trim())?;
    let mut table = PriceTable::new();
    for (country, services) in as_object(&v, "prices")? {
        // SMSBower ships one row keyed `""` (Faroe Islands, no country id); skip it like
        // `parse_countries` does. Non-numeric keys become `CountryRef::Slug`.
        if country.is_empty() {
            continue;
        }
        let country_key = CountryRef::parse(country);
        let mut row = BTreeMap::new();
        for (service, price) in as_object(services, "prices.country")? {
            let cost = value_to_f64(price.get("cost"))
                .ok_or_else(|| ApiError::Parse(format!("missing cost for {country}/{service}")))?;
            let count = value_to_u64(price.get("count")).unwrap_or(0);
            let physical_count = value_to_u64(price.get("physicalCount"));
            row.insert(
                ServiceCode(service.clone()),
                Price {
                    cost,
                    count,
                    physical_count,
                },
            );
        }
        table.insert(country_key, row);
    }
    Ok(table)
}

/// `{"status":"success","services":[{"code":"tg","name":"Telegram"},…]}`
pub fn parse_services(body: &str) -> ApiResult<Vec<Service>> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        services: Vec<Service>,
    }
    let env: Envelope = serde_json::from_str(body.trim())?;
    Ok(env.services)
}

/// `{"<id>":{"id":1,"rus":"…","eng":"Ukraine","chn":"…"[,"visible":1,"retry":1,"rent":1]},…}`
/// or a bare array of the same rows (Tiger SMS). `id` may be a number or a string depending on the provider.
pub fn parse_countries(body: &str) -> ApiResult<Vec<Country>> {
    let v: Value = serde_json::from_str(body.trim())?;
    // Either `{"<id>": {...}}` (sms-activate, Hero-SMS, SMSBower) or `[{...}]` (Tiger SMS).
    let entries: Vec<(String, &Value)> = match &v {
        Value::Object(map) => map.iter().map(|(k, c)| (k.clone(), c)).collect(),
        Value::Array(items) => items.iter().map(|c| (String::new(), c)).collect(),
        _ => return Err(ApiError::Unexpected(body.trim().to_owned())),
    };
    let mut out = Vec::new();
    for (key, c) in entries {
        let key = key.as_str();
        // Providers occasionally ship junk rows (empty key, no id); skip rather than fail the whole list.
        let Some(id) = value_to_u64(c.get("id")).or_else(|| key.parse().ok()) else {
            continue;
        };
        out.push(Country {
            key: CountryRef::Id(id as CountryId),
            name_en: value_to_string(c.get("eng")).unwrap_or_default(),
            name_ru: value_to_string(c.get("rus")),
            name_cn: value_to_string(c.get("chn")),
            iso: None,
            prefix: None,
            visible: value_to_bool(c.get("visible")),
            retry: value_to_bool(c.get("retry")),
            rent: value_to_bool(c.get("rent")),
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}

/// sms-activate / Hero-SMS shape: `{"0":{"country":73,"price":0.4,"retail_price":0.48,"count":1081284},…}`
pub fn parse_top_countries_indexed(body: &str) -> ApiResult<Vec<TopCountry>> {
    let v: Value = serde_json::from_str(body.trim())?;
    top_countries_indexed_from_value(&v)
}

/// [`parse_top_countries_indexed`] over an already-parsed value (for dialects whose answer nests
/// the indexed shape, e.g. Hero-SMS's per-service map, without re-serialising it).
pub fn top_countries_indexed_from_value(v: &Value) -> ApiResult<Vec<TopCountry>> {
    let mut rows: Vec<(u64, TopCountry)> = Vec::new();
    let entries: Vec<(u64, &Value)> = match v {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, x)| (i as u64, x))
            .collect(),
        Value::Object(map) => map
            .iter()
            .map(|(k, x)| (k.parse().unwrap_or(u64::MAX), x))
            .collect(),
        _ => return Err(ApiError::Unexpected(v.to_string())),
    };
    for (idx, row) in entries {
        let country = value_to_u64(row.get("country"))
            .ok_or_else(|| ApiError::Parse("missing country".into()))?
            as CountryId;
        rows.push((
            idx,
            TopCountry {
                country: CountryRef::Id(country),
                price: value_to_f64(row.get("price")).unwrap_or(0.0),
                retail_price: value_to_f64(row.get("retail_price")),
                count: value_to_u64(row.get("count")).unwrap_or(0),
                provider_id: None,
            },
        ));
    }
    rows.sort_by_key(|(i, _)| *i);
    Ok(rows.into_iter().map(|(_, r)| r).collect())
}

/// `{"tg":123,"wa_0":45,…}` — sms-activate appends `_0`/`_1` (forward flag) to service codes; we strip it.
pub fn parse_numbers_status(body: &str) -> ApiResult<BTreeMap<ServiceCode, u64>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let mut out = BTreeMap::new();
    for (key, count) in as_object(&v, "numbers status")? {
        let code = key
            .strip_suffix("_0")
            .or_else(|| key.strip_suffix("_1"))
            .unwrap_or(key);
        let n = value_to_u64(Some(count)).unwrap_or(0);
        let entry = out.entry(ServiceCode(code.to_owned())).or_insert(0);
        *entry = (*entry).max(n);
    }
    Ok(out)
}

/// `{"status":"success","countryOperators":{"73":["claro","tim",…]}}`
pub fn parse_operators(body: &str) -> ApiResult<BTreeMap<CountryRef, Vec<String>>> {
    let v: Value = serde_json::from_str(body.trim())?;
    let map = v.get("countryOperators").unwrap_or(&v);
    let mut out = BTreeMap::new();
    for (country, ops) in as_object(map, "operators")? {
        let id = CountryRef::parse(country);
        let list = ops
            .as_array()
            .map(|a| a.iter().filter_map(|x| value_to_string(Some(x))).collect())
            .unwrap_or_default();
        out.insert(id, list);
    }
    Ok(out)
}

/// sms-activate shape: `{"status":"success","activeActivations":[{"activationId":…,"serviceCode":…,"phoneNumber":…,
/// "activationCost":…,"activationStatus":…,"smsCode":…,"smsText":…,"activationTime":…,"countryCode":…,"canGetAnotherSms":…}]}`.
/// Also accepts a `data` array (Hero-SMS) or a bare array.
pub fn parse_active_activations(body: &str) -> ApiResult<Vec<ActiveActivation>> {
    let v: Value = serde_json::from_str(body.trim())?;
    active_activations_from_value(&v)
}

/// [`parse_active_activations`] over an already-parsed value (a bare array of rows is accepted,
/// so a dialect can hand over the array it prefers without re-serialising it).
pub fn active_activations_from_value(v: &Value) -> ApiResult<Vec<ActiveActivation>> {
    let items: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => ["activeActivations", "data"]
            .iter()
            .filter_map(|k| v.get(k))
            .find_map(|x| match x {
                Value::Array(a) => Some(a.iter().collect()),
                Value::Object(o) => o
                    .get("rows")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().collect()),
                _ => None,
            })
            .unwrap_or_default(),
        _ => return Err(ApiError::Unexpected(v.to_string())),
    };
    items
        .into_iter()
        .map(|a| {
            let id = value_to_string(a.get("activationId").or_else(|| a.get("id")))
                .ok_or_else(|| ApiError::Parse("activation without id".into()))?;
            Ok(ActiveActivation {
                id: ActivationId(id),
                service: value_to_string(a.get("serviceCode").or_else(|| a.get("service")))
                    .map(ServiceCode),
                phone: value_to_string(a.get("phoneNumber").or_else(|| a.get("phone"))),
                cost: value_to_f64(a.get("activationCost").or_else(|| a.get("cost"))),
                status: value_to_string(a.get("activationStatus").or_else(|| a.get("status"))),
                sms_code: value_to_string(a.get("smsCode"))
                    .filter(|s| !s.is_empty() && s != "null"),
                sms_text: value_to_string(a.get("smsText"))
                    .filter(|s| !s.is_empty() && s != "null"),
                activation_time: value_to_string(a.get("activationTime")),
                country: value_to_country(a.get("countryCode").or_else(|| a.get("country"))),
                can_get_another_sms: value_to_bool(a.get("canGetAnotherSms")),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// JSON error envelope of the Hero-SMS / Tiger SMS backend family.

/// Bodies larger than this are only deserialised to look for an error envelope when their first
/// bytes name the key in question (JSON objects list it first). Every envelope observed across
/// the Hero-SMS / Tiger SMS backend family is ≤ 151 bytes; the multi-megabyte price tables start
/// with a country or service key and are left to the action parser alone.
pub const ERROR_SHAPE_SCAN_LIMIT: usize = 4096;

/// Whether `body` is worth deserialising to look for `key` at the top level: small objects
/// always, large ones only when `key` appears within the first 64 bytes.
pub fn may_contain_top_level_key(body: &str, key: &str) -> bool {
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

/// `{"title":"<CODE>","details":"…","info":{…}}` — the JSON error envelope shared by Hero-SMS
/// and Tiger SMS (same backend family). Data objects never carry `title`.
#[derive(Clone, Debug, PartialEq)]
pub struct TitleEnvelope {
    /// The error code (`BAD_KEY`, `NOT_FOUND`, `UNPROCESSABLE_ENTITY`, …), trimmed.
    pub title: String,
    /// Human-readable text; empty when absent.
    pub details: String,
    /// Optional structured payload (`{"field","code","message"}`, `{"min"}`, …).
    pub info: Option<Value>,
}

impl TitleEnvelope {
    /// Parses `body`; `None` when it is not a `title` envelope.
    pub fn parse(body: &str) -> Option<Self> {
        let body = body.trim();
        if !may_contain_top_level_key(body, "\"title\"") {
            return None;
        }
        let v: Value = serde_json::from_str(body).ok()?;
        let title = v.get("title")?.as_str()?.trim().to_owned();
        let details = v
            .get("details")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        Some(Self {
            title,
            details,
            info: v.get("info").cloned(),
        })
    }

    /// `info.<key>`.
    pub fn info(&self, key: &str) -> Option<&Value> {
        self.info.as_ref().and_then(|i| i.get(key))
    }

    /// Maps the family's documented titles: `BAD_KEY`/`NO_KEY`, `BAD_ACTION` (empty payload —
    /// `details` is the fixed text "Method Not Found", not the action name), `NOT_FOUND`,
    /// `RATE_LIMIT` (`info.retry_after_seconds`), `UNPROCESSABLE_ENTITY` (`info.field` /
    /// `info.message` / `info.code`, else `details`), `WRONG_MAX_PRICE` (`info.min`), `BANNED`
    /// (`info.readable_date` / `info.banned_until`), `EARLY_CANCEL_DENIED`, then every standard
    /// token via [`ApiError::from_code`] (`Other` carries `"<title>: <details>"`). `None` when
    /// the title is not an error token at all.
    pub fn to_error(&self) -> Option<ApiError> {
        let details = self.details.as_str();
        let field = |k: &str| self.info(k);
        Some(match self.title.as_str() {
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
}

/// [`TitleEnvelope::parse`] followed by [`TitleEnvelope::to_error`].
pub fn error_from_title_envelope(body: &str) -> Option<ApiError> {
    TitleEnvelope::parse(body)?.to_error()
}

// ---------------------------------------------------------------------------
// Lenient JSON accessors (providers mix numbers and numeric strings freely).

pub fn as_object<'a>(v: &'a Value, what: &str) -> ApiResult<&'a serde_json::Map<String, Value>> {
    v.as_object()
        .ok_or_else(|| ApiError::Parse(format!("expected a JSON object for {what}")))
}

/// Numbers and numeric strings become `CountryRef::Id`, other strings `CountryRef::Slug`.
pub fn value_to_country(v: Option<&Value>) -> Option<CountryRef> {
    match v? {
        Value::Number(n) => n.as_u64().map(|id| CountryRef::Id(id as CountryId)),
        Value::String(s) if !s.trim().is_empty() => Some(CountryRef::parse(s)),
        _ => None,
    }
}

pub fn value_to_string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub fn value_to_f64(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn value_to_u64(v: Option<&Value>) -> Option<u64> {
    match v? {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn value_to_bool(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_i64().map(|i| i != 0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" | "" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! hero {
        ($name:literal) => {
            include_str!(concat!("../fixtures/hero_sms/", $name))
        };
    }
    macro_rules! bower {
        ($name:literal) => {
            include_str!(concat!("../fixtures/smsbower/", $name))
        };
    }

    #[test]
    fn balance_both_providers() {
        assert_eq!(parse_balance(hero!("getBalance.txt")).unwrap(), 12.5085);
        assert_eq!(parse_balance(bower!("getBalance.txt")).unwrap(), 18.739);
        assert!(parse_balance("BAD_KEY").is_err());
    }

    #[test]
    fn access_number_and_v2() {
        let a = parse_access_number("ACCESS_NUMBER:123456:79001234567").unwrap();
        assert_eq!(a.id.as_str(), "123456");
        assert_eq!(a.phone, "79001234567");
        let v2 = parse_activation_v2(
            r#"{"activationId":"987","phoneNumber":"31612345678","activationCost":"0.42","countryCode":"48","canGetAnotherSms":"1","activationTime":"2026-08-30 12:00:00","activationOperator":"kpn"}"#,
        )
        .unwrap();
        assert_eq!(v2.id.as_str(), "987");
        assert_eq!(v2.cost, Some(0.42));
        assert_eq!(v2.country, Some(CountryRef::Id(48)));
        assert_eq!(v2.can_get_another_sms, Some(true));
        assert_eq!(v2.operator.as_deref(), Some("kpn"));
        assert_eq!(parse_activation_v2("ACCESS_NUMBER:1:2").unwrap().phone, "2");
    }

    #[test]
    fn status_variants() {
        assert_eq!(
            parse_status("STATUS_WAIT_CODE").unwrap(),
            ActivationStatus::WaitCode
        );
        assert_eq!(
            parse_status("STATUS_OK:12345\n").unwrap(),
            ActivationStatus::Ok {
                code: "12345".into()
            }
        );
        assert_eq!(
            parse_status("STATUS_WAIT_RETRY:111").unwrap(),
            ActivationStatus::WaitRetry {
                last_code: "111".into()
            }
        );
        assert_eq!(
            parse_status("STATUS_CANCEL").unwrap(),
            ActivationStatus::Cancelled
        );
        assert!(parse_status("NO_ACTIVATION").is_err());
        assert_eq!(
            parse_set_status("ACCESS_CANCEL").unwrap(),
            StatusAck::Cancel
        );
        assert_eq!(
            parse_set_status("ACCESS_ACTIVATION").unwrap(),
            StatusAck::Activation
        );
    }

    #[test]
    fn prices_hero_and_bower() {
        let hero = parse_prices(hero!("getPrices_service_tg.txt")).unwrap();
        let tg = ServiceCode::from("tg");
        assert_eq!(hero[&CountryRef::Id(170)][&tg].cost, 0.45);
        assert_eq!(hero[&CountryRef::Id(170)][&tg].count, 2369);
        assert_eq!(hero[&CountryRef::Id(170)][&tg].physical_count, Some(0));

        let bower = parse_prices(bower!("getPrices_service_tg_country_187.txt")).unwrap();
        assert_eq!(bower.len(), 1);
        assert_eq!(bower[&CountryRef::Id(187)][&tg].cost, 2.592);
        assert_eq!(bower[&CountryRef::Id(187)][&tg].count, 213697);
        assert_eq!(bower[&CountryRef::Id(187)][&tg].physical_count, None);

        assert!(parse_prices("{}").unwrap().is_empty());
        let full = parse_prices(hero!("getPrices.txt")).unwrap();
        assert!(full.len() > 10);
    }

    #[test]
    fn services_and_countries_both_providers() {
        let hero = parse_services(hero!("getServicesList.txt")).unwrap();
        assert!(
            hero.iter()
                .any(|s| s.code.as_str() == "tg" && s.name == "Telegram")
        );
        let bower = parse_services(bower!("getServicesList.txt")).unwrap();
        assert!(
            bower
                .iter()
                .any(|s| s.code.as_str() == "kt" && s.name == "KakaoTalk")
        );

        let hero_c = parse_countries(hero!("getCountries.txt")).unwrap();
        let ua = hero_c.iter().find(|c| c.id() == Some(1)).unwrap();
        assert_eq!(ua.name_en, "Ukraine");
        assert_eq!(ua.rent, Some(true));
        let tiger_c =
            parse_countries(include_str!("../fixtures/tiger_sms/getCountries.txt")).unwrap();
        assert!(
            tiger_c
                .iter()
                .any(|c| c.id() == Some(74) && c.name_en == "Afghanistan" && c.rent.is_none())
        );
        let bower_c = parse_countries(bower!("getCountries.txt")).unwrap();
        let india = bower_c.iter().find(|c| c.id() == Some(22)).unwrap();
        assert_eq!(india.name_en, "India");
        assert_eq!(india.rent, None);
        assert!(bower_c.windows(2).all(|w| w[0].key < w[1].key));
    }

    #[test]
    fn top_countries_indexed_hero() {
        let rows =
            parse_top_countries_indexed(hero!("getTopCountriesByService_service_tg.txt")).unwrap();
        assert_eq!(rows[0].country, CountryRef::Id(73));
        assert_eq!(rows[0].price, 0.4);
        assert_eq!(rows[0].retail_price, Some(0.48));
        assert_eq!(rows[0].count, 1081284);
        assert!(rows.len() > 5);
    }

    #[test]
    fn numbers_status_and_operators_hero() {
        let ns = parse_numbers_status(hero!("getNumbersStatus_country_73.txt")).unwrap();
        assert_eq!(ns[&ServiceCode::from("bqp")], 284063);
        assert!(parse_numbers_status("{}").unwrap().is_empty());
        let sa = parse_numbers_status(r#"{"tg_0":5,"tg_1":7,"wa_0":1}"#).unwrap();
        assert_eq!(sa[&ServiceCode::from("tg")], 7);

        let ops = parse_operators(hero!("getOperators_country_73.txt")).unwrap();
        assert!(ops[&CountryRef::Id(73)].iter().any(|o| o == "vivo"));
    }

    #[test]
    fn active_activations_shapes() {
        let hero = parse_active_activations(hero!("getActiveActivations.txt")).unwrap();
        assert!(hero.is_empty());
        let sa = parse_active_activations(
            r#"{"status":"success","activeActivations":[{"activationId":"635468","serviceCode":"tg","phoneNumber":"79000000000","activationCost":"0.42","activationStatus":"4","smsCode":null,"smsText":null,"activationTime":"2026-08-30 10:00:00","countryCode":"0","canGetAnotherSms":"1"}]}"#,
        )
        .unwrap();
        assert_eq!(sa.len(), 1);
        assert_eq!(sa[0].id.as_str(), "635468");
        assert_eq!(sa[0].service, Some(ServiceCode::from("tg")));
        assert_eq!(sa[0].sms_code, None);
        assert_eq!(sa[0].country, Some(CountryRef::Id(0)));
    }

    #[test]
    fn classify_standard_handles_tokens_and_http() {
        let ok = HttpResponse {
            status: 200,
            body: "ACCESS_BALANCE:1".into(),
        };
        assert!(classify_standard(&ok).is_ok());
        let json = HttpResponse {
            status: 200,
            body: r#"{"a":1}"#.into(),
        };
        assert!(classify_standard(&json).is_ok());
        let err = HttpResponse {
            status: 200,
            body: bower!("getStatus_id_1.txt").into(),
        };
        assert!(matches!(
            classify_standard(&err),
            Err(ApiError::NoActivation)
        ));
        let bad = HttpResponse {
            status: 200,
            body: bower!("nosuchaction.txt").into(),
        };
        assert!(matches!(
            classify_standard(&bad),
            Err(ApiError::BadAction(_))
        ));
        let http = HttpResponse {
            status: 503,
            body: "nope".into(),
        };
        assert!(matches!(
            classify_standard(&http),
            Err(ApiError::Http { status: 503, .. })
        ));
        let limited = HttpResponse {
            status: 429,
            body: "slow down".into(),
        };
        assert!(matches!(
            classify_standard(&limited),
            Err(ApiError::RateLimited { .. })
        ));
    }

    #[test]
    fn title_envelope_family_shapes() {
        let env = TitleEnvelope::parse(hero!("getStatusV2_id_1.txt")).unwrap();
        assert_eq!(env.title, "NOT_FOUND");
        assert_eq!(env.details, "Activation Not Found");
        assert!(matches!(env.to_error(), Some(ApiError::NoActivation)));
        assert!(matches!(
            error_from_title_envelope(hero!("ratelimit_429.txt")),
            Some(ApiError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            error_from_title_envelope(
                r#"{"title":"RATE_LIMIT","details":"","info":{"retry_after_seconds":7}}"#
            ),
            Some(ApiError::RateLimited {
                retry_after: Some(7)
            })
        ));
        assert!(matches!(
            error_from_title_envelope(r#"{"title":"BAD_ACTION","details":"Method Not Found"}"#),
            Some(ApiError::BadAction(a)) if a.is_empty()
        ));
        assert!(
            matches!(error_from_title_envelope(r#"{"title":"WRONG_MAX_PRICE","info":{"min":0.5}}"#), Some(ApiError::WrongMaxPrice { min: Some(m) }) if m == 0.5)
        );
        match error_from_title_envelope(hero!("getNumberV2_service___probe___country_187.txt")) {
            Some(ApiError::Validation { field, .. }) => assert_eq!(field, "service"),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(
            matches!(error_from_title_envelope(r#"{"title":"NO_PROVIDERS","details":"none"}"#), Some(ApiError::Other(t)) if t == "NO_PROVIDERS: none")
        );
        // Not envelopes: data objects, arrays, tokens, a non-token title.
        assert!(TitleEnvelope::parse(r#"{"status":"success","data":[]}"#).is_none());
        assert!(TitleEnvelope::parse("[]").is_none());
        assert!(TitleEnvelope::parse("ACCESS_BALANCE:1").is_none());
        assert!(error_from_title_envelope(r#"{"title":"hello"}"#).is_none());
        // Large data bodies are only scanned in their first 64 bytes.
        let big = format!(
            r#"{{"187":{{"tg":{{"cost":"0.25","count":{}}}}}}}"#,
            "1".repeat(5000)
        );
        assert!(!may_contain_top_level_key(&big, "\"title\""));
        assert!(may_contain_top_level_key(r#"{"title":"x"}"#, "\"title\""));
        assert!(!may_contain_top_level_key("[]", "\"title\""));
    }
}
