//! Application state and the simulated behaviour from the design's `Component` class.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::model::*;
use crate::sim::{Event, Rng, Scheduler, SimConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    New,
    Favorites,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnackKind {
    Error,
    Success,
    Info,
}

#[derive(Clone, Debug)]
pub struct Snack {
    pub msg: String,
    pub kind: SnackKind,
    token: u64,
}

/// Everything the UI can ask the app to do. Collected while drawing, applied afterwards.
#[derive(Clone, Debug)]
pub enum Action {
    GoScreen(Screen),
    GoStep(u8),
    PickProvider(String),
    PickService(String),
    PickCountry(Country),
    PickOffer(Offer),
    ToggleFav(Favorite),
    ToggleSort,
    SetSearch(String),
    RequestNumber,
    RequestFav(usize),
    RemoveFav(usize),
    CopyPhone(u32),
    CopyCode(u32),
    DismissNumber(u32),
    CancelNumber(u32),
    TogglePref(PrefKey),
    SetKeyInput(String, String),
    Connect(String),
    Disconnect(String),
    DismissSnack,
}

pub struct StepInfo {
    pub num: u8,
    pub label: &'static str,
    pub value: Option<String>,
    pub active: bool,
    pub reachable: bool,
}

pub struct App {
    pub screen: Screen,
    pub step: u8,
    pub provider: Option<String>,
    pub service: Option<String>,
    pub country: Option<Country>,
    pub offer: Option<Offer>,
    pub loading_services: bool,
    pub loading_countries: bool,
    pub loading_offers: bool,
    pub snack: Option<Snack>,
    pub copied: Option<String>,
    pub key_inputs: HashMap<String, String>,
    pub connecting: Option<String>,
    pub search: String,
    pub sort_dir: Option<SortDir>,
    pub prefs: Prefs,
    pub favorites: Vec<Favorite>,
    pub providers: Vec<Provider>,
    pub numbers: Vec<Number>,
    pub sim: SimConfig,
    pub now: Instant,
    seq: u32,
    token_seq: u64,
    copied_token: u64,
    sched: Scheduler,
    rng: Rng,
    clipboard: Option<String>,
}

const LOAD_DELAY: Duration = Duration::from_millis(950);
const SNACK_TTL: Duration = Duration::from_millis(4200);
const COPIED_TTL: Duration = Duration::from_millis(1600);
const NUMBER_TTL: Duration = Duration::from_secs(900);

impl App {
    pub fn new() -> Self {
        let now = Instant::now();
        let mut n1 = Number::requesting(1, "Hero SMS", "Telegram", COUNTRIES[0], 0.42);
        n1.phone = Some("+1 415 830 2247".into());
        n1.status = NumberStatus::Received;
        n1.code = Some("39 284".into());
        let mut n2 = Number::requesting(2, "5SIM", "WhatsApp", COUNTRIES[2], 0.87);
        n2.phone = Some("+31 6 4471 0392".into());
        n2.status = NumberStatus::Waiting;
        n2.expires_at = Some(now + Duration::from_secs(741));
        n2.total = NUMBER_TTL;
        let mut n3 = Number::requesting(3, "Tiger SMS", "Google", COUNTRIES[5], 0.11);
        n3.phone = Some("+91 90042 17765".into());
        n3.status = NumberStatus::Expired;

        Self {
            screen: Screen::New,
            step: 1,
            provider: None,
            service: None,
            country: None,
            offer: None,
            loading_services: false,
            loading_countries: false,
            loading_offers: false,
            snack: None,
            copied: None,
            key_inputs: HashMap::new(),
            connecting: None,
            search: String::new(),
            sort_dir: None,
            prefs: Prefs {
                sound: true,
                auto_copy: false,
                notify: true,
                strip_dial: false,
            },
            favorites: vec![
                Favorite {
                    provider: "Hero SMS".into(),
                    service: "Telegram".into(),
                    country: COUNTRIES[0],
                    operator: "Any operator".into(),
                    price: 0.1569,
                },
                Favorite {
                    provider: "5SIM".into(),
                    service: "WhatsApp".into(),
                    country: COUNTRIES[2],
                    operator: "Vodafone".into(),
                    price: 0.4412,
                },
            ],
            providers: vec![
                Provider::new("Hero SMS", true, 12.4, "hk_9f3a2b1c8d"),
                Provider::new("5SIM", true, 3.85, "5s_77e1c0aa42"),
                Provider::new("Tiger SMS", true, 27.1, "tg_b52d99e013"),
                Provider::new("Grizzly SMS", false, 0.0, ""),
                Provider::new("SMSBower", false, 0.0, ""),
            ],
            numbers: vec![n1, n2, n3],
            sim: SimConfig::default(),
            now,
            seq: 10,
            token_seq: 0,
            copied_token: 0,
            sched: Scheduler::default(),
            rng: Rng::from_time(),
            clipboard: None,
        }
    }

    // ------------------------------------------------------------------
    // Per-frame plumbing

    /// Advance the clock, expire numbers, and run any due simulated callbacks.
    pub fn tick(&mut self) {
        self.now = Instant::now();
        let now = self.now;
        for n in &mut self.numbers {
            if n.status == NumberStatus::Waiting && n.expires_at.is_some_and(|e| e <= now) {
                n.status = NumberStatus::Expired;
            }
        }
        for ev in self.sched.drain_due(now) {
            self.handle_event(ev);
        }
    }

    pub fn take_clipboard(&mut self) -> Option<String> {
        self.clipboard.take()
    }

    /// True while something on screen is animating (skeletons, countdowns, blink).
    pub fn animating(&self) -> bool {
        self.loading_services
            || self.loading_countries
            || self.loading_offers
            || self.connecting.is_some()
            || self
                .numbers
                .iter()
                .any(|n| matches!(n.status, NumberStatus::Requesting | NumberStatus::Waiting))
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.sched.next_due()
    }

    fn next_token(&mut self) -> u64 {
        self.token_seq += 1;
        self.token_seq
    }

    fn handle_event(&mut self, ev: Event) {
        match ev {
            Event::ServicesLoaded => self.loading_services = false,
            Event::CountriesLoaded => self.loading_countries = false,
            Event::OffersLoaded => self.loading_offers = false,
            Event::RequestResolved { id } => self.resolve_request(id),
            Event::CodeArrives { id } => self.arrive_code(id),
            Event::CancelResolved { id } => self.resolve_cancel(id),
            Event::ConnectResolved { provider } => self.resolve_connect(&provider),
            Event::SnackExpire { token } => {
                if self.snack.as_ref().is_some_and(|s| s.token == token) {
                    self.snack = None;
                }
            }
            Event::CopiedExpire { token } => {
                if self.copied_token == token {
                    self.copied = None;
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Actions from the UI

    pub fn apply(&mut self, action: Action) {
        match action {
            Action::GoScreen(s) => self.screen = s,
            Action::GoStep(n) => {
                if self.step_reachable(n) {
                    self.screen = Screen::New;
                    self.step = n;
                    self.search.clear();
                }
            }
            Action::PickProvider(name) => self.pick_provider(&name),
            Action::PickService(s) => self.pick_service(&s),
            Action::PickCountry(c) => self.pick_country(c),
            Action::PickOffer(o) => self.offer = Some(o),
            Action::ToggleFav(f) => self.toggle_fav(f),
            Action::ToggleSort => {
                self.sort_dir = match self.sort_dir {
                    None => Some(SortDir::Asc),
                    Some(SortDir::Asc) => Some(SortDir::Desc),
                    Some(SortDir::Desc) => None,
                }
            }
            Action::SetSearch(s) => self.search = s,
            Action::RequestNumber => {
                if let (Some(p), Some(s), Some(c)) =
                    (self.provider.clone(), self.service.clone(), self.country)
                {
                    let price = self.offer.as_ref().map(|o| o.price);
                    self.request_for(&p, &s, c, price);
                }
            }
            Action::RequestFav(i) => {
                if let Some(f) = self.favorites.get(i).cloned() {
                    self.request_for(&f.provider, &f.service, f.country, Some(f.price));
                }
            }
            Action::RemoveFav(i) => {
                if i < self.favorites.len() {
                    self.favorites.remove(i);
                }
            }
            Action::CopyPhone(id) => {
                if let Some(n) = self.numbers.iter().find(|n| n.id == id)
                    && let Some(phone) = &n.phone
                {
                    let text = phone_for_clipboard(phone, &n.country, self.prefs.strip_dial);
                    self.copy(text, format!("{id}-p"));
                }
            }
            Action::CopyCode(id) => {
                if let Some(code) = self
                    .numbers
                    .iter()
                    .find(|n| n.id == id)
                    .and_then(|n| n.code.clone())
                {
                    self.copy(code.replace(' ', ""), format!("{id}-c"));
                }
            }
            Action::DismissNumber(id) => self.numbers.retain(|n| n.id != id),
            Action::CancelNumber(id) => self.cancel_number(id),
            Action::TogglePref(k) => self.prefs.toggle(k),
            Action::SetKeyInput(name, v) => {
                self.key_inputs.insert(name, v);
            }
            Action::Connect(name) => self.connect_provider(&name),
            Action::Disconnect(name) => {
                if let Some(p) = self.providers.iter_mut().find(|p| p.name == name) {
                    p.connected = false;
                    p.key.clear();
                    p.balance = 0.0;
                }
                self.toast(format!("{name} disconnected."), SnackKind::Info);
            }
            Action::DismissSnack => self.snack = None,
        }
    }

    // ------------------------------------------------------------------
    // Behaviour

    pub fn toast(&mut self, msg: impl Into<String>, kind: SnackKind) {
        let token = self.next_token();
        self.snack = Some(Snack {
            msg: msg.into(),
            kind,
            token,
        });
        self.sched.schedule(SNACK_TTL, Event::SnackExpire { token });
    }

    fn copy(&mut self, text: String, key: String) {
        self.clipboard = Some(text);
        self.copied = Some(key);
        let token = self.next_token();
        self.copied_token = token;
        self.sched
            .schedule(COPIED_TTL, Event::CopiedExpire { token });
    }

    fn pick_provider(&mut self, name: &str) {
        let Some(p) = self.providers.iter().find(|p| p.name == name) else {
            return;
        };
        if !p.connected {
            let msg = format!("{} is not connected. Add its API key in Settings.", p.name);
            self.toast(msg, SnackKind::Info);
            return;
        }
        self.provider = Some(p.name.clone());
        self.service = None;
        self.country = None;
        self.step = 2;
        self.loading_services = true;
        self.screen = Screen::New;
        self.search.clear();
        self.sched.schedule(LOAD_DELAY, Event::ServicesLoaded);
    }

    fn pick_service(&mut self, s: &str) {
        self.service = Some(s.to_string());
        self.country = None;
        self.step = 3;
        self.loading_countries = true;
        self.search.clear();
        self.sched.schedule(LOAD_DELAY, Event::CountriesLoaded);
    }

    fn pick_country(&mut self, c: Country) {
        self.country = Some(c);
        self.offer = None;
        self.step = 4;
        self.loading_offers = true;
        self.search.clear();
        self.sched.schedule(LOAD_DELAY, Event::OffersLoaded);
    }

    pub fn is_fav(&self, fav: &Favorite) -> bool {
        self.favorites.iter().any(|f| f.same(fav))
    }

    fn toggle_fav(&mut self, fav: Favorite) {
        if self.is_fav(&fav) {
            self.favorites.retain(|f| !f.same(&fav));
        } else {
            self.favorites.push(fav);
        }
    }

    fn request_for(
        &mut self,
        provider: &str,
        service: &str,
        country: Country,
        price_override: Option<f64>,
    ) {
        let price = price_override.unwrap_or_else(|| price_for(provider, service, country.name));
        self.seq += 1;
        let id = self.seq;
        self.numbers
            .insert(0, Number::requesting(id, provider, service, country, price));
        let delay = Duration::from_secs_f64(1.1 + self.rng.unit() * 0.7);
        self.sched.schedule(delay, Event::RequestResolved { id });
    }

    fn resolve_request(&mut self, id: u32) {
        let Some(idx) = self.numbers.iter().position(|n| n.id == id) else {
            return;
        };
        if self.rng.unit() * 100.0 < self.sim.fail_rate_pct {
            let n = self.numbers.remove(idx);
            let msg = format!(
                "{}: no numbers available for {} · {}. Try another country.",
                n.provider, n.service, n.country.name
            );
            self.toast(msg, SnackKind::Error);
            return;
        }
        let n = &mut self.numbers[idx];
        n.phone = Some(phone_for(id, &n.service, &n.country));
        n.status = NumberStatus::Waiting;
        n.expires_at = Some(self.now + NUMBER_TTL);
        n.total = NUMBER_TTL;
        let delay = self.sim.code_delay.mul_f64(0.7 + self.rng.unit() * 0.8);
        self.sched.schedule(delay, Event::CodeArrives { id });
    }

    fn arrive_code(&mut self, id: u32) {
        let Some(n) = self.numbers.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if n.status != NumberStatus::Waiting {
            return;
        }
        let code = code_for(id);
        n.status = NumberStatus::Received;
        n.code = Some(code.clone());
        let phone = n.phone.clone().unwrap_or_default();
        if self.prefs.notify {
            self.toast(format!("Code received for {phone}"), SnackKind::Success);
        }
        // prefs.sound: the design plays a short chime — no audio in this mock-up.
        if self.prefs.auto_copy {
            self.clipboard = Some(code.replace(' ', ""));
        }
    }

    fn cancel_number(&mut self, id: u32) {
        let now = self.now;
        let Some(n) = self.numbers.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if n.cancelable_at.is_some_and(|t| t > now) || n.cancel_pending {
            return;
        }
        n.cancel_pending = true;
        self.sched
            .schedule(Duration::from_millis(900), Event::CancelResolved { id });
    }

    fn resolve_cancel(&mut self, id: u32) {
        let r = self.rng.unit();
        let now = self.now;
        let Some(n) = self.numbers.iter_mut().find(|n| n.id == id) else {
            return;
        };
        n.cancel_pending = false;
        let (provider, price) = (n.provider.clone(), n.price);
        if r < 0.5 {
            n.status = NumberStatus::Cancelled;
            self.toast(
                format!(
                    "Number cancelled · {} refunded to {provider}",
                    fmt_usd(price)
                ),
                SnackKind::Success,
            );
        } else if r < 0.8 {
            self.toast(
                format!("{provider}: this number cannot be cancelled."),
                SnackKind::Error,
            );
        } else {
            n.cancelable_at = Some(now + Duration::from_secs(120));
            self.toast(
                format!("{provider}: number can be cancelled in 2:00."),
                SnackKind::Info,
            );
        }
    }

    fn connect_provider(&mut self, name: &str) {
        let key = self
            .key_inputs
            .get(name)
            .map(|k| k.trim().to_string())
            .unwrap_or_default();
        if key.chars().count() < 8 {
            self.toast(
                format!("API key looks too short. Check it in your {name} dashboard."),
                SnackKind::Error,
            );
            return;
        }
        self.connecting = Some(name.to_string());
        self.sched.schedule(
            Duration::from_millis(1300),
            Event::ConnectResolved {
                provider: name.to_string(),
            },
        );
    }

    fn resolve_connect(&mut self, name: &str) {
        self.connecting = None;
        let key = self
            .key_inputs
            .get(name)
            .map(|k| k.trim().to_string())
            .unwrap_or_default();
        if key.to_lowercase().contains("bad") {
            self.toast(format!("{name}: invalid API key."), SnackKind::Error);
            return;
        }
        if let Some(p) = self.providers.iter_mut().find(|p| p.name == name) {
            p.connected = true;
            p.balance = balance_for_key(&key);
            p.key = key;
        }
        self.toast(format!("{name} connected."), SnackKind::Success);
    }

    // ------------------------------------------------------------------
    // Demo scenes (NUMBER_DESK_SCENE=…) — jump straight to a screen for demos/screenshots.

    /// Runs simulated callbacks due within `d` without waiting (skips skeleton loaders).
    fn fast_forward(&mut self, d: Duration) {
        for ev in self.sched.drain_due(Instant::now() + d) {
            self.handle_event(ev);
        }
    }

    pub fn apply_scene(&mut self, scene: &str) {
        let load = Duration::from_secs(2);
        match scene {
            "step2" => {
                self.pick_provider("Hero SMS");
                self.fast_forward(load);
            }
            "step3" => {
                self.apply_scene("step2");
                self.pick_service("Telegram");
                self.fast_forward(load);
            }
            "step4" => {
                self.apply_scene("step3");
                self.pick_country(COUNTRIES[0]);
                self.fast_forward(load);
            }
            "offer" => {
                self.apply_scene("step4");
                if let Some(g) = self.offer_groups().first()
                    && let Some(t) = g.tiers.get(1)
                {
                    self.offer = Some(Offer {
                        operator: g.name.clone(),
                        price: t.price,
                    });
                }
            }
            "requesting" => {
                self.apply_scene("offer");
                self.apply(Action::RequestNumber);
            }
            "favorites" => self.screen = Screen::Favorites,
            "settings" => {
                self.screen = Screen::Settings;
                self.key_inputs
                    .insert("Grizzly SMS".into(), "gz_4c1d9e7a02".into());
            }
            "connecting" => {
                self.apply_scene("settings");
                self.connect_provider("Grizzly SMS");
            }
            "snack" => self.toast(
                "Hero SMS: no numbers available for Telegram · United States. Try another country.",
                SnackKind::Error,
            ),
            "empty" => {
                self.numbers.clear();
                self.favorites.clear();
                self.screen = Screen::Favorites;
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Derived view data

    pub fn step_reachable(&self, n: u8) -> bool {
        match n {
            1 => true,
            2 => self.provider.is_some(),
            3 => self.service.is_some(),
            4 => self.country.is_some(),
            _ => false,
        }
    }

    pub fn steps(&self) -> [StepInfo; 4] {
        let defs: [(u8, &'static str, Option<String>); 4] = [
            (1, "Provider", self.provider.clone()),
            (2, "Service", self.service.clone()),
            (3, "Country", self.country.map(|c| c.name.to_string())),
            (
                4,
                "Offer",
                self.offer
                    .as_ref()
                    .map(|o| format!("{} · {}", fmt_usd(o.price), o.operator)),
            ),
        ];
        defs.map(|(num, label, value)| StepInfo {
            num,
            label,
            value,
            active: self.screen == Screen::New && self.step == num,
            reachable: self.step_reachable(num),
        })
    }

    pub fn step_title(&self) -> &'static str {
        match self.step {
            1 => "Choose a provider",
            2 => "Choose a service",
            3 => "Choose a country",
            _ => "Choose an offer",
        }
    }

    pub fn search_placeholder(&self) -> &'static str {
        match self.step {
            1 => "Search providers",
            2 => "Search services",
            3 => "Search countries",
            _ => "Search operators",
        }
    }

    fn matches(&self, s: &str) -> bool {
        let q = self.search.trim().to_lowercase();
        q.is_empty() || s.to_lowercase().contains(&q)
    }

    pub fn provider_rows(&self) -> Vec<&Provider> {
        self.providers
            .iter()
            .filter(|p| self.matches(&p.name))
            .collect()
    }

    pub fn service_rows(&self) -> Vec<&'static str> {
        SERVICES
            .iter()
            .copied()
            .filter(|s| self.matches(s))
            .collect()
    }

    pub fn country_rows(&self) -> Vec<Country> {
        let (Some(p), Some(s)) = (&self.provider, &self.service) else {
            return Vec::new();
        };
        let mut list: Vec<Country> = COUNTRIES
            .iter()
            .copied()
            .filter(|c| self.matches(c.name) || self.matches(c.code))
            .collect();
        if let Some(dir) = self.sort_dir {
            list.sort_by(|a, b| {
                let d = price_for(p, s, a.name)
                    .partial_cmp(&price_for(p, s, b.name))
                    .unwrap_or(std::cmp::Ordering::Equal);
                if dir == SortDir::Asc { d } else { d.reverse() }
            });
        }
        list
    }

    pub fn country_price(&self, c: &Country) -> f64 {
        match (&self.provider, &self.service) {
            (Some(p), Some(s)) => price_for(p, s, c.name),
            _ => 0.0,
        }
    }

    pub fn offer_groups(&self) -> Vec<OfferGroup> {
        if self.step != 4 || self.loading_offers {
            return Vec::new();
        }
        let (Some(p), Some(s), Some(c)) = (&self.provider, &self.service, &self.country) else {
            return Vec::new();
        };
        offers_for(p, s, c)
            .into_iter()
            .filter(|g| self.matches(&g.name))
            .collect()
    }

    pub fn balances(&self) -> impl Iterator<Item = &Provider> {
        self.providers.iter().filter(|p| p.connected)
    }

    /// (line, via, price) for the sticky request bar, when an offer is selected.
    pub fn summary(&self) -> Option<(String, String, String)> {
        let offer = self.offer.as_ref()?;
        let line = format!(
            "{} · {}",
            self.service.as_deref().unwrap_or(""),
            self.country.map(|c| c.name).unwrap_or("")
        );
        let via = format!(
            "via {} · {}",
            self.provider.as_deref().unwrap_or(""),
            offer.operator
        );
        Some((line, via, fmt_usd4(offer.price)))
    }

    pub fn copied_is(&self, key: &str) -> bool {
        self.copied.as_deref() == Some(key)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picking_a_disconnected_provider_only_toasts() {
        let mut app = App::new();
        app.apply(Action::PickProvider("Grizzly SMS".into()));
        assert_eq!(app.step, 1);
        assert!(app.provider.is_none());
        assert!(
            app.snack
                .as_ref()
                .is_some_and(|s| s.kind == SnackKind::Info)
        );
    }

    #[test]
    fn wizard_flow_advances_steps_and_loads() {
        let mut app = App::new();
        app.apply(Action::PickProvider("Hero SMS".into()));
        assert_eq!(app.step, 2);
        assert!(app.loading_services);
        assert!(app.step_reachable(2) && !app.step_reachable(3));
        app.apply(Action::PickService("Telegram".into()));
        assert_eq!(app.step, 3);
        app.apply(Action::PickCountry(COUNTRIES[0]));
        assert_eq!(app.step, 4);
        assert!(app.loading_offers);
        assert!(app.offer_groups().is_empty());
        app.handle_event(Event::OffersLoaded);
        assert!(!app.offer_groups().is_empty());
        assert!(app.summary().is_none());
        let o = Offer {
            operator: "Any operator".into(),
            price: 0.5,
        };
        app.apply(Action::PickOffer(o));
        assert!(app.summary().is_some());
    }

    #[test]
    fn request_creates_requesting_number_then_resolves() {
        let mut app = App::new();
        app.sim.fail_rate_pct = 0.0;
        app.apply(Action::RequestFav(0));
        assert_eq!(app.numbers[0].status, NumberStatus::Requesting);
        let id = app.numbers[0].id;
        app.handle_event(Event::RequestResolved { id });
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        assert!(
            app.numbers[0]
                .phone
                .as_deref()
                .is_some_and(|p| p.starts_with("+1 "))
        );
        app.handle_event(Event::CodeArrives { id });
        assert_eq!(app.numbers[0].status, NumberStatus::Received);
        assert!(app.numbers[0].code.is_some());
        assert!(
            app.snack
                .as_ref()
                .is_some_and(|s| s.kind == SnackKind::Success)
        );
    }

    #[test]
    fn failed_request_removes_number_and_toasts_error() {
        let mut app = App::new();
        app.sim.fail_rate_pct = 100.0;
        app.apply(Action::RequestFav(1));
        let id = app.numbers[0].id;
        let before = app.numbers.len();
        app.handle_event(Event::RequestResolved { id });
        assert_eq!(app.numbers.len(), before - 1);
        assert!(
            app.snack
                .as_ref()
                .is_some_and(|s| s.kind == SnackKind::Error)
        );
    }

    #[test]
    fn connect_validates_key_length_and_bad_keys() {
        let mut app = App::new();
        app.apply(Action::SetKeyInput("Grizzly SMS".into(), "short".into()));
        app.apply(Action::Connect("Grizzly SMS".into()));
        assert!(app.connecting.is_none());
        assert!(
            app.snack
                .as_ref()
                .is_some_and(|s| s.kind == SnackKind::Error)
        );

        app.apply(Action::SetKeyInput(
            "Grizzly SMS".into(),
            "this-is-bad-key".into(),
        ));
        app.apply(Action::Connect("Grizzly SMS".into()));
        assert_eq!(app.connecting.as_deref(), Some("Grizzly SMS"));
        app.handle_event(Event::ConnectResolved {
            provider: "Grizzly SMS".into(),
        });
        assert!(!app.providers[3].connected);

        app.apply(Action::SetKeyInput(
            "Grizzly SMS".into(),
            "gz_0123456789".into(),
        ));
        app.apply(Action::Connect("Grizzly SMS".into()));
        app.handle_event(Event::ConnectResolved {
            provider: "Grizzly SMS".into(),
        });
        assert!(app.providers[3].connected);
        assert_eq!(app.balances().count(), 4);
    }

    #[test]
    fn copy_respects_strip_dial_pref() {
        let mut app = App::new();
        app.apply(Action::CopyPhone(2));
        assert_eq!(app.take_clipboard().as_deref(), Some("+31 6 4471 0392"));
        assert!(app.copied_is("2-p"));
        app.apply(Action::TogglePref(PrefKey::StripDial));
        app.apply(Action::CopyPhone(2));
        assert_eq!(app.take_clipboard().as_deref(), Some("6 4471 0392"));
        app.apply(Action::CopyCode(1));
        assert_eq!(app.take_clipboard().as_deref(), Some("39284"));
    }

    #[test]
    fn sort_cycles_and_orders_countries() {
        let mut app = App::new();
        app.apply(Action::PickProvider("Hero SMS".into()));
        app.apply(Action::PickService("Telegram".into()));
        app.apply(Action::ToggleSort);
        assert_eq!(app.sort_dir, Some(SortDir::Asc));
        let asc: Vec<f64> = app
            .country_rows()
            .iter()
            .map(|c| app.country_price(c))
            .collect();
        assert!(asc.windows(2).all(|w| w[0] <= w[1]));
        app.apply(Action::ToggleSort);
        let desc: Vec<f64> = app
            .country_rows()
            .iter()
            .map(|c| app.country_price(c))
            .collect();
        assert!(desc.windows(2).all(|w| w[0] >= w[1]));
        app.apply(Action::ToggleSort);
        assert_eq!(app.sort_dir, None);
    }

    #[test]
    fn favorites_toggle_and_remove() {
        let mut app = App::new();
        let fav = app.favorites[0].clone();
        assert!(app.is_fav(&fav));
        app.apply(Action::ToggleFav(fav.clone()));
        assert!(!app.is_fav(&fav));
        app.apply(Action::ToggleFav(fav.clone()));
        assert!(app.is_fav(&fav));
        // Re-adding appends, so it is now the last entry.
        app.apply(Action::RemoveFav(app.favorites.len() - 1));
        assert!(!app.is_fav(&fav));
    }
}
