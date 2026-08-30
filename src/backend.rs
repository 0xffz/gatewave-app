//! What the app needs from a provider, and the real implementation over the `sms-activate` clients.
//!
//! [`Backend`] is deliberately narrower than `SmsActivateApi`: it speaks in app terms (rows for
//! the wizard, offer groups with a [`OfferSelector`] that says how to buy). Everything the four
//! providers share goes through the trait; the two places they differ — how offers are listed
//! and how a chosen offer is turned into a purchase request — are matched per provider.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sms_activate::providers::fivesim::FiveSim;
use sms_activate::providers::hero_sms::HeroSms;
use sms_activate::providers::smsbower::{SmsBower, SmsBowerRequestExt};
use sms_activate::providers::tiger_sms::{TigerNumberOptions, TigerSms};
use sms_activate::{
    Activation, ActivationId, ActivationStatus, ActiveActivation, ApiError, ApiResult,
    Capabilities, Country, CountryRef, NumberRequest, Service, ServiceCode, SmsActivateApi,
    StatusAck,
};

use crate::domain::country_badge;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProviderKind {
    HeroSms,
    FiveSim,
    TigerSms,
    SmsBower,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 4] = [
        ProviderKind::HeroSms,
        ProviderKind::FiveSim,
        ProviderKind::TigerSms,
        ProviderKind::SmsBower,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ProviderKind::HeroSms => "Hero SMS",
            ProviderKind::FiveSim => "5SIM",
            ProviderKind::TigerSms => "Tiger SMS",
            ProviderKind::SmsBower => "SMSBower",
        }
    }

    /// Environment variable / `.env` entry holding this provider's key.
    pub fn env_key(self) -> &'static str {
        match self {
            ProviderKind::HeroSms => "HERO_SMS_API_KEY",
            ProviderKind::FiveSim => "FIVESIM_API_KEY",
            ProviderKind::TigerSms => "TIGER_SMS_API_KEY",
            ProviderKind::SmsBower => "SMSBOWER_API_KEY",
        }
    }

    /// Where the user finds the key (shown next to the input in Settings).
    pub fn key_hint(self) -> &'static str {
        match self {
            ProviderKind::HeroSms => "API key from hero-sms.com → Profile",
            ProviderKind::FiveSim => "JWT token from 5sim.net → Profile → API key",
            ProviderKind::TigerSms => "API key from tiger-sms.com → Profile",
            ProviderKind::SmsBower => "API key from smsbower.app → Profile",
        }
    }

    /// Minimum plausible key length, used to reject obvious typos before a network call.
    pub fn min_key_len(self) -> usize {
        match self {
            ProviderKind::FiveSim => 32,
            _ => 8,
        }
    }

    /// How long after a purchase the provider refuses to cancel (`EARLY_CANCEL_DENIED`).
    /// Known up front so the Cancel button can count the wait down instead of bouncing off
    /// the API. `None` when the provider cancels straight away.
    pub fn cancel_grace(self) -> Option<Duration> {
        match self {
            // `info.minActivationTime: 120` on Hero-SMS / Tiger SMS; SMSBower documents the
            // same two minutes.
            ProviderKind::HeroSms | ProviderKind::TigerSms => Some(Duration::from_secs(120)),
            ProviderKind::SmsBower => Some(sms_activate::providers::smsbower::CANCEL_GRACE_PERIOD),
            ProviderKind::FiveSim => None,
        }
    }
}

/// How to turn a chosen offer into a purchase request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OfferSelector {
    /// Provider default (cheapest available).
    Any,
    /// Bid up to this price (`maxPrice`).
    MaxPrice(f64),
    /// A specific mobile operator (5SIM path segment / sms-activate `operator`).
    Operator(String),
    /// A specific upstream partner (`providerIds`), at its quoted price.
    Partner { id: String, price: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OfferTier {
    pub price: f64,
    pub count: u64,
    pub selector: OfferSelector,
}

/// One block on step 4: an operator / partner with its price tiers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OfferGroup {
    pub name: String,
    pub total: u64,
    pub tiers: Vec<OfferTier>,
}

pub const ANY_OPERATOR: &str = "Any operator";

/// One row on step 3: a country where the chosen service is sold.
#[derive(Clone, Debug, PartialEq)]
pub struct CountryRow {
    pub key: CountryRef,
    pub name: String,
    /// Two-letter badge (ISO code or initials).
    pub code: String,
    pub dial: Option<String>,
    pub price: f64,
    pub count: u64,
}

/// A provider as the app sees it. All methods block; the app calls them from [`crate::worker::Worker`].
pub trait Backend: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> Capabilities;
    fn balance(&self) -> ApiResult<f64>;
    fn services(&self) -> ApiResult<Vec<Service>>;
    /// Countries where `service` is sold, with its price and stock there.
    fn countries_for(&self, service: &ServiceCode) -> ApiResult<Vec<CountryRow>>;
    /// Offer groups for a service in a country.
    fn offers(&self, service: &ServiceCode, country: &CountryRef) -> ApiResult<Vec<OfferGroup>>;
    /// Buys a number. **Spends money.**
    fn buy(
        &self,
        service: &ServiceCode,
        country: &CountryRef,
        selector: &OfferSelector,
    ) -> ApiResult<Activation>;
    fn status(&self, id: &ActivationId) -> ApiResult<ActivationStatus>;
    fn cancel(&self, id: &ActivationId) -> ApiResult<StatusAck>;
    fn complete(&self, id: &ActivationId) -> ApiResult<StatusAck>;
    /// Activations the provider still considers active (empty when unsupported).
    fn active(&self) -> ApiResult<Vec<ActiveActivation>>;
}

pub type SharedBackend = Arc<dyn Backend>;

// ---------------------------------------------------------------------------
// Real providers

enum Inner {
    Hero(HeroSms),
    FiveSim(FiveSim),
    Tiger(TigerSms),
    Bower(SmsBower),
}

pub struct RealBackend {
    kind: ProviderKind,
    inner: Inner,
    countries: Mutex<Option<Arc<Vec<Country>>>>,
}

impl RealBackend {
    pub fn connect(kind: ProviderKind, api_key: &str) -> Self {
        let inner = match kind {
            ProviderKind::HeroSms => Inner::Hero(HeroSms::with_api_key(api_key)),
            ProviderKind::FiveSim => Inner::FiveSim(FiveSim::with_api_key(api_key)),
            ProviderKind::TigerSms => Inner::Tiger(TigerSms::with_api_key(api_key)),
            ProviderKind::SmsBower => Inner::Bower(SmsBower::with_api_key(api_key)),
        };
        Self {
            kind,
            inner,
            countries: Mutex::new(None),
        }
    }

    pub fn shared(kind: ProviderKind, api_key: &str) -> SharedBackend {
        Arc::new(Self::connect(kind, api_key))
    }

    fn api(&self) -> &dyn SmsActivateApi {
        match &self.inner {
            Inner::Hero(c) => c,
            Inner::FiveSim(c) => c,
            Inner::Tiger(c) => c,
            Inner::Bower(c) => c,
        }
    }

    /// `getCountries` is large and static; fetch it once per session.
    fn countries_cached(&self) -> ApiResult<Arc<Vec<Country>>> {
        if let Some(c) = self.countries.lock().unwrap().clone() {
            return Ok(c);
        }
        let fetched = Arc::new(self.api().get_countries()?);
        *self.countries.lock().unwrap() = Some(fetched.clone());
        Ok(fetched)
    }

    /// Single "Any operator" group from the plain price table — used when a provider has no
    /// richer offer data for the pair.
    fn fallback_offers(
        &self,
        service: &ServiceCode,
        country: &CountryRef,
    ) -> ApiResult<Vec<OfferGroup>> {
        let table = self.api().get_prices(Some(service), Some(country))?;
        let price = table.get(country).and_then(|m| m.get(service));
        Ok(match price {
            Some(p) => vec![OfferGroup {
                name: ANY_OPERATOR.into(),
                total: p.count,
                tiers: vec![OfferTier {
                    price: p.cost,
                    count: p.count,
                    selector: OfferSelector::Any,
                }],
            }],
            None => Vec::new(),
        })
    }
}

fn numeric_country(country: &CountryRef) -> ApiResult<u32> {
    country.id().ok_or(ApiError::BadCountry)
}

impl Backend for RealBackend {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    fn capabilities(&self) -> Capabilities {
        self.api().capabilities()
    }

    fn balance(&self) -> ApiResult<f64> {
        self.api().get_balance()
    }

    fn services(&self) -> ApiResult<Vec<Service>> {
        let mut services = self.api().get_services()?;
        services.sort_by_key(|s| s.name.to_lowercase());
        services.dedup_by(|a, b| a.code == b.code);
        Ok(services)
    }

    fn countries_for(&self, service: &ServiceCode) -> ApiResult<Vec<CountryRow>> {
        let prices = self.api().get_prices(Some(service), None)?;
        let countries = self.countries_cached()?;
        let mut rows: Vec<CountryRow> = prices
            .into_iter()
            .filter_map(|(key, services)| {
                let price = services.get(service)?;
                let info = countries.iter().find(|c| c.key == key);
                let name = info
                    .map(|c| c.name_en.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| key.to_string());
                Some(CountryRow {
                    code: country_badge(&name, info.and_then(|c| c.iso.as_deref())),
                    dial: info.and_then(|c| c.prefix.clone()),
                    name,
                    key,
                    price: price.cost,
                    count: price.count,
                })
            })
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(rows)
    }

    fn offers(&self, service: &ServiceCode, country: &CountryRef) -> ApiResult<Vec<OfferGroup>> {
        let mut groups = match &self.inner {
            Inner::Hero(h) => {
                let id = numeric_country(country)?;
                let rows = h.get_top_countries_free_price(service)?;
                match rows.iter().find(|r| r.country == id) {
                    Some(row) if !row.free_price_map.is_empty() => vec![OfferGroup {
                        name: ANY_OPERATOR.into(),
                        total: row.count,
                        tiers: row
                            .free_price_map
                            .iter()
                            .map(|t| OfferTier {
                                price: t.price,
                                count: t.count,
                                selector: OfferSelector::MaxPrice(t.price),
                            })
                            .collect(),
                    }],
                    Some(row) => vec![OfferGroup {
                        name: ANY_OPERATOR.into(),
                        total: row.count,
                        tiers: vec![OfferTier {
                            price: row.price,
                            count: row.count,
                            selector: OfferSelector::Any,
                        }],
                    }],
                    None => Vec::new(),
                }
            }
            Inner::Bower(b) => {
                let id = numeric_country(country)?;
                let mut groups = Vec::new();
                let hist = b.get_prices_v2(Some(service), Some(id))?;
                if let Some(buckets) = hist.get(&id).and_then(|m| m.get(service))
                    && !buckets.is_empty()
                {
                    groups.push(OfferGroup {
                        name: ANY_OPERATOR.into(),
                        total: buckets.iter().map(|b| b.count).sum(),
                        tiers: buckets
                            .iter()
                            .map(|b| OfferTier {
                                price: b.price,
                                count: b.count,
                                selector: OfferSelector::MaxPrice(b.price),
                            })
                            .collect(),
                    });
                }
                let v3 = b.get_prices_v3(Some(service), Some(id))?;
                for offer in v3
                    .get(&id)
                    .and_then(|m| m.get(service))
                    .into_iter()
                    .flatten()
                {
                    groups.push(OfferGroup {
                        name: format!("Partner {}", offer.provider_id),
                        total: offer.count,
                        tiers: vec![OfferTier {
                            price: offer.price,
                            count: offer.count,
                            selector: OfferSelector::Partner {
                                id: offer.provider_id.clone(),
                                price: offer.price,
                            },
                        }],
                    });
                }
                groups
            }
            Inner::Tiger(t) => {
                let id = numeric_country(country)?;
                let mut groups = Vec::new();
                let ladders = t.get_free_prices(Some(service), Some(id))?;
                if let Some(ladder) = ladders.get(&id).and_then(|m| m.get(service))
                    && !ladder.buckets.is_empty()
                {
                    groups.push(OfferGroup {
                        name: ANY_OPERATOR.into(),
                        total: ladder.total_count(),
                        tiers: ladder
                            .buckets
                            .iter()
                            .map(|b| OfferTier {
                                price: b.price,
                                count: b.count,
                                selector: OfferSelector::MaxPrice(b.price),
                            })
                            .collect(),
                    });
                }
                let v3 = t.get_prices_v3(service, id)?;
                for offer in v3
                    .get(&id)
                    .and_then(|m| m.get(service))
                    .map(|p| p.providers.as_slice())
                    .unwrap_or_default()
                {
                    let Some(price) = offer.cheapest() else {
                        continue;
                    };
                    groups.push(OfferGroup {
                        name: format!("Provider {}", offer.provider_id),
                        total: offer.count,
                        tiers: vec![OfferTier {
                            price,
                            count: offer.count,
                            selector: OfferSelector::Partner {
                                id: offer.provider_id.to_string(),
                                price,
                            },
                        }],
                    });
                }
                groups
            }
            Inner::FiveSim(f) => {
                let slug = country.slug().ok_or(ApiError::BadCountry)?;
                let ops = f.prices_by_operator(slug, service.as_str())?;
                let mut groups = Vec::new();
                let in_stock: Vec<(&String, _)> = ops.iter().filter(|(_, o)| o.count > 0).collect();
                let pool: Vec<_> = if in_stock.is_empty() {
                    ops.iter().collect()
                } else {
                    in_stock
                };
                if let Some(min) = pool
                    .iter()
                    .map(|(_, o)| o.cost)
                    .min_by(|a, b| a.total_cmp(b))
                {
                    groups.push(OfferGroup {
                        name: ANY_OPERATOR.into(),
                        total: ops.values().map(|o| o.count).sum(),
                        tiers: vec![OfferTier {
                            price: min,
                            count: ops.values().map(|o| o.count).sum(),
                            selector: OfferSelector::Any,
                        }],
                    });
                }
                let mut named: Vec<_> = ops.iter().collect();
                named.sort_by(|(a, x), (b, y)| {
                    (y.count > 0)
                        .cmp(&(x.count > 0))
                        .then(x.cost.total_cmp(&y.cost))
                        .then(a.cmp(b))
                });
                for (op, o) in named {
                    groups.push(OfferGroup {
                        name: op.clone(),
                        total: o.count,
                        tiers: vec![OfferTier {
                            price: o.cost,
                            count: o.count,
                            selector: OfferSelector::Operator(op.clone()),
                        }],
                    });
                }
                groups
            }
        };
        if groups.is_empty() {
            groups = self.fallback_offers(service, country)?;
        }
        Ok(groups)
    }

    fn buy(
        &self,
        service: &ServiceCode,
        country: &CountryRef,
        selector: &OfferSelector,
    ) -> ApiResult<Activation> {
        let base = NumberRequest::new(service.clone(), country.clone());
        let req = match (selector, &self.inner) {
            (OfferSelector::Any, _) => base,
            (OfferSelector::MaxPrice(p), _) => base.max_price(*p),
            (OfferSelector::Operator(op), _) => base.operator(op.clone()),
            (OfferSelector::Partner { id, price }, Inner::Bower(_)) => {
                base.max_price(*price).provider_ids([id.as_str()])
            }
            (OfferSelector::Partner { id, price }, Inner::Tiger(_)) => {
                let pid: u64 = id.parse().map_err(|_| ApiError::Validation {
                    field: "providerIds".into(),
                    message: format!("not a numeric provider id: {id}"),
                })?;
                TigerNumberOptions::new()
                    .provider_ids([pid])
                    .apply(&base.max_price(*price))
            }
            // Providers without partner filters: honour the quoted price instead.
            (OfferSelector::Partner { price, .. }, _) => base.max_price(*price),
        };
        self.api().get_number(&req)
    }

    fn status(&self, id: &ActivationId) -> ApiResult<ActivationStatus> {
        self.api().get_status(id)
    }

    fn cancel(&self, id: &ActivationId) -> ApiResult<StatusAck> {
        self.api().cancel(id)
    }

    fn complete(&self, id: &ActivationId) -> ApiResult<StatusAck> {
        self.api().complete(id)
    }

    fn active(&self) -> ApiResult<Vec<ActiveActivation>> {
        if self.api().capabilities().active_activations {
            self.api().get_active_activations()
        } else {
            Ok(Vec::new())
        }
    }
}

// ---------------------------------------------------------------------------
// Test double

/// Scriptable backend for app tests: canned answers, a call log, and an optional failure switch.
#[cfg(test)]
pub mod mock {
    use super::*;

    #[derive(Default)]
    pub struct MockBackend {
        pub kind: Option<ProviderKind>,
        pub balance: f64,
        pub services: Vec<Service>,
        pub countries: Vec<CountryRow>,
        pub offers: Vec<OfferGroup>,
        pub activation: Option<Activation>,
        pub status: Option<ActivationStatus>,
        pub active: Vec<ActiveActivation>,
        /// When set, every call fails with this error (cloned by `Display`).
        pub fail_with: Mutex<Option<String>>,
        /// Overrides `status` at runtime (tests drive the poll loop through this).
        pub status_cell: Mutex<Option<ActivationStatus>>,
        pub calls: Mutex<Vec<String>>,
    }

    impl MockBackend {
        pub fn new(kind: ProviderKind) -> Self {
            Self {
                kind: Some(kind),
                balance: 10.0,
                services: vec![
                    Service {
                        code: ServiceCode::from("tg"),
                        name: "Telegram".into(),
                    },
                    Service {
                        code: ServiceCode::from("wa"),
                        name: "WhatsApp".into(),
                    },
                ],
                countries: vec![CountryRow {
                    key: CountryRef::Id(187),
                    name: "United States".into(),
                    code: "US".into(),
                    dial: Some("+1".into()),
                    price: 0.25,
                    count: 1000,
                }],
                offers: vec![OfferGroup {
                    name: ANY_OPERATOR.into(),
                    total: 1000,
                    tiers: vec![
                        OfferTier {
                            price: 0.25,
                            count: 500,
                            selector: OfferSelector::MaxPrice(0.25),
                        },
                        OfferTier {
                            price: 0.5,
                            count: 1000,
                            selector: OfferSelector::MaxPrice(0.5),
                        },
                    ],
                }],
                activation: Some(Activation::new("777", "12025550123")),
                status: Some(ActivationStatus::WaitCode),
                ..Default::default()
            }
        }

        pub fn failing(mut self, msg: &str) -> Self {
            self.fail_with = Mutex::new(Some(msg.into()));
            self
        }

        pub fn set_status(&self, status: ActivationStatus) -> &Self {
            *self.status_cell.lock().unwrap() = Some(status);
            self
        }

        fn log(&self, call: impl Into<String>) -> ApiResult<()> {
            self.calls.lock().unwrap().push(call.into());
            match self.fail_with.lock().unwrap().clone() {
                Some(msg) => Err(ApiError::Other(msg)),
                None => Ok(()),
            }
        }

        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Backend for MockBackend {
        fn kind(&self) -> ProviderKind {
            self.kind.unwrap_or(ProviderKind::HeroSms)
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                active_activations: true,
                ..Default::default()
            }
        }
        fn balance(&self) -> ApiResult<f64> {
            self.log("balance")?;
            Ok(self.balance)
        }
        fn services(&self) -> ApiResult<Vec<Service>> {
            self.log("services")?;
            Ok(self.services.clone())
        }
        fn countries_for(&self, service: &ServiceCode) -> ApiResult<Vec<CountryRow>> {
            self.log(format!("countries_for {service}"))?;
            Ok(self.countries.clone())
        }
        fn offers(
            &self,
            service: &ServiceCode,
            country: &CountryRef,
        ) -> ApiResult<Vec<OfferGroup>> {
            self.log(format!("offers {service} {country}"))?;
            Ok(self.offers.clone())
        }
        fn buy(
            &self,
            service: &ServiceCode,
            country: &CountryRef,
            selector: &OfferSelector,
        ) -> ApiResult<Activation> {
            self.log(format!("buy {service} {country} {selector:?}"))?;
            self.activation.clone().ok_or(ApiError::NoNumbers)
        }
        fn status(&self, id: &ActivationId) -> ApiResult<ActivationStatus> {
            self.log(format!("status {id}"))?;
            if let Some(s) = self.status_cell.lock().unwrap().clone() {
                return Ok(s);
            }
            self.status.clone().ok_or(ApiError::NoActivation)
        }
        fn cancel(&self, id: &ActivationId) -> ApiResult<StatusAck> {
            self.log(format!("cancel {id}"))?;
            Ok(StatusAck::Cancel)
        }
        fn complete(&self, id: &ActivationId) -> ApiResult<StatusAck> {
            self.log(format!("complete {id}"))?;
            Ok(StatusAck::Activation)
        }
        fn active(&self) -> ApiResult<Vec<ActiveActivation>> {
            self.log("active")?;
            Ok(self.active.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn provider_kind_names_round_trip() {
        let names: Vec<&str> = ProviderKind::ALL.iter().map(|k| k.name()).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
        for k in ProviderKind::ALL {
            assert!(k.env_key().ends_with("_API_KEY"));
            assert!(k.min_key_len() >= 8 && !k.key_hint().is_empty());
        }
        let json = serde_json::to_string(&ProviderKind::FiveSim).unwrap();
        assert_eq!(json, "\"FiveSim\"");
        let m: BTreeMap<ProviderKind, String> = serde_json::from_str(r#"{"HeroSms":"k"}"#).unwrap();
        assert_eq!(m[&ProviderKind::HeroSms], "k");
    }

    #[test]
    fn selectors_serialize() {
        let s = OfferSelector::Partner {
            id: "3170".into(),
            price: 0.765,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<OfferSelector>(&json).unwrap(), s);
    }

    #[test]
    fn mock_backend_logs_and_fails() {
        use mock::MockBackend;
        let m = MockBackend::new(ProviderKind::SmsBower);
        assert_eq!(m.balance().unwrap(), 10.0);
        assert_eq!(m.services().unwrap().len(), 2);
        let a = m
            .buy(
                &ServiceCode::from("tg"),
                &CountryRef::Id(187),
                &OfferSelector::MaxPrice(0.25),
            )
            .unwrap();
        assert_eq!(a.phone, "12025550123");
        assert_eq!(m.calls().len(), 3);
        let f = MockBackend::new(ProviderKind::HeroSms).failing("boom");
        assert!(matches!(f.balance(), Err(ApiError::Other(m)) if m == "boom"));
    }
}
