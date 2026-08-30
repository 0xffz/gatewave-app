//! Provider-neutral data types for the sms-activate protocol family.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Opaque activation (order) identifier. Kept as a string: some providers use 64-bit ids.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivationId(pub String);

impl ActivationId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActivationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ActivationId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}
impl From<String> for ActivationId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl From<u64> for ActivationId {
    fn from(n: u64) -> Self {
        Self(n.to_string())
    }
}

/// Short service code as used by the protocol (`tg` = Telegram, `wa` = WhatsApp, `go` = Google …).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServiceCode(pub String);

impl ServiceCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServiceCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ServiceCode {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Numeric country id used by the sms-activate family (0 = Russia, 1 = Ukraine, 187 = USA …).
/// Providers mostly share the numbering but may differ; always resolve via [`Country`] lists.
/// See [`CountryRef`] for the provider-neutral key.
pub type CountryId = u32;

/// Parameters for `getNumber` / `getNumberV2`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NumberRequest {
    pub service: ServiceCode,
    /// Provider-native country key (see [`CountryRef`]).
    pub country: CountryRef,
    /// Mobile operator filter (provider-specific names, see `get_operators`).
    pub operator: Option<String>,
    /// Refuse numbers more expensive than this (`maxPrice`).
    pub max_price: Option<f64>,
    /// Refuse numbers cheaper than this (`minPrice`, SMSBower).
    pub min_price: Option<f64>,
    /// Extra provider-specific query parameters, appended verbatim (e.g. `providerIds`).
    pub extra: Vec<(String, String)>,
}

impl NumberRequest {
    pub fn new(service: impl Into<ServiceCode>, country: impl Into<CountryRef>) -> Self {
        Self {
            service: service.into(),
            country: country.into(),
            ..Default::default()
        }
    }
    pub fn operator(mut self, operator: impl Into<String>) -> Self {
        self.operator = Some(operator.into());
        self
    }
    pub fn max_price(mut self, price: f64) -> Self {
        self.max_price = Some(price);
        self
    }
    pub fn min_price(mut self, price: f64) -> Self {
        self.min_price = Some(price);
        self
    }
    pub fn extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.push((key.into(), value.into()));
        self
    }
}

/// A purchased number. Only `id` and `phone` are guaranteed (`getNumber`); the rest comes from `getNumberV2`.
#[derive(Clone, Debug, PartialEq)]
pub struct Activation {
    pub id: ActivationId,
    /// Phone number in international format without `+`, exactly as returned by the provider.
    pub phone: String,
    pub cost: Option<f64>,
    pub country: Option<CountryRef>,
    pub can_get_another_sms: Option<bool>,
    /// Provider-formatted timestamp (usually `YYYY-MM-DD HH:MM:SS`).
    pub activation_time: Option<String>,
    pub operator: Option<String>,
}

impl Activation {
    pub fn new(id: impl Into<ActivationId>, phone: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            phone: phone.into(),
            cost: None,
            country: None,
            can_get_another_sms: None,
            activation_time: None,
            operator: None,
        }
    }
}

/// Result of `getStatus`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivationStatus {
    /// `STATUS_WAIT_CODE` — no SMS yet.
    WaitCode,
    /// `STATUS_WAIT_RETRY:<last_code>` — another SMS was requested; `last_code` is the previous one.
    WaitRetry { last_code: String },
    /// `STATUS_WAIT_RESEND` — waiting for the client to resend (rare).
    WaitResend,
    /// `STATUS_CANCEL` — activation cancelled.
    Cancelled,
    /// `STATUS_OK:<code>` — code received.
    Ok { code: String },
    /// The activation timed out without a code (5SIM `TIMEOUT`); sms-activate providers report this as `Cancelled`.
    Expired,
    /// The activation was completed by the client (5SIM `FINISHED`); `code` is the last code received, if any.
    Finished { code: Option<String> },
}

impl ActivationStatus {
    /// The received code, if any.
    pub fn code(&self) -> Option<&str> {
        match self {
            ActivationStatus::Ok { code } => Some(code),
            ActivationStatus::Finished { code } => code.as_deref(),
            _ => None,
        }
    }
    /// Whether polling can stop: `Ok`, `Cancelled`, `Expired` (5SIM `TIMEOUT`) and `Finished`
    /// (5SIM `FINISHED`) are terminal.
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            ActivationStatus::Ok { .. }
                | ActivationStatus::Cancelled
                | ActivationStatus::Expired
                | ActivationStatus::Finished { .. }
        )
    }
}

/// `setStatus` transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusAction {
    /// 1 — SMS has been sent to the number (optional).
    Ready,
    /// 3 — request another SMS (free).
    RequestAnotherCode,
    /// 6 — confirm the code and complete the activation.
    Complete,
    /// 8 — cancel the activation (refund).
    Cancel,
}

impl StatusAction {
    pub fn code(self) -> u8 {
        match self {
            StatusAction::Ready => 1,
            StatusAction::RequestAnotherCode => 3,
            StatusAction::Complete => 6,
            StatusAction::Cancel => 8,
        }
    }
}

/// Acknowledgement returned by `setStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusAck {
    /// `ACCESS_READY`
    Ready,
    /// `ACCESS_RETRY_GET`
    RetryGet,
    /// `ACCESS_ACTIVATION`
    Activation,
    /// `ACCESS_CANCEL`
    Cancel,
}

/// One cell of the price table.
#[derive(Clone, Debug, PartialEq)]
pub struct Price {
    pub cost: f64,
    pub count: u64,
    /// Hero-SMS only: numbers on physical SIMs.
    pub physical_count: Option<u64>,
}

/// `getPrices` result: country → service → price.
pub type PriceTable = BTreeMap<CountryRef, BTreeMap<ServiceCode, Price>>;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Service {
    pub code: ServiceCode,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Country {
    /// The key to pass back to this provider (`Id` for the sms-activate family, `Slug` for 5SIM).
    pub key: CountryRef,
    pub name_en: String,
    pub name_ru: Option<String>,
    pub name_cn: Option<String>,
    /// ISO 3166-1 alpha-2 code, lowercase, when the provider exposes it (5SIM).
    pub iso: Option<String>,
    /// Dialling prefix such as `+44`, when the provider exposes it (5SIM).
    pub prefix: Option<String>,
    pub visible: Option<bool>,
    pub retry: Option<bool>,
    pub rent: Option<bool>,
}

impl Country {
    /// Numeric id for sms-activate-family providers; `None` for slug-keyed providers.
    pub fn id(&self) -> Option<CountryId> {
        self.key.id()
    }
}

/// Provider-native country identifier.
///
/// The sms-activate family uses numeric ids (`187` = USA); REST providers such as 5SIM use names
/// (`"england"`). Every country-keyed API in this crate uses this type, so the app can pass a
/// provider's own key straight back to it. Ids and slugs are *provider-specific*: resolve them via
/// [`SmsActivateApi::get_countries`](crate::SmsActivateApi::get_countries) rather than assuming a shared numbering.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CountryRef {
    Id(CountryId),
    /// Human slug such as `united-states` (SMSBower) or `england` (5SIM).
    Slug(String),
}

impl CountryRef {
    /// `"187"` → `Id(187)`, anything else → `Slug`.
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        match s.parse::<CountryId>() {
            Ok(id) => CountryRef::Id(id),
            Err(_) => CountryRef::Slug(s.to_owned()),
        }
    }

    pub fn id(&self) -> Option<CountryId> {
        match self {
            CountryRef::Id(id) => Some(*id),
            CountryRef::Slug(_) => None,
        }
    }

    pub fn slug(&self) -> Option<&str> {
        match self {
            CountryRef::Id(_) => None,
            CountryRef::Slug(s) => Some(s),
        }
    }
}

impl Default for CountryRef {
    fn default() -> Self {
        CountryRef::Id(0)
    }
}

impl fmt::Display for CountryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CountryRef::Id(id) => write!(f, "{id}"),
            CountryRef::Slug(s) => f.write_str(s),
        }
    }
}

impl From<CountryId> for CountryRef {
    fn from(id: CountryId) -> Self {
        CountryRef::Id(id)
    }
}

impl From<&str> for CountryRef {
    fn from(s: &str) -> Self {
        CountryRef::parse(s)
    }
}

impl From<String> for CountryRef {
    fn from(s: String) -> Self {
        CountryRef::parse(&s)
    }
}

impl From<&CountryRef> for CountryRef {
    fn from(c: &CountryRef) -> Self {
        c.clone()
    }
}

/// One row of `getTopCountriesByService`.
#[derive(Clone, Debug, PartialEq)]
pub struct TopCountry {
    pub country: CountryRef,
    pub price: f64,
    pub retail_price: Option<f64>,
    pub count: u64,
    /// Upstream partner/provider id when the provider exposes per-partner offers (SMSBower).
    pub provider_id: Option<String>,
}

/// One row of `getActiveActivations`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveActivation {
    pub id: ActivationId,
    pub service: Option<ServiceCode>,
    pub phone: Option<String>,
    pub cost: Option<f64>,
    /// Raw provider status string (e.g. `4` / `STATUS_WAIT_CODE`).
    pub status: Option<String>,
    pub sms_code: Option<String>,
    pub sms_text: Option<String>,
    pub activation_time: Option<String>,
    pub country: Option<CountryRef>,
    pub can_get_another_sms: Option<bool>,
}

/// Which optional parts of the protocol a provider implements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// `getNumberV2` (JSON activation with cost/operator/time).
    pub get_number_v2: bool,
    /// `getNumbersStatus` (available counts per service for a country).
    pub numbers_status: bool,
    /// `getActiveActivations`.
    pub active_activations: bool,
    /// `getOperators`.
    pub operators: bool,
    /// `getPricesV2` (price → count histogram).
    pub prices_v2: bool,
    /// `getPricesV3` (per upstream provider).
    pub prices_v3: bool,
    /// Honours `maxPrice` / `minPrice` on `getNumber`.
    /// Some providers implement only `maxPrice` (Hero-SMS, Tiger SMS); see the provider module docs.
    pub price_bounds: bool,
    /// `providerIds` / `exceptProviderIds` filters on `getNumber`.
    pub provider_filters: bool,
}
