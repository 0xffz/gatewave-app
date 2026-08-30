//! App-level domain types shared by the state machine, the UI and persistence.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sms_activate::{ActivationId, CountryRef, ServiceCode};

use crate::backend::{OfferSelector, ProviderKind};

/// How long a freshly bought number stays valid when the provider does not say (sms-activate default).
pub const DEFAULT_NUMBER_TTL: Duration = Duration::from_secs(15 * 60);

// ---------------------------------------------------------------------------
// Formatting

pub fn fmt_usd(v: f64) -> String {
    format!("${v:.2}")
}

pub fn fmt_usd4(v: f64) -> String {
    format!("${v:.4}")
}

/// `toLocaleString()` for integers: thousands separators.
pub fn fmt_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// `m:ss`, clamped at zero.
pub fn mmss(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

/// Two-letter badge for a country: the ISO code when known, else initials of the name.
pub fn country_badge(name: &str, iso: Option<&str>) -> String {
    if let Some(iso) = iso.filter(|s| !s.trim().is_empty()) {
        return iso.trim().to_ascii_uppercase();
    }
    let words: Vec<&str> = name.split_whitespace().collect();
    let badge: String = match words.as_slice() {
        [] => "??".into(),
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    badge.to_ascii_uppercase()
}

/// `hk_••••••••1c8d` — first three and last four characters of an API key.
pub fn masked_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let head: String = key.chars().take(3).collect();
    let tail_len = key.chars().count().saturating_sub(4);
    let tail: String = key.chars().skip(tail_len).collect();
    format!("{head}••••••••{tail}")
}

/// A phone number as understood by libphonenumber (`phonenumber` crate).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneParts {
    /// `+14158302247`
    pub e164: String,
    /// `+1 415 830 2247` — international format, groups separated by spaces.
    pub international: String,
    /// `415 830 2247` — the international grouping without the country code.
    pub national: String,
    /// Country calling code (`1`, `31`, `380` …).
    pub country_code: u16,
}

/// Parses a provider phone number. Providers send E.164 digits with or without the leading `+`.
pub fn parse_phone(raw: &str) -> Option<PhoneParts> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let number = phonenumber::parse(None, format!("+{digits}")).ok()?;
    let country_code = number.country().code();
    let international = number
        .format()
        .mode(phonenumber::Mode::International)
        .to_string()
        .replace('-', " ");
    let prefix = format!("+{country_code}");
    let national = international
        .strip_prefix(&prefix)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| digits[prefix.len() - 1..].to_owned());
    Some(PhoneParts {
        e164: number.format().mode(phonenumber::Mode::E164).to_string(),
        international,
        national,
        country_code,
    })
}

/// How a number is shown: libphonenumber's international grouping, or the raw value with a
/// leading `+` when it cannot be parsed.
pub fn phone_display(raw: &str) -> String {
    match parse_phone(raw) {
        Some(p) => p.international,
        None => {
            let t = raw.trim();
            if t.starts_with('+') {
                t.to_owned()
            } else {
                format!("+{t}")
            }
        }
    }
}

/// What goes to the clipboard: the international number, or — with the "strip dial" pref — the
/// national part without the country code (`6 4471 0392` for `+31 6 4471 0392`). Falls back to
/// trimming a known dialling prefix (or just the `+`) when the number cannot be parsed.
pub fn phone_for_clipboard(phone: &str, dial: Option<&str>, strip_dial: bool) -> String {
    if let Some(p) = parse_phone(phone) {
        return if strip_dial {
            p.national
        } else {
            p.international
        };
    }
    if !strip_dial {
        return phone.to_string();
    }
    let bare = phone.trim().trim_start_matches('+');
    if let Some(dial) = dial {
        let d = dial.trim().trim_start_matches('+');
        if !d.is_empty()
            && let Some(rest) = bare.strip_prefix(d)
        {
            return rest.trim_start().to_string();
        }
    }
    bare.to_string()
}

// ---------------------------------------------------------------------------
// Preferences

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefKey {
    Sound,
    AutoCopy,
    Notify,
    StripDial,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub sound: bool,
    pub auto_copy: bool,
    pub notify: bool,
    pub strip_dial: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            sound: true,
            auto_copy: false,
            notify: true,
            strip_dial: false,
        }
    }
}

impl Prefs {
    pub fn get(&self, k: PrefKey) -> bool {
        match k {
            PrefKey::Sound => self.sound,
            PrefKey::AutoCopy => self.auto_copy,
            PrefKey::Notify => self.notify,
            PrefKey::StripDial => self.strip_dial,
        }
    }

    pub fn toggle(&mut self, k: PrefKey) {
        match k {
            PrefKey::Sound => self.sound = !self.sound,
            PrefKey::AutoCopy => self.auto_copy = !self.auto_copy,
            PrefKey::Notify => self.notify = !self.notify,
            PrefKey::StripDial => self.strip_dial = !self.strip_dial,
        }
    }
}

pub const PREF_DEFS: [(PrefKey, &str, &str); 4] = [
    (
        PrefKey::Sound,
        "Sound when code is received",
        "Short chime on every incoming code",
    ),
    (
        PrefKey::AutoCopy,
        "Auto-copy received code",
        "Code goes straight to the clipboard",
    ),
    (
        PrefKey::Notify,
        "Show snackbar on code received",
        "Errors are always shown",
    ),
    (
        PrefKey::StripDial,
        "Copy number without country code",
        "e.g. 6 4471 0392 instead of +31 6 4471 0392",
    ),
];

// ---------------------------------------------------------------------------
// Favorites

/// A saved provider · service · country · offer combination.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Favorite {
    pub provider: ProviderKind,
    pub service: ServiceCode,
    pub service_name: String,
    pub country: CountryRef,
    pub country_name: String,
    /// Two-letter badge (ISO code or initials).
    pub country_code: String,
    /// Dialling prefix when known (`+31`), carried onto numbers bought from the favorite.
    #[serde(default)]
    pub dial: Option<String>,
    /// Offer group name (`Any operator`, `vodafone`, `Partner 3170` …).
    pub operator: String,
    pub price: f64,
    pub selector: OfferSelector,
}

impl Favorite {
    /// Identity used for star toggling (provider · service · country · operator · price).
    pub fn same(&self, other: &Favorite) -> bool {
        self.provider == other.provider
            && self.service == other.service
            && self.country == other.country
            && self.operator == other.operator
            && self.price == other.price
    }
}

// ---------------------------------------------------------------------------
// Numbers (activations)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberStatus {
    /// `get_number` in flight.
    Requesting,
    /// Number assigned, polling for the SMS.
    Waiting,
    /// Code received.
    Received,
    /// Timed out without a code.
    Expired,
    /// Cancelled (refunded when the provider allows).
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Number {
    /// Local id (unique within the app, stable across restarts).
    pub id: u32,
    pub provider: ProviderKind,
    /// The provider's activation/order id once assigned.
    pub remote_id: Option<ActivationId>,
    pub service: ServiceCode,
    pub service_name: String,
    pub country: CountryRef,
    pub country_name: String,
    /// Dialling prefix when known (`+31`), used by the "strip dial" pref.
    pub dial: Option<String>,
    pub phone: Option<String>,
    pub price: f64,
    pub status: NumberStatus,
    pub code: Option<String>,
    pub expires_at: Option<SystemTime>,
    #[serde(default)]
    pub total: Duration,
    /// Earliest moment a cancel is allowed (e.g. SMSBower's two-minute grace period).
    pub cancelable_at: Option<SystemTime>,
    /// A cancel request is in flight.
    #[serde(skip)]
    pub cancel_pending: bool,
    /// A status poll is in flight.
    #[serde(skip)]
    pub polling: bool,
}

impl Number {
    #[allow(clippy::too_many_arguments)]
    pub fn requesting(
        id: u32,
        provider: ProviderKind,
        service: ServiceCode,
        service_name: impl Into<String>,
        country: CountryRef,
        country_name: impl Into<String>,
        dial: Option<String>,
        price: f64,
    ) -> Self {
        Self {
            id,
            provider,
            remote_id: None,
            service,
            service_name: service_name.into(),
            country,
            country_name: country_name.into(),
            dial,
            phone: None,
            price,
            status: NumberStatus::Requesting,
            code: None,
            expires_at: None,
            total: Duration::ZERO,
            cancelable_at: None,
            cancel_pending: false,
            polling: false,
        }
    }

    pub fn time_left(&self, now: SystemTime) -> Duration {
        self.expires_at
            .and_then(|e| e.duration_since(now).ok())
            .unwrap_or(Duration::ZERO)
    }

    /// 0..=1 fraction of the waiting window remaining.
    pub fn progress(&self, now: SystemTime) -> f32 {
        if self.total.is_zero() {
            return 0.0;
        }
        (self.time_left(now).as_secs_f32() / self.total.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn dismissible(&self) -> bool {
        matches!(
            self.status,
            NumberStatus::Expired | NumberStatus::Cancelled | NumberStatus::Received
        )
    }

    /// Still needs the provider's attention (polling / expiry tracking).
    pub fn is_live(&self) -> bool {
        matches!(
            self.status,
            NumberStatus::Requesting | NumberStatus::Waiting
        )
    }

    /// `Telegram · United States · Hero SMS`
    pub fn meta_line(&self) -> String {
        format!(
            "{} · {} · {}",
            self.service_name,
            self.country_name,
            self.provider.name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_helpers() {
        assert_eq!(fmt_usd(12.4), "$12.40");
        assert_eq!(fmt_usd4(0.1569), "$0.1569");
        assert_eq!(fmt_thousands(1234567), "1,234,567");
        assert_eq!(fmt_thousands(999), "999");
        assert_eq!(mmss(Duration::from_secs(741)), "12:21");
        assert_eq!(country_badge("United States", None), "US");
        assert_eq!(country_badge("England", Some("gb")), "GB");
        assert_eq!(country_badge("india", None), "IN");
        assert_eq!(masked_key("hk_9f3a2b1c8d"), "hk_••••••••1c8d");
        assert_eq!(masked_key("ab"), "ab••••••••ab");
        assert_eq!(masked_key(""), "");
    }

    #[test]
    fn phone_parsing_and_display() {
        let p = parse_phone("14158302247").unwrap();
        assert_eq!(p.country_code, 1);
        assert_eq!(p.e164, "+14158302247");
        assert_eq!(p.international, "+1 415 830 2247");
        assert_eq!(p.national, "415 830 2247");
        let nl = parse_phone("+31644710392").unwrap();
        assert_eq!(nl.international, "+31 6 44710392");
        assert_eq!(nl.national, "6 44710392");
        let ua = parse_phone("380501234567").unwrap();
        assert_eq!(ua.country_code, 380);
        assert!(ua.national.starts_with("50"));
        assert_eq!(phone_display("+1 415 830 2247"), "+1 415 830 2247");
        assert_eq!(phone_display("garbage"), "+garbage");
        assert!(parse_phone("").is_none());
    }

    #[test]
    fn clipboard_phone_rules() {
        assert_eq!(
            phone_for_clipboard("+31644710392", None, true),
            "6 44710392"
        );
        assert_eq!(
            phone_for_clipboard("31644710392", Some("+31"), false),
            "+31 6 44710392"
        );
        assert_eq!(
            phone_for_clipboard("447350690992", None, true),
            "7350 690992"
        );
        assert_eq!(
            phone_for_clipboard("12025550123", None, true),
            "202 555 0123"
        );
        // Unparseable numbers fall back to prefix trimming.
        assert_eq!(
            phone_for_clipboard("+999 12 34", Some("+999"), true),
            "12 34"
        );
        assert_eq!(phone_for_clipboard("+999 12 34", None, false), "+999 12 34");
    }

    #[test]
    fn number_lifecycle_helpers() {
        let now = SystemTime::now();
        let mut n = Number::requesting(
            1,
            ProviderKind::HeroSms,
            ServiceCode::from("tg"),
            "Telegram",
            CountryRef::Id(187),
            "United States",
            None,
            0.25,
        );
        assert!(n.is_live() && !n.dismissible());
        assert_eq!(n.progress(now), 0.0);
        n.status = NumberStatus::Waiting;
        n.total = DEFAULT_NUMBER_TTL;
        n.expires_at = Some(now + Duration::from_secs(450));
        assert!((n.progress(now) - 0.5).abs() < 0.01);
        n.status = NumberStatus::Received;
        assert!(n.dismissible() && !n.is_live());
        assert_eq!(n.meta_line(), "Telegram · United States · Hero SMS");
        let json = serde_json::to_string(&n).unwrap();
        let back: Number = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, NumberStatus::Received);
        assert_eq!(back.country, CountryRef::Id(187));
    }

    #[test]
    fn prefs_and_favorites() {
        let mut p = Prefs::default();
        assert!(p.sound && p.notify && !p.auto_copy);
        p.toggle(PrefKey::AutoCopy);
        assert!(p.get(PrefKey::AutoCopy));
        let f = Favorite {
            provider: ProviderKind::FiveSim,
            service: ServiceCode::from("telegram"),
            service_name: "telegram".into(),
            country: CountryRef::Slug("england".into()),
            country_name: "England".into(),
            country_code: "GB".into(),
            dial: Some("+44".into()),
            operator: "vodafone".into(),
            price: 0.8,
            selector: OfferSelector::Operator("vodafone".into()),
        };
        let mut g = f.clone();
        assert!(f.same(&g));
        g.price = 0.9;
        assert!(!f.same(&g));
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<Favorite>(&json).unwrap(), f);
    }
}
