//! The provider-facing trait, the per-provider [`Dialect`] hooks, and the generic [`Client`]
//! that implements the protocol once for every dialect.

use std::collections::BTreeMap;

use crate::error::{ApiError, ApiResult};
use crate::protocol;
use crate::transport::{HttpResponse, Transport};
use crate::types::*;

/// What the application talks to. Object-safe: `Box<dyn SmsActivateApi>` works.
///
/// Every provider that speaks the sms-activate protocol family implements this via
/// [`Client`] + a [`Dialect`]. Methods the provider does not support return
/// [`ApiError::Unsupported`]; check [`SmsActivateApi::capabilities`] first to avoid the round trip.
pub trait SmsActivateApi: Send + Sync {
    /// Human-readable provider name (`"Hero SMS"`).
    fn provider(&self) -> &'static str;

    fn capabilities(&self) -> Capabilities;

    /// `getBalance` → account balance in the provider's currency (USD for both current providers).
    fn get_balance(&self) -> ApiResult<f64>;

    /// Buy a number. Uses `getNumberV2` when the provider supports it, `getNumber` otherwise.
    fn get_number(&self, request: &NumberRequest) -> ApiResult<Activation>;

    /// `getStatus` — poll for the SMS code.
    fn get_status(&self, id: &ActivationId) -> ApiResult<ActivationStatus>;

    /// `setStatus` — drive the activation state machine (ready / retry / complete / cancel).
    fn set_status(&self, id: &ActivationId, action: StatusAction) -> ApiResult<StatusAck>;

    /// `getPrices` — optionally narrowed to a service and/or country.
    fn get_prices(
        &self,
        service: Option<&ServiceCode>,
        country: Option<&CountryRef>,
    ) -> ApiResult<PriceTable>;

    /// `getServicesList`
    fn get_services(&self) -> ApiResult<Vec<Service>>;

    /// `getCountries`
    fn get_countries(&self) -> ApiResult<Vec<Country>>;

    /// `getTopCountriesByService`
    fn get_top_countries(&self, service: &ServiceCode) -> ApiResult<Vec<TopCountry>>;

    /// `getNumbersStatus` — available numbers per service in a country. Optional capability.
    fn get_numbers_status(
        &self,
        _country: &CountryRef,
        _operator: Option<&str>,
    ) -> ApiResult<BTreeMap<ServiceCode, u64>> {
        Err(ApiError::Unsupported("getNumbersStatus"))
    }

    /// `getActiveActivations`. Optional capability.
    fn get_active_activations(&self) -> ApiResult<Vec<ActiveActivation>> {
        Err(ApiError::Unsupported("getActiveActivations"))
    }

    /// `getOperators`. Optional capability.
    fn get_operators(
        &self,
        _country: Option<&CountryRef>,
    ) -> ApiResult<BTreeMap<CountryRef, Vec<String>>> {
        Err(ApiError::Unsupported("getOperators"))
    }

    // -- conveniences -------------------------------------------------------

    /// `setStatus` 8 — cancel and refund.
    fn cancel(&self, id: &ActivationId) -> ApiResult<StatusAck> {
        self.set_status(id, StatusAction::Cancel)
    }

    /// `setStatus` 6 — confirm the code and finish.
    fn complete(&self, id: &ActivationId) -> ApiResult<StatusAck> {
        self.set_status(id, StatusAction::Complete)
    }

    /// `setStatus` 3 — ask for another SMS on the same number.
    fn request_another_code(&self, id: &ActivationId) -> ApiResult<StatusAck> {
        self.set_status(id, StatusAction::RequestAnotherCode)
    }
}

/// Everything that differs between providers. Defaults are the sms-activate reference behaviour,
/// so a fully compatible provider only needs `name`, `endpoint` and `capabilities`.
pub trait Dialect: Send + Sync {
    fn name(&self) -> &'static str;

    /// Full handler URL, e.g. `https://hero-sms.com/stubs/handler_api.php`.
    fn endpoint(&self) -> &str;

    fn capabilities(&self) -> Capabilities;

    /// Turn a raw HTTP response into `Ok(())` (body is data) or the provider's error.
    /// Override when the provider uses JSON error envelopes or non-200 statuses.
    fn classify(&self, resp: &HttpResponse) -> ApiResult<()> {
        protocol::classify_standard(resp)
    }

    /// Last chance to add/rename query parameters before a request goes out
    /// (e.g. provider-specific filters). `params` excludes `api_key` and `action`.
    fn adjust_params(&self, _action: &str, _params: &mut Vec<(String, String)>) {}

    fn parse_balance(&self, body: &str) -> ApiResult<f64> {
        protocol::parse_balance(body)
    }
    fn parse_activation(&self, body: &str) -> ApiResult<Activation> {
        protocol::parse_activation_v2(body)
    }
    fn parse_status(&self, body: &str) -> ApiResult<ActivationStatus> {
        protocol::parse_status(body)
    }
    fn parse_set_status(&self, body: &str) -> ApiResult<StatusAck> {
        protocol::parse_set_status(body)
    }
    fn parse_prices(&self, body: &str) -> ApiResult<PriceTable> {
        protocol::parse_prices(body)
    }
    fn parse_services(&self, body: &str) -> ApiResult<Vec<Service>> {
        protocol::parse_services(body)
    }
    fn parse_countries(&self, body: &str) -> ApiResult<Vec<Country>> {
        protocol::parse_countries(body)
    }
    fn parse_top_countries(&self, body: &str) -> ApiResult<Vec<TopCountry>> {
        protocol::parse_top_countries_indexed(body)
    }
    fn parse_numbers_status(&self, body: &str) -> ApiResult<BTreeMap<ServiceCode, u64>> {
        protocol::parse_numbers_status(body)
    }
    fn parse_active_activations(&self, body: &str) -> ApiResult<Vec<ActiveActivation>> {
        protocol::parse_active_activations(body)
    }
    fn parse_operators(&self, body: &str) -> ApiResult<BTreeMap<CountryRef, Vec<String>>> {
        protocol::parse_operators(body)
    }
}

/// Generic protocol client: `Transport` does HTTP, `Dialect` supplies provider specifics.
pub struct Client<T, D> {
    transport: T,
    dialect: D,
    api_key: String,
}

impl<T: Transport, D: Dialect> Client<T, D> {
    pub fn new(transport: T, dialect: D, api_key: impl Into<String>) -> Self {
        Self {
            transport,
            dialect,
            api_key: api_key.into(),
        }
    }

    pub fn dialect(&self) -> &D {
        &self.dialect
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Performs `action` with the given query parameters and returns the raw body after the
    /// dialect has classified it as data. Providers use this for their own extra actions.
    pub fn call(&self, action: &str, params: Vec<(String, String)>) -> ApiResult<String> {
        let mut params = params;
        self.dialect.adjust_params(action, &mut params);
        let url = build_url(self.dialect.endpoint(), &self.api_key, action, &params);
        let resp = self.transport.get(&url)?;
        self.dialect.classify(&resp)?;
        Ok(resp.body)
    }

    fn require(&self, enabled: bool, action: &'static str) -> ApiResult<()> {
        if enabled {
            Ok(())
        } else {
            Err(ApiError::Unsupported(action))
        }
    }
}

impl<T: Transport, D: Dialect> SmsActivateApi for Client<T, D> {
    fn provider(&self) -> &'static str {
        self.dialect.name()
    }

    fn capabilities(&self) -> Capabilities {
        self.dialect.capabilities()
    }

    fn get_balance(&self) -> ApiResult<f64> {
        let body = self.call("getBalance", Vec::new())?;
        self.dialect.parse_balance(&body)
    }

    fn get_number(&self, request: &NumberRequest) -> ApiResult<Activation> {
        let caps = self.dialect.capabilities();
        let mut params = vec![
            ("service".to_owned(), request.service.0.clone()),
            ("country".to_owned(), request.country.to_string()),
        ];
        if let Some(op) = &request.operator {
            params.push(("operator".to_owned(), op.clone()));
        }
        if let Some(p) = request.max_price {
            params.push(("maxPrice".to_owned(), fmt_price(p)));
        }
        if let Some(p) = request.min_price {
            params.push(("minPrice".to_owned(), fmt_price(p)));
        }
        params.extend(request.extra.iter().cloned());
        if caps.get_number_v2 {
            let body = self.call("getNumberV2", params)?;
            self.dialect.parse_activation(&body)
        } else {
            let body = self.call("getNumber", params)?;
            protocol::parse_access_number(&body)
        }
    }

    fn get_status(&self, id: &ActivationId) -> ApiResult<ActivationStatus> {
        let body = self.call("getStatus", vec![("id".to_owned(), id.0.clone())])?;
        self.dialect.parse_status(&body)
    }

    fn set_status(&self, id: &ActivationId, action: StatusAction) -> ApiResult<StatusAck> {
        let body = self.call(
            "setStatus",
            vec![
                ("id".to_owned(), id.0.clone()),
                ("status".to_owned(), action.code().to_string()),
            ],
        )?;
        self.dialect.parse_set_status(&body)
    }

    fn get_prices(
        &self,
        service: Option<&ServiceCode>,
        country: Option<&CountryRef>,
    ) -> ApiResult<PriceTable> {
        let mut params = Vec::new();
        if let Some(s) = service {
            params.push(("service".to_owned(), s.0.clone()));
        }
        if let Some(c) = country {
            params.push(("country".to_owned(), c.to_string()));
        }
        let body = self.call("getPrices", params)?;
        self.dialect.parse_prices(&body)
    }

    fn get_services(&self) -> ApiResult<Vec<Service>> {
        let body = self.call("getServicesList", Vec::new())?;
        self.dialect.parse_services(&body)
    }

    fn get_countries(&self) -> ApiResult<Vec<Country>> {
        let body = self.call("getCountries", Vec::new())?;
        self.dialect.parse_countries(&body)
    }

    fn get_top_countries(&self, service: &ServiceCode) -> ApiResult<Vec<TopCountry>> {
        let body = self.call(
            "getTopCountriesByService",
            vec![("service".to_owned(), service.0.clone())],
        )?;
        self.dialect.parse_top_countries(&body)
    }

    fn get_numbers_status(
        &self,
        country: &CountryRef,
        operator: Option<&str>,
    ) -> ApiResult<BTreeMap<ServiceCode, u64>> {
        self.require(
            self.dialect.capabilities().numbers_status,
            "getNumbersStatus",
        )?;
        let mut params = vec![("country".to_owned(), country.to_string())];
        if let Some(op) = operator {
            params.push(("operator".to_owned(), op.to_owned()));
        }
        let body = self.call("getNumbersStatus", params)?;
        self.dialect.parse_numbers_status(&body)
    }

    fn get_active_activations(&self) -> ApiResult<Vec<ActiveActivation>> {
        self.require(
            self.dialect.capabilities().active_activations,
            "getActiveActivations",
        )?;
        let body = self.call("getActiveActivations", Vec::new())?;
        self.dialect.parse_active_activations(&body)
    }

    fn get_operators(
        &self,
        country: Option<&CountryRef>,
    ) -> ApiResult<BTreeMap<CountryRef, Vec<String>>> {
        self.require(self.dialect.capabilities().operators, "getOperators")?;
        let params = country
            .map(|c| vec![("country".to_owned(), c.to_string())])
            .unwrap_or_default();
        let body = self.call("getOperators", params)?;
        self.dialect.parse_operators(&body)
    }
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

/// `<endpoint>?api_key=…&action=…&k=v…` with percent-encoded values.
pub fn build_url(
    endpoint: &str,
    api_key: &str,
    action: &str,
    params: &[(String, String)],
) -> String {
    let mut url = String::with_capacity(endpoint.len() + 64 + params.len() * 16);
    url.push_str(endpoint);
    url.push(if endpoint.contains('?') { '&' } else { '?' });
    url.push_str("api_key=");
    url.push_str(&encode(api_key));
    url.push_str("&action=");
    url.push_str(&encode(action));
    for (k, v) in params {
        url.push('&');
        url.push_str(&encode(k));
        url.push('=');
        url.push_str(&encode(v));
    }
    url
}

/// RFC 3986 unreserved characters pass through; everything else is `%XX`-encoded (UTF-8).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FakeTransport;

    struct Plain;
    impl Dialect for Plain {
        fn name(&self) -> &'static str {
            "Plain"
        }
        fn endpoint(&self) -> &str {
            "https://example.test/stubs/handler_api.php"
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
    }

    struct V2;
    impl Dialect for V2 {
        fn name(&self) -> &'static str {
            "V2"
        }
        fn endpoint(&self) -> &str {
            "https://v2.test/api"
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                get_number_v2: true,
                numbers_status: true,
                ..Default::default()
            }
        }
    }

    #[test]
    fn url_building_and_encoding() {
        let url = build_url(
            "https://x.test/h.php",
            "k&y",
            "getNumber",
            &[
                ("service".into(), "tg".into()),
                ("ref".into(), "a b/ü".into()),
            ],
        );
        assert_eq!(
            url,
            "https://x.test/h.php?api_key=k%26y&action=getNumber&service=tg&ref=a%20b%2F%C3%BC"
        );
        assert_eq!(fmt_price(0.25), "0.25");
        assert_eq!(fmt_price(1.0), "1");
        assert_eq!(fmt_price(0.15689), "0.1569");
    }

    #[test]
    fn balance_round_trip_records_request() {
        let t = FakeTransport::new().push(200, "ACCESS_BALANCE:3.5");
        let c = Client::new(t, Plain, "KEY");
        assert_eq!(c.get_balance().unwrap(), 3.5);
        let reqs = c.transport().requests();
        assert_eq!(
            reqs,
            vec!["https://example.test/stubs/handler_api.php?api_key=KEY&action=getBalance"]
        );
    }

    #[test]
    fn get_number_uses_v1_or_v2_by_capability() {
        let req = NumberRequest::new("tg", 187)
            .max_price(0.5)
            .extra("providerIds", "1,2");
        let c = Client::new(
            FakeTransport::new().push(200, "ACCESS_NUMBER:42:12025550123"),
            Plain,
            "K",
        );
        let a = c.get_number(&req).unwrap();
        assert_eq!(a.id.as_str(), "42");
        assert!(
            c.transport().requests()[0]
                .contains("action=getNumber&service=tg&country=187&maxPrice=0.5&providerIds=1%2C2")
        );

        let c = Client::new(
            FakeTransport::new().push(200, r#"{"activationId":7,"phoneNumber":"1","activationCost":0.3,"countryCode":187,"canGetAnotherSms":true,"activationTime":"t","activationOperator":"op"}"#),
            V2,
            "K",
        );
        let a = c.get_number(&req).unwrap();
        assert_eq!(a.cost, Some(0.3));
        assert!(c.transport().requests()[0].contains("action=getNumberV2&"));
    }

    #[test]
    fn status_set_status_and_errors() {
        let c = Client::new(
            FakeTransport::new()
                .push(200, "STATUS_OK:5555")
                .push(200, "ACCESS_CANCEL")
                .push(200, "NO_ACTIVATION")
                .push(500, "boom"),
            Plain,
            "K",
        );
        let id = ActivationId::from("9");
        assert_eq!(
            c.get_status(&id).unwrap(),
            ActivationStatus::Ok {
                code: "5555".into()
            }
        );
        assert_eq!(c.cancel(&id).unwrap(), StatusAck::Cancel);
        assert!(matches!(c.get_status(&id), Err(ApiError::NoActivation)));
        assert!(matches!(
            c.get_status(&id),
            Err(ApiError::Http { status: 500, .. })
        ));
        assert!(c.transport().requests()[1].ends_with("action=setStatus&id=9&status=8"));
    }

    #[test]
    fn unsupported_actions_do_not_hit_the_network() {
        let c = Client::new(FakeTransport::new(), Plain, "K");
        assert!(matches!(
            c.get_numbers_status(&CountryRef::Id(1), None),
            Err(ApiError::Unsupported("getNumbersStatus"))
        ));
        assert!(matches!(
            c.get_active_activations(),
            Err(ApiError::Unsupported(_))
        ));
        assert!(matches!(
            c.get_operators(None),
            Err(ApiError::Unsupported(_))
        ));
        assert!(c.transport().requests().is_empty());

        let c = Client::new(FakeTransport::new().push(200, r#"{"tg":3}"#), V2, "K");
        assert_eq!(
            c.get_numbers_status(&CountryRef::Id(1), Some("mts"))
                .unwrap()[&ServiceCode::from("tg")],
            3
        );
        assert!(
            c.transport().requests()[0].ends_with("action=getNumbersStatus&country=1&operator=mts")
        );
    }

    #[test]
    fn trait_is_object_safe() {
        let boxed: Box<dyn SmsActivateApi> = Box::new(Client::new(
            FakeTransport::new().push(200, "ACCESS_BALANCE:1"),
            Plain,
            "K",
        ));
        assert_eq!(boxed.provider(), "Plain");
        assert_eq!(boxed.get_balance().unwrap(), 1.0);
    }
}
