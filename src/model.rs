//! Pure data + deterministic logic ported from the Number Desk design mock.
//! No egui here so everything is unit-testable.

use std::time::{Duration, Instant};

pub const SERVICES: [&str; 10] = [
    "Telegram",
    "WhatsApp",
    "Google",
    "Instagram",
    "Discord",
    "Amazon",
    "Uber",
    "PayPal",
    "TikTok",
    "Twitter",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Country {
    pub name: &'static str,
    pub code: &'static str,
    pub dial: &'static str,
}

const fn c(name: &'static str, code: &'static str, dial: &'static str) -> Country {
    Country { name, code, dial }
}

pub const COUNTRIES: [Country; 10] = [
    c("United States", "US", "+1"),
    c("United Kingdom", "GB", "+44"),
    c("Netherlands", "NL", "+31"),
    c("Poland", "PL", "+48"),
    c("Indonesia", "ID", "+62"),
    c("India", "IN", "+91"),
    c("Ukraine", "UA", "+380"),
    c("Vietnam", "VN", "+84"),
    c("Philippines", "PH", "+63"),
    c("Kazakhstan", "KZ", "+7"),
];

pub const OPERATOR_POOL: [&str; 6] = [
    "Vodafone",
    "Orange",
    "Lycamobile",
    "NOS",
    "Lebara",
    "T-Mobile",
];

/// Same hash as the design's `hash()` — JS `(h * 31 + code) >>> 0`.
pub fn hash(s: &str) -> u32 {
    let mut h: u32 = 7;
    for ch in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(ch as u32);
    }
    h
}

pub fn price_for(provider: &str, service: &str, country_name: &str) -> f64 {
    let key = format!("{provider}{service}{country_name}");
    0.05 + (hash(&key) % 140) as f64 / 100.0
}

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

pub fn masked_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let head: String = key.chars().take(3).collect();
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}••••••••{tail}")
}

pub fn phone_for(id: u32, service: &str, country: &Country) -> String {
    let digits = (hash(&format!("{id}{service}")) % 90_000_000 + 10_000_000).to_string();
    format!(
        "{} {} {} {}",
        country.dial,
        &digits[0..3],
        &digits[3..6],
        &digits[6..]
    )
}

pub fn code_for(id: u32) -> String {
    let code = (hash(&format!("c{id}")) % 90_000 + 10_000).to_string();
    format!("{} {}", &code[0..2], &code[2..])
}

pub fn balance_for_key(key: &str) -> f64 {
    (hash(key) % 4000) as f64 / 100.0
}

#[derive(Clone, Debug, PartialEq)]
pub struct Provider {
    pub name: String,
    pub connected: bool,
    pub balance: f64,
    pub key: String,
}

impl Provider {
    pub fn new(name: &str, connected: bool, balance: f64, key: &str) -> Self {
        Self {
            name: name.into(),
            connected,
            balance,
            key: key.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Favorite {
    pub provider: String,
    pub service: String,
    pub country: Country,
    pub operator: String,
    pub price: f64,
}

impl Favorite {
    /// Equivalent of the design's `favKey` comparison.
    pub fn same(&self, other: &Favorite) -> bool {
        self.provider == other.provider
            && self.service == other.service
            && self.country.code == other.country.code
            && self.operator == other.operator
            && self.price == other.price
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub operator: String,
    pub price: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OfferTier {
    pub price: f64,
    pub count: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OfferGroup {
    pub name: String,
    pub total: u64,
    pub tiers: Vec<OfferTier>,
}

pub fn offers_for(provider: &str, service: &str, country: &Country) -> Vec<OfferGroup> {
    struct Op {
        name: String,
        tiers: u32,
        mult: f64,
    }
    let base = price_for(provider, service, country.name);
    let h = hash(&format!("{provider}{service}{}", country.name));
    let mut ops = vec![Op {
        name: "Any operator".into(),
        tiers: 4,
        mult: 1.0,
    }];
    for i in 0..3u32 {
        let op = OPERATOR_POOL[((h >> (i * 4)) % OPERATOR_POOL.len() as u32) as usize];
        if !ops.iter().any(|o| o.name == op) {
            ops.push(Op {
                name: op.into(),
                tiers: 1 + (h >> (i * 3)) % 3,
                mult: 0.85 + ((h >> i) % 30) as f64 / 100.0,
            });
        }
    }
    ops.iter()
        .enumerate()
        .map(|(oi, o)| {
            let tiers: Vec<OfferTier> = (0..o.tiers)
                .map(|k| {
                    let price = base * o.mult * (0.72 + k as f64 * 0.21);
                    let raw = hash(&format!("{}{}{}", o.name, k, service)) % 4000;
                    let count = (raw >> (oi as u32 + k)).max(3) as u64;
                    OfferTier {
                        price: (price * 10000.0).round() / 10000.0,
                        count,
                    }
                })
                .collect();
            OfferGroup {
                name: o.name.clone(),
                total: tiers.iter().map(|t| t.count).sum(),
                tiers,
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberStatus {
    Requesting,
    Waiting,
    Received,
    Expired,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Number {
    pub id: u32,
    pub provider: String,
    pub service: String,
    pub country: Country,
    pub phone: Option<String>,
    pub price: f64,
    pub status: NumberStatus,
    pub code: Option<String>,
    pub expires_at: Option<Instant>,
    pub total: Duration,
    pub cancel_pending: bool,
    pub cancelable_at: Option<Instant>,
}

impl Number {
    pub fn requesting(
        id: u32,
        provider: &str,
        service: &str,
        country: Country,
        price: f64,
    ) -> Self {
        Self {
            id,
            provider: provider.into(),
            service: service.into(),
            country,
            phone: None,
            price,
            status: NumberStatus::Requesting,
            code: None,
            expires_at: None,
            total: Duration::ZERO,
            cancel_pending: false,
            cancelable_at: None,
        }
    }

    pub fn time_left(&self, now: Instant) -> Duration {
        self.expires_at
            .map(|e| e.saturating_duration_since(now))
            .unwrap_or(Duration::ZERO)
    }

    /// 0..=1 fraction of the waiting window remaining.
    pub fn progress(&self, now: Instant) -> f32 {
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefKey {
    Sound,
    AutoCopy,
    Notify,
    StripDial,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub sound: bool,
    pub auto_copy: bool,
    pub notify: bool,
    pub strip_dial: bool,
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

/// Strip the dial prefix from a phone when the pref is on.
pub fn phone_for_clipboard(phone: &str, country: &Country, strip_dial: bool) -> String {
    if strip_dial && let Some(rest) = phone.strip_prefix(country.dial) {
        return rest.trim().to_string();
    }
    phone.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_js_reference() {
        // h = 7; "a" => 7*31 + 97 = 314
        assert_eq!(hash("a"), 314);
        assert_eq!(hash(""), 7);
        // wraps like `>>> 0`
        let long = "Hero SMSTelegramUnited States";
        assert_eq!(hash(long), hash(long));
    }

    #[test]
    fn price_in_expected_range() {
        for p in ["Hero SMS", "5SIM"] {
            for s in SERVICES {
                for c in COUNTRIES {
                    let v = price_for(p, s, c.name);
                    assert!((0.05..=1.44).contains(&v), "{v}");
                }
            }
        }
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(fmt_usd(12.4), "$12.40");
        assert_eq!(fmt_usd4(0.1569), "$0.1569");
        assert_eq!(fmt_thousands(0), "0");
        assert_eq!(fmt_thousands(999), "999");
        assert_eq!(fmt_thousands(1000), "1,000");
        assert_eq!(fmt_thousands(1234567), "1,234,567");
        assert_eq!(mmss(Duration::from_secs(741)), "12:21");
        assert_eq!(mmss(Duration::from_secs(5)), "0:05");
        assert_eq!(mmss(Duration::ZERO), "0:00");
        assert_eq!(masked_key("hk_9f3a2b1c8d"), "hk_••••••••1c8d");
        assert_eq!(masked_key(""), "");
    }

    #[test]
    fn phone_and_code_shapes() {
        let phone = phone_for(11, "Telegram", &COUNTRIES[0]);
        assert!(phone.starts_with("+1 "));
        let parts: Vec<&str> = phone.split(' ').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[1].len(), 3);
        assert_eq!(parts[2].len(), 3);
        assert_eq!(parts[3].len(), 2);
        let code = code_for(11);
        assert_eq!(code.len(), 6);
        assert_eq!(&code[2..3], " ");
    }

    #[test]
    fn offers_have_any_operator_first_and_four_tiers() {
        let groups = offers_for("Hero SMS", "Telegram", &COUNTRIES[0]);
        assert_eq!(groups[0].name, "Any operator");
        assert_eq!(groups[0].tiers.len(), 4);
        assert!(groups.len() >= 2 && groups.len() <= 4);
        for g in &groups {
            assert_eq!(g.total, g.tiers.iter().map(|t| t.count).sum::<u64>());
            for t in &g.tiers {
                assert!(t.count >= 3);
                assert!(t.price > 0.0);
            }
        }
        // operator names are unique
        let mut names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), groups.len());
    }

    #[test]
    fn favorite_identity() {
        let a = Favorite {
            provider: "Hero SMS".into(),
            service: "Telegram".into(),
            country: COUNTRIES[0],
            operator: "Any operator".into(),
            price: 0.1569,
        };
        let mut b = a.clone();
        assert!(a.same(&b));
        b.price = 0.16;
        assert!(!a.same(&b));
    }

    #[test]
    fn clipboard_phone_respects_strip_dial() {
        let nl = COUNTRIES[2];
        assert_eq!(
            phone_for_clipboard("+31 6 4471 0392", &nl, true),
            "6 4471 0392"
        );
        assert_eq!(
            phone_for_clipboard("+31 6 4471 0392", &nl, false),
            "+31 6 4471 0392"
        );
    }

    #[test]
    fn number_progress_and_time_left() {
        let now = Instant::now();
        let mut n = Number::requesting(1, "5SIM", "WhatsApp", COUNTRIES[2], 0.87);
        assert_eq!(n.progress(now), 0.0);
        n.status = NumberStatus::Waiting;
        n.total = Duration::from_secs(900);
        n.expires_at = Some(now + Duration::from_secs(450));
        assert!((n.progress(now) - 0.5).abs() < 0.01);
        assert_eq!(mmss(n.time_left(now)), "7:30");
        assert!(!n.dismissible());
        n.status = NumberStatus::Received;
        assert!(n.dismissible());
    }
}
