//! Application state: provider slots, the four-step wizard and the number lifecycle.
//!
//! The UI collects [`Action`]s while drawing and [`App::apply`] runs them. Anything that talks to
//! a provider goes through the [`Worker`]; results come back as [`Event`]s that
//! [`App::tick`] drains once per frame together with the wall-clock [`Timers`] (snackbar expiry,
//! status polls …).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

use sms_activate::{
    Activation, ActivationStatus, ActiveActivation, ApiError, ApiResult, CountryRef, Service,
    ServiceCode, StatusAck,
};

use crate::backend::{
    CountryRow, OfferGroup, OfferSelector, OfferTier, ProviderKind, RealBackend, SharedBackend,
};
use crate::config::Config;
use crate::domain::{
    DEFAULT_NUMBER_TTL, Favorite, Number, NumberStatus, PrefKey, Prefs, fmt_usd, fmt_usd4, mmss,
    parse_phone, phone_display, phone_for_clipboard,
};
use crate::worker::{Timers, Worker};

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
    PickProvider(ProviderKind),
    PickService(Service),
    PickCountry(CountryRow),
    /// Offer group name + tier.
    PickOffer(String, OfferTier),
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
    SetKeyInput(ProviderKind, String),
    Connect(ProviderKind),
    Disconnect(ProviderKind),
    DismissSnack,
}

/// Worker results and timer callbacks.
#[derive(Debug)]
pub enum Event {
    BalanceLoaded {
        kind: ProviderKind,
        /// The key the backend was created with; a mismatch means the slot changed meanwhile.
        key: String,
        result: ApiResult<f64>,
    },
    ServicesLoaded {
        kind: ProviderKind,
        generation: u64,
        result: ApiResult<Vec<Service>>,
    },
    CountriesLoaded {
        kind: ProviderKind,
        service: ServiceCode,
        generation: u64,
        result: ApiResult<Vec<CountryRow>>,
    },
    OffersLoaded {
        kind: ProviderKind,
        service: ServiceCode,
        country: CountryRef,
        generation: u64,
        result: ApiResult<Vec<OfferGroup>>,
    },
    Bought {
        local_id: u32,
        result: ApiResult<Activation>,
    },
    Polled {
        local_id: u32,
        result: ApiResult<ActivationStatus>,
    },
    CancelDone {
        local_id: u32,
        result: ApiResult<StatusAck>,
    },
    CompleteDone {
        local_id: u32,
        result: ApiResult<StatusAck>,
    },
    ActiveLoaded {
        kind: ProviderKind,
        /// The key the backend was created with; a mismatch means the slot changed meanwhile.
        key: String,
        result: ApiResult<Vec<ActiveNumber>>,
    },
    SnackExpire {
        token: u64,
    },
    CopiedExpire {
        token: u64,
    },
    PollDue {
        local_id: u32,
    },
}

/// Builds a backend for a provider from its API key.
pub type BackendFactory = Box<dyn Fn(ProviderKind, &str) -> SharedBackend + Send>;

/// An activation the provider still lists, with the names its card needs (resolved in the
/// worker, see [`resolve_active`]).
#[derive(Clone, Debug, Default)]
pub struct ActiveNumber {
    pub activation: ActiveActivation,
    pub service_name: Option<String>,
    pub country_name: Option<String>,
    pub dial: Option<String>,
}

/// One of the four providers as the app sees it.
pub struct ProviderSlot {
    pub kind: ProviderKind,
    /// Key the current backend was created with.
    pub key: Option<String>,
    pub backend: Option<SharedBackend>,
    pub connected: bool,
    pub connecting: bool,
    pub balance: Option<f64>,
    pub error: Option<String>,
    /// The connect in flight was started from Settings (toast on success).
    settings_connect: bool,
}

impl ProviderSlot {
    fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            key: None,
            backend: None,
            connected: false,
            connecting: false,
            balance: None,
            error: None,
            settings_connect: false,
        }
    }

    pub fn name(&self) -> &'static str {
        self.kind.name()
    }
}

pub struct StepInfo {
    pub num: u8,
    pub label: &'static str,
    pub value: Option<String>,
    pub active: bool,
    pub reachable: bool,
}

/// Everything needed to buy one number.
#[derive(Clone, Debug)]
struct Order {
    kind: ProviderKind,
    service: ServiceCode,
    service_name: String,
    country: CountryRef,
    country_name: String,
    dial: Option<String>,
    price: f64,
    selector: OfferSelector,
}

pub struct App {
    pub screen: Screen,
    pub step: u8,
    pub providers: Vec<ProviderSlot>,
    pub provider: Option<ProviderKind>,
    pub services: Vec<Service>,
    pub service: Option<Service>,
    pub countries: Vec<CountryRow>,
    pub country: Option<CountryRow>,
    pub offer_groups: Vec<OfferGroup>,
    /// Selected tier: offer group name + tier.
    pub offer: Option<(String, OfferTier)>,
    pub loading_services: bool,
    pub loading_countries: bool,
    pub loading_offers: bool,
    pub search: String,
    pub sort_dir: Option<SortDir>,
    pub numbers: Vec<Number>,
    pub favorites: Vec<Favorite>,
    pub prefs: Prefs,
    pub snack: Option<Snack>,
    pub copied: Option<String>,
    pub key_inputs: HashMap<ProviderKind, String>,
    pub config: Config,
    /// Wall clock as of the last [`App::tick`]; number countdowns are drawn against it.
    pub now: SystemTime,
    /// Bumped on every wizard step change so late responses for an earlier selection are dropped.
    generation: u64,
    worker: Worker<Event>,
    rx: Receiver<Event>,
    timers: Timers<Event>,
    factory: BackendFactory,
    /// Where [`App::persist`] writes; `None` means [`Config::path`] (tests point at a scratch file).
    config_path: Option<PathBuf>,
    token_seq: u64,
    copied_token: u64,
    clipboard: Option<String>,
    /// Set when a received code should be announced with the chime; drained by the frame loop.
    chime: bool,
    /// Floating developer panel (F12), debug builds only.
    #[cfg(debug_assertions)]
    pub debug: crate::ui::debug::DebugPanel,
}

const SNACK_TTL: Duration = Duration::from_millis(4200);
const COPIED_TTL: Duration = Duration::from_millis(1600);
/// First status poll after a purchase.
const FIRST_POLL: Duration = Duration::from_secs(5);
/// Poll cadence while the provider says "wait".
const POLL_INTERVAL: Duration = Duration::from_secs(6);
/// Poll retry after an unexpected error.
const POLL_RETRY: Duration = Duration::from_secs(12);
/// Poll retry after HTTP 429.
const POLL_RATE_LIMITED: Duration = Duration::from_secs(20);
/// Fallback wait after an unexpected `EARLY_CANCEL_DENIED` from a provider without a known
/// grace period (see [`ProviderKind::cancel_grace`]).
const CANCEL_GRACE: Duration = Duration::from_secs(120);

impl App {
    /// Production constructor: loads the config, seeds keys from the environment and connects
    /// every configured provider over real HTTP.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut config = Config::load();
        let seeded = config.seed_keys_from_env();
        let (worker, rx) = Worker::threaded(Some(cc.egui_ctx.clone()));
        let mut app = Self::with_parts(
            config,
            worker,
            rx,
            Box::new(|kind, key| RealBackend::shared(kind, key)),
        );
        if !seeded.is_empty() {
            app.persist();
        }
        #[allow(unused_mut)]
        let mut app = app;
        #[cfg(debug_assertions)]
        match std::env::var("GATEWAVE_DEBUG_SCREEN").as_deref() {
            Ok("favorites") => app.screen = Screen::Favorites,
            Ok("settings") => app.screen = Screen::Settings,
            _ => {}
        }
        app
    }

    /// Wires the state machine to the given worker and backend factory (tests use
    /// [`Worker::inline`] and mock backends) and starts connecting configured providers.
    pub fn with_parts(
        config: Config,
        worker: Worker<Event>,
        rx: Receiver<Event>,
        factory: BackendFactory,
    ) -> Self {
        let mut app = Self {
            screen: Screen::New,
            step: 1,
            providers: ProviderKind::ALL
                .into_iter()
                .map(ProviderSlot::new)
                .collect(),
            provider: None,
            services: Vec::new(),
            service: None,
            countries: Vec::new(),
            country: None,
            offer_groups: Vec::new(),
            offer: None,
            loading_services: false,
            loading_countries: false,
            loading_offers: false,
            search: String::new(),
            sort_dir: None,
            numbers: Vec::new(),
            favorites: config.favorites.clone(),
            prefs: config.prefs.clone(),
            snack: None,
            copied: None,
            key_inputs: HashMap::new(),
            config,
            now: SystemTime::now(),
            generation: 0,
            worker,
            rx,
            timers: Timers::default(),
            factory,
            config_path: None,
            token_seq: 0,
            copied_token: 0,
            clipboard: None,
            chime: false,
            #[cfg(debug_assertions)]
            debug: Default::default(),
        };
        app.start();
        app
    }

    /// Restores numbers from the config and kicks off the provider connections.
    fn start(&mut self) {
        let now = self.now;
        let mut numbers = std::mem::take(&mut self.config.numbers);
        // A purchase that was in flight when the app last quit has no id to resume from.
        numbers.retain(|n| n.status != NumberStatus::Requesting);
        for n in &mut numbers {
            n.polling = false;
            n.cancel_pending = false;
            if n.status == NumberStatus::Waiting {
                if n.expires_at.is_some_and(|e| e <= now) {
                    n.status = NumberStatus::Expired;
                } else {
                    self.timers
                        .schedule(Duration::ZERO, Event::PollDue { local_id: n.id });
                }
            }
        }
        self.numbers = numbers;

        let keys: Vec<(ProviderKind, String)> = ProviderKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let key = self.config.keys.get(&kind)?.trim();
                (!key.is_empty()).then(|| (kind, key.to_owned()))
            })
            .collect();
        for (kind, key) in keys {
            self.connect_slot(kind, key, false);
        }
    }

    // ------------------------------------------------------------------
    // Per-frame plumbing

    /// Drains due timers and worker results, then expires overdue numbers.
    pub fn tick(&mut self) {
        self.tick_at(Instant::now(), SystemTime::now());
    }

    fn tick_at(&mut self, mono: Instant, wall: SystemTime) {
        self.now = wall;
        for ev in self.timers.drain_due(mono) {
            self.handle_event(ev);
        }
        while let Ok(ev) = self.rx.try_recv() {
            self.handle_event(ev);
        }
        let mut expired = Vec::new();
        for n in &mut self.numbers {
            if n.status == NumberStatus::Waiting && n.expires_at.is_some_and(|e| e <= wall) {
                n.status = NumberStatus::Expired;
                n.polling = false;
                n.cancel_pending = false;
                expired.push(n.id);
            }
        }
        if !expired.is_empty() {
            self.timers.retain(
                |e| !matches!(e, Event::PollDue { local_id } if expired.contains(local_id)),
            );
            for id in expired {
                self.cancel_expired(id);
            }
            self.persist();
        }
    }

    /// The local clock gave up on a number: ask the provider to cancel it so the price comes back
    /// when the activation is still open there. The result is handled quietly by
    /// [`App::cancel_done`]; the startup `active()` listing re-opens it if the provider disagrees.
    fn cancel_expired(&mut self, id: u32) {
        let Some(n) = self.numbers.iter().find(|n| n.id == id) else {
            return;
        };
        let (Some(remote_id), Some(backend)) =
            (n.remote_id.clone(), self.slot(n.provider).backend.clone())
        else {
            return;
        };
        if let Some(n) = self.numbers.iter_mut().find(|n| n.id == id) {
            n.cancel_pending = true;
        }
        self.worker.run(move || Event::CancelDone {
            local_id: id,
            result: backend.cancel(&remote_id),
        });
    }

    pub fn take_clipboard(&mut self) -> Option<String> {
        self.clipboard.take()
    }

    /// True once per received code while the sound preference is on.
    pub fn take_chime(&mut self) -> bool {
        std::mem::take(&mut self.chime)
    }

    /// True while something on screen moves (skeletons, countdowns, blink, snackbar).
    pub fn busy(&self) -> bool {
        self.loading_services
            || self.loading_countries
            || self.loading_offers
            || self.snack.is_some()
            || self.providers.iter().any(|p| p.connecting)
            || self.numbers.iter().any(Number::is_live)
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.timers.next_due()
    }

    fn next_token(&mut self) -> u64 {
        self.token_seq += 1;
        self.token_seq
    }

    fn next_local_id(&mut self) -> u32 {
        self.config.next_number_id = self.config.next_number_id.max(1);
        let id = self.config.next_number_id;
        self.config.next_number_id += 1;
        id
    }

    /// Writes keys, prefs, favorites and numbers to disk. Never panics.
    pub fn persist(&mut self) {
        self.config.prefs = self.prefs.clone();
        self.config.favorites = self.favorites.clone();
        self.config.numbers = self.numbers.clone();
        let result = match &self.config_path {
            Some(path) => self.config.save_to(path),
            None => self.config.save(),
        };
        if let Err(e) = result {
            let path = self.config_path.clone().unwrap_or_else(Config::path);
            eprintln!("gatewave: could not save {}: {e}", path.display());
        }
    }

    // ------------------------------------------------------------------
    // Provider slots

    fn slot(&self, kind: ProviderKind) -> &ProviderSlot {
        self.providers
            .iter()
            .find(|p| p.kind == kind)
            .expect("every ProviderKind has a slot")
    }

    fn slot_mut(&mut self, kind: ProviderKind) -> &mut ProviderSlot {
        self.providers
            .iter_mut()
            .find(|p| p.kind == kind)
            .expect("every ProviderKind has a slot")
    }

    fn backend(&self, kind: ProviderKind) -> Option<SharedBackend> {
        let slot = self.slot(kind);
        slot.connected.then(|| slot.backend.clone()).flatten()
    }

    /// Creates a backend for `key`, fetches its balance and (when supported) its active activations.
    /// `from_settings` marks a connect the user started in Settings (it toasts on success).
    fn connect_slot(&mut self, kind: ProviderKind, key: String, from_settings: bool) {
        let backend = (self.factory)(kind, &key);
        debug_assert_eq!(backend.kind(), kind, "factory built the wrong provider");
        let slot = self.slot_mut(kind);
        slot.key = Some(key.clone());
        slot.backend = Some(backend.clone());
        slot.connecting = true;
        slot.connected = false;
        slot.balance = None;
        slot.error = None;
        slot.settings_connect = from_settings;
        let b = backend.clone();
        let k = key.clone();
        self.worker.run(move || Event::BalanceLoaded {
            kind,
            key: k,
            result: b.balance(),
        });
        if backend.capabilities().active_activations {
            self.worker.run(move || Event::ActiveLoaded {
                kind,
                result: backend.active().map(|list| resolve_active(&backend, list)),
                key,
            });
        }
    }

    fn refresh_balance(&mut self, kind: ProviderKind) {
        let slot = self.slot(kind);
        if let (Some(backend), Some(key)) = (slot.backend.clone(), slot.key.clone()) {
            self.worker.run(move || Event::BalanceLoaded {
                kind,
                key,
                result: backend.balance(),
            });
        }
    }

    // ------------------------------------------------------------------
    // Events

    pub fn handle_event(&mut self, ev: Event) {
        match ev {
            Event::BalanceLoaded { kind, key, result } => self.balance_loaded(kind, key, result),
            Event::ServicesLoaded {
                kind,
                generation,
                result,
            } => {
                if generation != self.generation || self.provider != Some(kind) {
                    return;
                }
                self.loading_services = false;
                match result {
                    Ok(list) => self.services = list,
                    Err(e) => {
                        self.services.clear();
                        self.toast(provider_error(kind, &e), SnackKind::Error);
                    }
                }
            }
            Event::CountriesLoaded {
                kind,
                service,
                generation,
                result,
            } => {
                if generation != self.generation
                    || self.provider != Some(kind)
                    || self.service.as_ref().map(|s| &s.code) != Some(&service)
                {
                    return;
                }
                self.loading_countries = false;
                match result {
                    Ok(list) => self.countries = list,
                    Err(e) => {
                        self.countries.clear();
                        self.toast(provider_error(kind, &e), SnackKind::Error);
                    }
                }
            }
            Event::OffersLoaded {
                kind,
                service,
                country,
                generation,
                result,
            } => {
                if generation != self.generation
                    || self.provider != Some(kind)
                    || self.service.as_ref().map(|s| &s.code) != Some(&service)
                    || self.country.as_ref().map(|c| &c.key) != Some(&country)
                {
                    return;
                }
                self.loading_offers = false;
                match result {
                    Ok(list) => self.offer_groups = list,
                    Err(e) => {
                        self.offer_groups.clear();
                        self.toast(provider_error(kind, &e), SnackKind::Error);
                    }
                }
            }
            Event::Bought { local_id, result } => self.bought(local_id, result),
            Event::Polled { local_id, result } => self.polled(local_id, result),
            Event::CancelDone { local_id, result } => self.cancel_done(local_id, result),
            Event::CompleteDone { local_id, result } => {
                if let Err(e) = result {
                    eprintln!("gatewave: could not complete activation #{local_id}: {e}");
                }
            }
            Event::ActiveLoaded { kind, key, result } => self.active_loaded(kind, key, result),
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
            Event::PollDue { local_id } => self.poll(local_id),
        }
    }

    fn balance_loaded(&mut self, kind: ProviderKind, key: String, result: ApiResult<f64>) {
        let slot = self.slot_mut(kind);
        if slot.key.as_deref() != Some(key.as_str()) {
            return; // disconnected or re-keyed while the call was in flight
        }
        let was_connecting = slot.connecting;
        let from_settings = std::mem::take(&mut slot.settings_connect);
        slot.connecting = false;
        match result {
            Ok(balance) => {
                slot.connected = true;
                slot.balance = Some(balance);
                slot.error = None;
                if was_connecting {
                    eprintln!("gatewave: {} connected, balance {balance:.2}", kind.name());
                }
                if self.config.keys.get(&kind) != Some(&key) {
                    // The key is only stored once the provider accepts it.
                    self.config.keys.insert(kind, key);
                    self.persist();
                }
                if from_settings {
                    self.toast(format!("{} connected.", kind.name()), SnackKind::Success);
                }
            }
            Err(e) => {
                let msg = provider_error(kind, &e);
                // A failed connect (or a key the provider now rejects) tears the slot down. A
                // transient error on a balance *refresh* after a purchase or cancel must not:
                // polls and cancels for numbers already paid for keep using the backend, and the
                // last known balance stays on screen.
                if was_connecting || matches!(e, ApiError::BadKey) {
                    slot.connected = false;
                    slot.balance = None;
                    slot.backend = None;
                    slot.key = None;
                }
                slot.error = Some(e.to_string());
                if was_connecting {
                    eprintln!("gatewave: {msg}");
                } else {
                    eprintln!("gatewave: {}: balance refresh failed: {e}", kind.name());
                }
                let toast = match e {
                    ApiError::BadKey => format!("{msg}."),
                    _ => msg,
                };
                self.toast(toast, SnackKind::Error);
            }
        }
    }

    fn active_loaded(
        &mut self,
        kind: ProviderKind,
        key: String,
        result: ApiResult<Vec<ActiveNumber>>,
    ) {
        if self.slot(kind).key.as_deref() != Some(key.as_str()) {
            return; // disconnected or re-keyed while the call was in flight
        }
        let list = match result {
            Ok(list) => list,
            Err(e) => {
                eprintln!(
                    "gatewave: {}: could not list active activations: {e}",
                    kind.name()
                );
                return;
            }
        };
        let now = self.now;
        let mut changed = false;
        for item in list {
            let ActiveNumber {
                activation: a,
                service_name,
                country_name,
                dial,
            } = item;
            if let Some(n) = self
                .numbers
                .iter_mut()
                .find(|n| n.provider == kind && n.remote_id.as_ref() == Some(&a.id))
            {
                // Still open at the provider although the local clock gave up on it: resume.
                if n.status == NumberStatus::Expired {
                    n.status = NumberStatus::Waiting;
                    n.expires_at = Some(now + DEFAULT_NUMBER_TTL);
                    n.total = DEFAULT_NUMBER_TTL;
                    n.cancelable_at = None;
                    n.cancel_pending = false;
                    n.polling = false;
                    let local_id = n.id;
                    self.timers
                        .schedule(Duration::ZERO, Event::PollDue { local_id });
                    changed = true;
                }
                continue;
            }
            let id = self.next_local_id();
            let service = a.service.clone().unwrap_or_default();
            let service_name = service_name.unwrap_or_else(|| service.to_string());
            let country = a.country.clone().unwrap_or_default();
            let country_name = country_name
                .or_else(|| a.country.as_ref().map(|c| c.to_string()))
                .unwrap_or_else(|| "—".into());
            let mut n = Number::requesting(
                id,
                kind,
                service,
                service_name,
                country,
                country_name,
                dial,
                a.cost.unwrap_or(0.0),
            );
            n.remote_id = Some(a.id);
            n.phone = a.phone.as_deref().map(with_plus);
            n.status = NumberStatus::Waiting;
            n.expires_at = Some(now + DEFAULT_NUMBER_TTL);
            n.total = DEFAULT_NUMBER_TTL;
            self.numbers.insert(0, n);
            self.timers
                .schedule(Duration::ZERO, Event::PollDue { local_id: id });
            changed = true;
        }
        if changed {
            self.persist();
        }
    }

    fn bought(&mut self, local_id: u32, result: ApiResult<Activation>) {
        let Some(idx) = self.numbers.iter().position(|n| n.id == local_id) else {
            return;
        };
        let now = self.now;
        match result {
            Ok(activation) => {
                let n = &mut self.numbers[idx];
                let kind = n.provider;
                n.status = NumberStatus::Waiting;
                n.remote_id = Some(activation.id);
                n.phone = Some(with_plus(&activation.phone));
                if n.dial.is_none()
                    && let Some(parts) = parse_phone(&activation.phone)
                {
                    n.dial = Some(format!("+{}", parts.country_code));
                }
                n.price = activation.cost.unwrap_or(n.price);
                n.expires_at = Some(now + DEFAULT_NUMBER_TTL);
                n.total = DEFAULT_NUMBER_TTL;
                // Providers that refuse early cancels get the button disabled right away,
                // counting the grace period down instead of bouncing off the API.
                n.cancelable_at = kind.cancel_grace().map(|g| now + g);
                self.timers
                    .schedule(FIRST_POLL, Event::PollDue { local_id });
                self.refresh_balance(kind);
            }
            Err(e) => {
                let n = self.numbers.remove(idx);
                let msg = match e {
                    ApiError::NoNumbers => format!(
                        "{}: no numbers available for {} · {}. Try another country.",
                        n.provider.name(),
                        n.service_name,
                        n.country_name
                    ),
                    e => provider_error(n.provider, &e),
                };
                self.toast(msg, SnackKind::Error);
            }
        }
        self.persist();
    }

    fn poll(&mut self, local_id: u32) {
        let Some(n) = self.numbers.iter_mut().find(|n| n.id == local_id) else {
            return;
        };
        if n.status != NumberStatus::Waiting || n.polling {
            return;
        }
        let Some(remote_id) = n.remote_id.clone() else {
            return;
        };
        let kind = n.provider;
        let Some(backend) = self.slot(kind).backend.clone() else {
            // Provider disconnected: try again later; expiry stops the loop eventually.
            self.timers
                .schedule(POLL_RETRY, Event::PollDue { local_id });
            return;
        };
        if let Some(n) = self.numbers.iter_mut().find(|n| n.id == local_id) {
            n.polling = true;
        }
        self.worker.run(move || Event::Polled {
            local_id,
            result: backend.status(&remote_id),
        });
    }

    fn polled(&mut self, local_id: u32, result: ApiResult<ActivationStatus>) {
        let Some(idx) = self.numbers.iter().position(|n| n.id == local_id) else {
            return;
        };
        self.numbers[idx].polling = false;
        if self.numbers[idx].status != NumberStatus::Waiting {
            return;
        }
        let reschedule = |app: &mut App, delay: Duration| {
            app.timers.schedule(delay, Event::PollDue { local_id });
        };
        match result {
            Ok(
                ActivationStatus::WaitCode
                | ActivationStatus::WaitRetry { .. }
                | ActivationStatus::WaitResend,
            ) => reschedule(self, POLL_INTERVAL),
            Ok(ActivationStatus::Ok { code }) => self.code_received(idx, code),
            Ok(ActivationStatus::Finished { code: Some(code) }) => self.code_received(idx, code),
            Ok(ActivationStatus::Finished { code: None }) | Ok(ActivationStatus::Expired) => {
                self.numbers[idx].status = NumberStatus::Expired;
                self.persist();
            }
            Ok(ActivationStatus::Cancelled) => {
                self.numbers[idx].status = NumberStatus::Cancelled;
                self.persist();
            }
            Err(ApiError::RateLimited { .. }) => reschedule(self, POLL_RATE_LIMITED),
            Err(ApiError::NoActivation) => {
                self.numbers[idx].status = NumberStatus::Expired;
                self.persist();
            }
            Err(e) => {
                eprintln!(
                    "gatewave: {}: status poll failed: {e}",
                    self.numbers[idx].provider.name()
                );
                reschedule(self, POLL_RETRY);
            }
        }
    }

    fn code_received(&mut self, idx: usize, code: String) {
        let n = &mut self.numbers[idx];
        n.status = NumberStatus::Received;
        n.code = Some(code.clone());
        let phone = n.phone.as_deref().map(phone_display).unwrap_or_default();
        if self.prefs.notify {
            self.toast(format!("Code received for {phone}"), SnackKind::Success);
        }
        if self.prefs.sound {
            self.chime = true;
        }
        if self.prefs.auto_copy {
            self.clipboard = Some(code.replace(' ', ""));
        }
        self.persist();
    }

    fn cancel_done(&mut self, local_id: u32, result: ApiResult<StatusAck>) {
        let now = self.now;
        let Some(n) = self.numbers.iter_mut().find(|n| n.id == local_id) else {
            return;
        };
        n.cancel_pending = false;
        let (kind, price) = (n.provider, n.price);
        // A cancel fired by the local TTL (see `cancel_expired`) is best effort: stay quiet.
        let automatic = n.status == NumberStatus::Expired;
        match result {
            Ok(_) => {
                n.status = NumberStatus::Cancelled;
                n.polling = false;
                self.timers
                    .retain(|e| !matches!(e, Event::PollDue { local_id: id } if *id == local_id));
                self.toast(
                    format!(
                        "Number cancelled · {} refunded to {}",
                        fmt_usd(price),
                        kind.name()
                    ),
                    SnackKind::Success,
                );
                self.refresh_balance(kind);
                self.persist();
            }
            Err(e) if automatic => eprintln!(
                "gatewave: {}: could not cancel expired activation #{local_id}: {e}",
                kind.name()
            ),
            Err(ApiError::EarlyCancelDenied) => {
                let grace = kind.cancel_grace().unwrap_or(CANCEL_GRACE);
                n.cancelable_at = Some(now + grace);
                self.toast(
                    format!(
                        "{}: number can be cancelled in {}",
                        kind.name(),
                        mmss(grace)
                    ),
                    SnackKind::Info,
                );
                self.persist();
            }
            Err(e) => self.toast(provider_error(kind, &e), SnackKind::Error),
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
            Action::PickProvider(kind) => self.pick_provider(kind),
            Action::PickService(s) => self.pick_service(s),
            Action::PickCountry(c) => self.pick_country(c),
            Action::PickOffer(group, tier) => self.offer = Some((group, tier)),
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
                if let Some(order) = self.wizard_order() {
                    self.request(order);
                }
            }
            Action::RequestFav(i) => {
                if let Some(f) = self.favorites.get(i).cloned() {
                    self.request(Order {
                        kind: f.provider,
                        service: f.service,
                        service_name: f.service_name,
                        country: f.country,
                        country_name: f.country_name,
                        dial: f.dial,
                        price: f.price,
                        selector: f.selector,
                    });
                }
            }
            Action::RemoveFav(i) => {
                if i < self.favorites.len() {
                    self.favorites.remove(i);
                    self.persist();
                }
            }
            Action::CopyPhone(id) => {
                if let Some(n) = self.numbers.iter().find(|n| n.id == id)
                    && let Some(phone) = &n.phone
                {
                    let text = phone_for_clipboard(phone, n.dial.as_deref(), self.prefs.strip_dial);
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
            Action::DismissNumber(id) => self.dismiss_number(id),
            Action::CancelNumber(id) => self.cancel_number(id),
            Action::TogglePref(k) => {
                self.prefs.toggle(k);
                self.persist();
            }
            Action::SetKeyInput(kind, v) => {
                self.key_inputs.insert(kind, v);
            }
            Action::Connect(kind) => self.connect_provider(kind),
            Action::Disconnect(kind) => self.disconnect_provider(kind),
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
        self.timers
            .schedule(SNACK_TTL, Event::SnackExpire { token });
    }

    fn copy(&mut self, text: String, key: String) {
        self.clipboard = Some(text);
        self.copied = Some(key);
        let token = self.next_token();
        self.copied_token = token;
        self.timers
            .schedule(COPIED_TTL, Event::CopiedExpire { token });
    }

    fn not_connected_toast(&mut self, kind: ProviderKind) {
        self.toast(
            format!(
                "{} is not connected. Add its API key in Settings.",
                kind.name()
            ),
            SnackKind::Info,
        );
    }

    fn pick_provider(&mut self, kind: ProviderKind) {
        let Some(backend) = self.backend(kind) else {
            self.not_connected_toast(kind);
            return;
        };
        self.provider = Some(kind);
        self.service = None;
        self.country = None;
        self.offer = None;
        self.services.clear();
        self.countries.clear();
        self.offer_groups.clear();
        self.step = 2;
        self.screen = Screen::New;
        self.search.clear();
        self.generation += 1;
        let generation = self.generation;
        self.loading_services = true;
        self.worker.run(move || Event::ServicesLoaded {
            kind,
            generation,
            result: backend.services(),
        });
    }

    fn pick_service(&mut self, service: Service) {
        let Some(kind) = self.provider else {
            return;
        };
        let Some(backend) = self.backend(kind) else {
            self.not_connected_toast(kind);
            return;
        };
        let code = service.code.clone();
        self.service = Some(service);
        self.country = None;
        self.offer = None;
        self.countries.clear();
        self.offer_groups.clear();
        self.step = 3;
        self.search.clear();
        self.generation += 1;
        let generation = self.generation;
        self.loading_countries = true;
        self.worker.run(move || Event::CountriesLoaded {
            kind,
            result: backend.countries_for(&code),
            service: code,
            generation,
        });
    }

    fn pick_country(&mut self, country: CountryRow) {
        let (Some(kind), Some(service)) = (self.provider, self.service.clone()) else {
            return;
        };
        let Some(backend) = self.backend(kind) else {
            self.not_connected_toast(kind);
            return;
        };
        let key = country.key.clone();
        self.country = Some(country);
        self.offer = None;
        self.offer_groups.clear();
        self.step = 4;
        self.search.clear();
        self.generation += 1;
        let generation = self.generation;
        self.loading_offers = true;
        let code = service.code;
        self.worker.run(move || Event::OffersLoaded {
            kind,
            result: backend.offers(&code, &key),
            service: code,
            country: key,
            generation,
        });
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
        self.persist();
    }

    /// The wizard's current selection as a purchase, once an offer is picked.
    fn wizard_order(&self) -> Option<Order> {
        let kind = self.provider?;
        let service = self.service.as_ref()?;
        let country = self.country.as_ref()?;
        let (_, tier) = self.offer.as_ref()?;
        Some(Order {
            kind,
            service: service.code.clone(),
            service_name: service.name.clone(),
            country: country.key.clone(),
            country_name: country.name.clone(),
            dial: country.dial.clone(),
            price: tier.price,
            selector: tier.selector.clone(),
        })
    }

    /// Inserts a `Requesting` card and buys the number in the background.
    fn request(&mut self, order: Order) {
        let Some(backend) = self.backend(order.kind) else {
            self.not_connected_toast(order.kind);
            return;
        };
        let local_id = self.next_local_id();
        self.numbers.insert(
            0,
            Number::requesting(
                local_id,
                order.kind,
                order.service.clone(),
                order.service_name,
                order.country.clone(),
                order.country_name,
                order.dial,
                order.price,
            ),
        );
        let Order {
            service,
            country,
            selector,
            ..
        } = order;
        self.worker.run(move || Event::Bought {
            local_id,
            result: backend.buy(&service, &country, &selector),
        });
    }

    fn cancel_number(&mut self, id: u32) {
        let now = self.now;
        let Some(n) = self.numbers.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if n.status != NumberStatus::Waiting
            || n.cancel_pending
            || n.cancelable_at.is_some_and(|t| t > now)
        {
            return;
        }
        let Some(remote_id) = n.remote_id.clone() else {
            return;
        };
        let kind = n.provider;
        let Some(backend) = self.slot(kind).backend.clone() else {
            self.not_connected_toast(kind);
            return;
        };
        if let Some(n) = self.numbers.iter_mut().find(|n| n.id == id) {
            n.cancel_pending = true;
        }
        self.worker.run(move || Event::CancelDone {
            local_id: id,
            result: backend.cancel(&remote_id),
        });
    }

    fn dismiss_number(&mut self, id: u32) {
        let Some(idx) = self.numbers.iter().position(|n| n.id == id) else {
            return;
        };
        let n = self.numbers.remove(idx);
        self.timers
            .retain(|e| !matches!(e, Event::PollDue { local_id } if *local_id == id));
        if n.status == NumberStatus::Received
            && let Some(remote_id) = n.remote_id
            && let Some(backend) = self.slot(n.provider).backend.clone()
        {
            // Best effort: tell the provider we are done with the activation.
            self.worker.run(move || Event::CompleteDone {
                local_id: id,
                result: backend.complete(&remote_id),
            });
        }
        self.persist();
    }

    fn connect_provider(&mut self, kind: ProviderKind) {
        let key = self
            .key_inputs
            .get(&kind)
            .map(|k| k.trim().to_string())
            .unwrap_or_default();
        if key.chars().count() < kind.min_key_len() {
            self.toast(
                format!(
                    "API key looks too short. Check it in your {} dashboard.",
                    kind.name()
                ),
                SnackKind::Error,
            );
            return;
        }
        self.connect_slot(kind, key, true);
    }

    fn disconnect_provider(&mut self, kind: ProviderKind) {
        if self.config.keys.remove(&kind).is_some() {
            self.persist();
        }
        let slot = self.slot_mut(kind);
        slot.key = None;
        slot.backend = None;
        slot.balance = None;
        slot.connected = false;
        slot.connecting = false;
        slot.error = None;
        self.key_inputs.remove(&kind);
        self.toast(format!("{} disconnected.", kind.name()), SnackKind::Info);
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
            (1, "Provider", self.provider.map(|k| k.name().to_string())),
            (2, "Service", self.service.as_ref().map(|s| s.name.clone())),
            (3, "Country", self.country.as_ref().map(|c| c.name.clone())),
            (
                4,
                "Offer",
                self.offer
                    .as_ref()
                    .map(|(group, t)| format!("{} · {group}", fmt_usd(t.price))),
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

    pub fn provider_rows(&self) -> Vec<&ProviderSlot> {
        self.providers
            .iter()
            .filter(|p| self.matches(p.name()))
            .collect()
    }

    pub fn service_rows(&self) -> Vec<&Service> {
        self.services
            .iter()
            .filter(|s| self.matches(&s.name) || self.matches(s.code.as_str()))
            .collect()
    }

    pub fn country_rows(&self) -> Vec<&CountryRow> {
        let mut list: Vec<&CountryRow> = self
            .countries
            .iter()
            .filter(|c| self.matches(&c.name) || self.matches(&c.code))
            .collect();
        if let Some(dir) = self.sort_dir {
            list.sort_by(|a, b| {
                let d = a.price.total_cmp(&b.price);
                if dir == SortDir::Asc { d } else { d.reverse() }
            });
        }
        list
    }

    pub fn offer_rows(&self) -> Vec<&OfferGroup> {
        if self.step != 4 || self.loading_offers {
            return Vec::new();
        }
        self.offer_groups
            .iter()
            .filter(|g| self.matches(&g.name))
            .collect()
    }

    /// A favorite for the given tier of the wizard's current provider · service · country.
    pub fn favorite_for(&self, group: &str, tier: &OfferTier) -> Option<Favorite> {
        let kind = self.provider?;
        let service = self.service.as_ref()?;
        let country = self.country.as_ref()?;
        Some(Favorite {
            provider: kind,
            service: service.code.clone(),
            service_name: service.name.clone(),
            country: country.key.clone(),
            country_name: country.name.clone(),
            country_code: country.code.clone(),
            dial: country.dial.clone(),
            operator: group.to_string(),
            price: tier.price,
            selector: tier.selector.clone(),
        })
    }

    /// Connected providers with a known balance, in [`ProviderKind::ALL`] order.
    pub fn balances(&self) -> Vec<(&'static str, f64)> {
        self.providers
            .iter()
            .filter(|p| p.connected)
            .filter_map(|p| Some((p.name(), p.balance?)))
            .collect()
    }

    /// (line, via, price) for the sticky request bar, when an offer is selected.
    pub fn summary(&self) -> Option<(String, String, String)> {
        let (group, tier) = self.offer.as_ref()?;
        let line = format!(
            "{} · {}",
            self.service.as_ref().map(|s| s.name.as_str()).unwrap_or(""),
            self.country.as_ref().map(|c| c.name.as_str()).unwrap_or("")
        );
        let via = format!(
            "via {} · {group}",
            self.provider.map(|k| k.name()).unwrap_or("")
        );
        Some((line, via, fmt_usd4(tier.price)))
    }

    pub fn copied_is(&self, key: &str) -> bool {
        self.copied.as_deref() == Some(key)
    }
}

/// `"<provider>: <error>"`, with the bare `BAD_KEY` token spelled out.
fn provider_error(kind: ProviderKind, err: &ApiError) -> String {
    match err {
        ApiError::BadKey => format!("{}: invalid API key", kind.name()),
        e => format!("{}: {e}", kind.name()),
    }
}

/// Looks up service and country names (and the dialling prefix) for activations recovered from
/// the provider, so their cards read like the ones bought here. Runs on the worker; lookup
/// failures leave the raw codes in place.
fn resolve_active(backend: &SharedBackend, list: Vec<ActiveActivation>) -> Vec<ActiveNumber> {
    if list.is_empty() {
        return Vec::new();
    }
    let services = backend.services().unwrap_or_default();
    let mut countries: Vec<(ServiceCode, Vec<CountryRow>)> = Vec::new();
    list.into_iter()
        .map(|a| {
            let service_name = a.service.as_ref().and_then(|code| {
                services
                    .iter()
                    .find(|s| &s.code == code)
                    .map(|s| s.name.clone())
            });
            let row = match (&a.service, &a.country) {
                (Some(code), Some(country)) => {
                    if !countries.iter().any(|(c, _)| c == code) {
                        let rows = backend.countries_for(code).unwrap_or_default();
                        countries.push((code.clone(), rows));
                    }
                    countries
                        .iter()
                        .find(|(c, _)| c == code)
                        .and_then(|(_, rows)| rows.iter().find(|r| &r.key == country))
                        .cloned()
                }
                _ => None,
            };
            ActiveNumber {
                service_name,
                country_name: row.as_ref().map(|r| r.name.clone()),
                dial: row.and_then(|r| r.dial),
                activation: a,
            }
        })
        .collect()
}

/// Providers return numbers without the `+`; the UI shows international format.
fn with_plus(phone: &str) -> String {
    let p = phone.trim();
    if p.starts_with('+') {
        p.to_string()
    } else {
        format!("+{p}")
    }
}

// ---------------------------------------------------------------------------
// Test support

#[cfg(test)]
pub mod testing {
    //! Builds apps over [`MockBackend`]s with an inline worker and a throw-away config file.

    use std::sync::Once;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::backend::mock::MockBackend;

    static SCRATCH_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A private config location for the whole test process, so nothing ever touches
    /// `~/.config/gatewave`. Set once, before any thread reads the environment.
    pub fn scratch_config_path() -> PathBuf {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let dir = std::env::temp_dir().join(format!("gatewave-tests-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            // SAFETY: called exactly once, guarded by `Once`, before any test thread has started
            // reading `GATEWAVE_CONFIG` (every test goes through this function first).
            unsafe { std::env::set_var("GATEWAVE_CONFIG", dir.join("config.json")) };
            *SCRATCH_DIR.lock().unwrap() = Some(dir);
        });
        let dir = SCRATCH_DIR.lock().unwrap().clone().unwrap();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        dir.join(format!("config-{n}.json"))
    }

    pub fn key_for(kind: ProviderKind) -> String {
        format!("{:?}_key_0123456789abcdef0123456789abcdef", kind)
    }

    /// A config with a key for every listed provider.
    pub fn config_with_keys(kinds: &[ProviderKind]) -> Config {
        let mut cfg = Config::default();
        for k in kinds {
            cfg.keys.insert(*k, key_for(*k));
        }
        cfg
    }

    /// Builds an app whose factory hands out the given mock for each provider (any provider not
    /// listed gets a fresh [`MockBackend`] of its own kind).
    pub fn app_with(
        config: Config,
        mocks: Vec<(ProviderKind, Arc<MockBackend>)>,
    ) -> (App, PathBuf) {
        let path = scratch_config_path();
        let (worker, rx) = Worker::inline();
        let table = mocks.clone();
        let factory: BackendFactory =
            Box::new(
                move |kind, _key| match table.iter().find(|(k, _)| *k == kind) {
                    Some((_, m)) => m.clone() as SharedBackend,
                    None => Arc::new(MockBackend::new(kind)) as SharedBackend,
                },
            );
        let mut app = App::with_parts(config, worker, rx, factory);
        // Nothing is written before the first event is handled, so the scratch file can be
        // swapped in after construction.
        app.config_path = Some(path.clone());
        (app, path)
    }

    /// One connected Hero SMS provider over a fresh mock.
    pub fn hero_app() -> (App, Arc<MockBackend>, PathBuf) {
        let mock = Arc::new(MockBackend::new(ProviderKind::HeroSms));
        let (mut app, path) = app_with(
            config_with_keys(&[ProviderKind::HeroSms]),
            vec![(ProviderKind::HeroSms, mock.clone())],
        );
        app.tick();
        (app, mock, path)
    }

    impl App {
        /// Runs timers due within `d` and drains the worker channel — the test clock.
        pub fn fast_forward(&mut self, d: Duration) {
            self.tick_at(Instant::now() + d, SystemTime::now() + d);
        }

        pub fn snack_text(&self) -> Option<&str> {
            self.snack.as_ref().map(|s| s.msg.as_str())
        }

        pub fn snack_kind(&self) -> Option<SnackKind> {
            self.snack.as_ref().map(|s| s.kind)
        }

        /// Walks the wizard provider → service → country → offer over the mock's canned data.
        pub fn walk_to_offer(&mut self, kind: ProviderKind) {
            self.apply(Action::PickProvider(kind));
            self.tick();
            let service = self.services[0].clone();
            self.apply(Action::PickService(service));
            self.tick();
            let country = self.countries[0].clone();
            self.apply(Action::PickCountry(country));
            self.tick();
            let group = self.offer_groups[0].clone();
            self.apply(Action::PickOffer(group.name, group.tiers[0].clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::testing::*;
    use super::*;
    use crate::backend::ANY_OPERATOR;
    use crate::backend::mock::MockBackend;

    #[test]
    fn startup_connects_configured_providers_and_merges_active() {
        let hero = Arc::new(MockBackend::new(ProviderKind::HeroSms));
        let mut bower = MockBackend::new(ProviderKind::SmsBower);
        bower.balance = 3.5;
        bower.active = vec![ActiveActivation {
            id: "555".into(),
            service: Some(ServiceCode::from("wa")),
            phone: Some("447700900123".into()),
            cost: Some(0.4),
            country: Some(CountryRef::Id(187)),
            ..Default::default()
        }];
        let bower = Arc::new(bower);
        let (mut app, _) = app_with(
            config_with_keys(&[ProviderKind::HeroSms, ProviderKind::SmsBower]),
            vec![
                (ProviderKind::HeroSms, hero.clone()),
                (ProviderKind::SmsBower, bower.clone()),
            ],
        );
        assert!(app.providers.iter().filter(|p| p.connecting).count() == 2);
        app.tick();
        let h = app.slot(ProviderKind::HeroSms);
        assert!(h.connected && !h.connecting && h.balance == Some(10.0));
        assert!(!app.slot(ProviderKind::FiveSim).connected);
        assert_eq!(app.balances(), vec![("Hero SMS", 10.0), ("SMSBower", 3.5)]);
        assert_eq!(hero.calls(), vec!["balance", "active"]);
        // The unknown SMSBower activation became a Waiting card.
        assert_eq!(app.numbers.len(), 1);
        let n = &app.numbers[0];
        assert_eq!(n.status, NumberStatus::Waiting);
        assert_eq!(n.phone.as_deref(), Some("+447700900123"));
        assert_eq!(n.remote_id.as_ref().map(|i| i.as_str()), Some("555"));
        assert_eq!(n.price, 0.4);
        // Names resolved through the provider's own lists, like a number bought here.
        assert_eq!(n.meta_line(), "WhatsApp · United States · SMSBower");
        assert_eq!(n.dial.as_deref(), Some("+1"));
        assert!(bower.calls().iter().any(|c| c == "services"));
        // It is polled straight away.
        app.fast_forward(Duration::from_millis(10));
        assert!(bower.calls().iter().any(|c| c == "status 555"));
        // A second listing does not duplicate it.
        let listing = || ActiveNumber {
            activation: bower.active[0].clone(),
            ..Default::default()
        };
        app.handle_event(Event::ActiveLoaded {
            kind: ProviderKind::SmsBower,
            key: key_for(ProviderKind::SmsBower),
            result: Ok(vec![listing()]),
        });
        assert_eq!(app.numbers.len(), 1);
        // A listing for a key the slot no longer has is stale and dropped.
        let mut stale = listing();
        stale.activation.id = "556".into();
        app.handle_event(Event::ActiveLoaded {
            kind: ProviderKind::SmsBower,
            key: "some-older-key".into(),
            result: Ok(vec![stale]),
        });
        assert_eq!(app.numbers.len(), 1);
    }

    #[test]
    fn active_listing_reopens_a_locally_expired_number() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        // The provider refuses the automatic cancel, so the card stays Expired.
        *mock.fail_with.lock().unwrap() = Some("busy".into());
        app.numbers[0].expires_at = Some(SystemTime::now() - Duration::from_secs(1));
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Expired);
        *mock.fail_with.lock().unwrap() = None;
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Expired);
        assert!(!app.numbers[0].cancel_pending);
        assert!(app.snack.is_none(), "automatic cancel failures stay quiet");

        // The provider still lists it → back to Waiting with a fresh window and a poll.
        app.handle_event(Event::ActiveLoaded {
            kind: ProviderKind::HeroSms,
            key: key_for(ProviderKind::HeroSms),
            result: Ok(vec![ActiveNumber {
                activation: ActiveActivation {
                    id: "777".into(),
                    ..Default::default()
                },
                ..Default::default()
            }]),
        });
        assert_eq!(app.numbers.len(), 1);
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        assert!(app.numbers[0].time_left(app.now) > Duration::from_secs(600));
        let polls = mock.calls().iter().filter(|c| c == &"status 777").count();
        app.fast_forward(Duration::from_millis(10));
        assert_eq!(
            mock.calls().iter().filter(|c| c == &"status 777").count(),
            polls + 1
        );
    }

    #[test]
    fn startup_failure_disconnects_and_toasts() {
        let bad = Arc::new(MockBackend::new(ProviderKind::FiveSim).failing("boom"));
        let (mut app, _) = app_with(
            config_with_keys(&[ProviderKind::FiveSim]),
            vec![(ProviderKind::FiveSim, bad)],
        );
        app.tick();
        let s = app.slot(ProviderKind::FiveSim);
        assert!(!s.connected && !s.connecting && s.backend.is_none());
        assert_eq!(s.error.as_deref(), Some("provider error `boom`"));
        assert_eq!(app.snack_text(), Some("5SIM: provider error `boom`"));
        assert_eq!(app.snack_kind(), Some(SnackKind::Error));
        // The key stays in the config (retried next launch) but is never shown in the input.
        assert!(app.config.keys.contains_key(&ProviderKind::FiveSim));
        assert!(!app.key_inputs.contains_key(&ProviderKind::FiveSim));
    }

    #[test]
    fn settings_reconnect_with_the_stored_key_toasts() {
        let mock = Arc::new(MockBackend::new(ProviderKind::FiveSim).failing("down"));
        let (mut app, _) = app_with(
            config_with_keys(&[ProviderKind::FiveSim]),
            vec![(ProviderKind::FiveSim, mock.clone())],
        );
        app.tick();
        assert!(!app.slot(ProviderKind::FiveSim).connected);
        *mock.fail_with.lock().unwrap() = None;
        app.apply(Action::SetKeyInput(
            ProviderKind::FiveSim,
            key_for(ProviderKind::FiveSim),
        ));
        app.apply(Action::Connect(ProviderKind::FiveSim));
        app.tick();
        assert!(app.slot(ProviderKind::FiveSim).connected);
        assert_eq!(app.snack_text(), Some("5SIM connected."));
        assert_eq!(app.snack_kind(), Some(SnackKind::Success));
    }

    #[test]
    fn failed_balance_refresh_keeps_the_connection() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        // A transient error on the post-purchase refresh.
        app.handle_event(Event::BalanceLoaded {
            kind: ProviderKind::HeroSms,
            key: key_for(ProviderKind::HeroSms),
            result: Err(ApiError::RateLimited { retry_after: None }),
        });
        let s = app.slot(ProviderKind::HeroSms);
        assert!(s.connected && s.backend.is_some() && s.key.is_some());
        assert_eq!(s.balance, Some(10.0), "last known balance stays");
        assert!(s.error.is_some());
        assert_eq!(app.snack_kind(), Some(SnackKind::Error));
        assert!(app.snack_text().unwrap().starts_with("Hero SMS: "));
        // Polling and cancelling the paid number keep working.
        app.fast_forward(Duration::from_secs(6));
        assert!(mock.calls().iter().any(|c| c == "status 777"));
        let id = app.numbers[0].id;
        app.fast_forward(CANCEL_GRACE);
        app.apply(Action::CancelNumber(id));
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Cancelled);
        // A key the provider now rejects does tear the slot down.
        app.handle_event(Event::BalanceLoaded {
            kind: ProviderKind::HeroSms,
            key: key_for(ProviderKind::HeroSms),
            result: Err(ApiError::BadKey),
        });
        let s = app.slot(ProviderKind::HeroSms);
        assert!(!s.connected && s.backend.is_none());
        assert_eq!(app.snack_text(), Some("Hero SMS: invalid API key."));
    }

    #[test]
    fn factory_builds_a_mock_for_unlisted_providers() {
        let (mut app, _, _) = hero_app();
        app.apply(Action::SetKeyInput(
            ProviderKind::SmsBower,
            key_for(ProviderKind::SmsBower),
        ));
        app.apply(Action::Connect(ProviderKind::SmsBower));
        app.tick();
        let s = app.slot(ProviderKind::SmsBower);
        assert!(s.connected && s.balance == Some(10.0));
        assert_eq!(app.snack_text(), Some("SMSBower connected."));
    }

    #[test]
    fn bad_key_is_spelled_out() {
        let bad = Arc::new(MockBackend::new(ProviderKind::TigerSms));
        let (mut app, _) = app_with(Config::default(), vec![(ProviderKind::TigerSms, bad)]);
        app.handle_event(Event::BalanceLoaded {
            kind: ProviderKind::TigerSms,
            key: "x".into(),
            result: Err(ApiError::BadKey),
        });
        // Stale (no such key on the slot) → ignored.
        assert!(app.snack.is_none());
        app.apply(Action::SetKeyInput(
            ProviderKind::TigerSms,
            "tiger-key-1234".into(),
        ));
        app.apply(Action::Connect(ProviderKind::TigerSms));
        app.handle_event(Event::BalanceLoaded {
            kind: ProviderKind::TigerSms,
            key: "tiger-key-1234".into(),
            result: Err(ApiError::BadKey),
        });
        assert_eq!(app.snack_text(), Some("Tiger SMS: invalid API key."));
    }

    #[test]
    fn picking_a_disconnected_provider_only_toasts() {
        let (mut app, _, _) = hero_app();
        app.apply(Action::PickProvider(ProviderKind::FiveSim));
        assert_eq!(app.step, 1);
        assert!(app.provider.is_none());
        assert_eq!(
            app.snack_text(),
            Some("5SIM is not connected. Add its API key in Settings.")
        );
        assert_eq!(app.snack_kind(), Some(SnackKind::Info));
    }

    #[test]
    fn full_wizard_flow_to_received_code() {
        let (mut app, mock, _) = hero_app();
        app.apply(Action::PickProvider(ProviderKind::HeroSms));
        assert_eq!(app.step, 2);
        assert!(app.loading_services && app.busy());
        assert!(app.step_reachable(2) && !app.step_reachable(3));
        app.tick();
        assert!(!app.loading_services);
        assert_eq!(app.service_rows().len(), 2);
        app.apply(Action::SetSearch("wa".into()));
        assert_eq!(app.service_rows()[0].name, "WhatsApp");
        let tg = app.services[0].clone();
        app.apply(Action::PickService(tg));
        assert_eq!(app.step, 3);
        assert!(app.loading_countries && app.search.is_empty());
        app.tick();
        assert_eq!(app.country_rows().len(), 1);
        assert_eq!(app.country_rows()[0].code, "US");
        let us = app.countries[0].clone();
        app.apply(Action::PickCountry(us));
        assert_eq!(app.step, 4);
        assert!(app.loading_offers && app.offer_rows().is_empty());
        app.tick();
        assert_eq!(app.offer_rows()[0].name, ANY_OPERATOR);
        assert!(app.summary().is_none());
        let tier = app.offer_groups[0].tiers[1].clone();
        app.apply(Action::PickOffer(ANY_OPERATOR.into(), tier));
        let (line, via, price) = app.summary().unwrap();
        assert_eq!(line, "Telegram · United States");
        assert_eq!(via, "via Hero SMS · Any operator");
        assert_eq!(price, "$0.5000");
        assert_eq!(
            app.steps()[3].value.as_deref(),
            Some("$0.50 · Any operator")
        );

        app.apply(Action::RequestNumber);
        assert_eq!(app.numbers[0].status, NumberStatus::Requesting);
        assert_eq!(app.step, 4);
        assert!(app.offer.is_some());
        app.tick();
        let n = &app.numbers[0];
        assert_eq!(n.status, NumberStatus::Waiting);
        assert_eq!(n.phone.as_deref(), Some("+12025550123"));
        assert_eq!(n.remote_id.as_ref().map(|i| i.as_str()), Some("777"));
        assert_eq!(n.price, 0.5);
        assert!(n.total == DEFAULT_NUMBER_TTL && n.expires_at.is_some());
        assert!(mock.calls().iter().any(|c| c == "buy tg 187 MaxPrice(0.5)"));
        // Money safety: one click, one purchase call.
        assert_eq!(
            mock.calls()
                .iter()
                .filter(|c| c.starts_with("buy "))
                .count(),
            1
        );
        // Balance refreshed after the purchase.
        assert_eq!(mock.calls().iter().filter(|c| *c == "balance").count(), 2);

        // First poll after 5 s says wait; the next one delivers the code.
        app.fast_forward(Duration::from_secs(6));
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        assert!(!app.numbers[0].polling);
        assert_eq!(
            mock.calls().iter().filter(|c| *c == "status 777").count(),
            1
        );
        mock.set_status(ActivationStatus::Ok {
            code: "39284".into(),
        });
        app.fast_forward(Duration::from_secs(13));
        assert_eq!(app.numbers[0].status, NumberStatus::Received);
        assert_eq!(app.numbers[0].code.as_deref(), Some("39284"));
        assert_eq!(app.snack_text(), Some("Code received for +1 202 555 0123"));
        assert_eq!(app.snack_kind(), Some(SnackKind::Success));
        // No further polls once received.
        app.fast_forward(Duration::from_secs(60));
        assert_eq!(
            mock.calls().iter().filter(|c| *c == "status 777").count(),
            2
        );
    }

    #[test]
    fn auto_copy_and_quiet_prefs() {
        let (mut app, mock, _) = hero_app();
        app.apply(Action::TogglePref(PrefKey::AutoCopy));
        app.apply(Action::TogglePref(PrefKey::Notify));
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        app.snack = None;
        mock.set_status(ActivationStatus::Finished {
            code: Some("12 345".into()),
        });
        app.fast_forward(Duration::from_secs(6));
        assert_eq!(app.numbers[0].status, NumberStatus::Received);
        assert!(app.snack.is_none());
        assert_eq!(app.take_clipboard().as_deref(), Some("12345"));
        // Sound is on by default: one chime per received code, then nothing.
        assert!(app.take_chime());
        assert!(!app.take_chime());

        // Sound off: a second code arrives silently.
        app.apply(Action::TogglePref(PrefKey::Sound));
        app.apply(Action::RequestNumber);
        app.tick();
        app.fast_forward(Duration::from_secs(6));
        assert_eq!(app.numbers[0].status, NumberStatus::Received);
        assert!(!app.take_chime());
    }

    #[test]
    fn poll_outcomes() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.handle_event(Event::Polled {
            local_id: id,
            result: Err(ApiError::RateLimited { retry_after: None }),
        });
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        assert!(app.snack.is_none());
        app.handle_event(Event::Polled {
            local_id: id,
            result: Ok(ActivationStatus::Cancelled),
        });
        assert_eq!(app.numbers[0].status, NumberStatus::Cancelled);

        // Expired / NoActivation.
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.handle_event(Event::Polled {
            local_id: id,
            result: Err(ApiError::NoActivation),
        });
        assert_eq!(app.numbers[0].status, NumberStatus::Expired);
        // A poll for an unknown id is harmless.
        app.handle_event(Event::PollDue { local_id: 9999 });
        assert!(mock.calls().iter().all(|c| c != "status 9999"));
    }

    #[test]
    fn number_expires_locally_and_drops_its_poll() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        app.numbers[0].expires_at = Some(SystemTime::now() - Duration::from_secs(1));
        let polls_before = mock
            .calls()
            .iter()
            .filter(|c| c.starts_with("status"))
            .count();
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Expired);
        assert!(app.next_deadline().is_none() || !app.busy());
        app.fast_forward(Duration::from_secs(30));
        let polls_after = mock
            .calls()
            .iter()
            .filter(|c| c.starts_with("status"))
            .count();
        assert_eq!(polls_before, polls_after);
        // Giving up locally asks the provider for a refund; here it agrees.
        assert!(mock.calls().iter().any(|c| c == "cancel 777"));
        assert_eq!(app.numbers[0].status, NumberStatus::Cancelled);
        assert_eq!(
            app.snack_text(),
            Some("Number cancelled · $0.25 refunded to Hero SMS")
        );
    }

    #[test]
    fn no_numbers_removes_card_and_toasts() {
        let mut m = MockBackend::new(ProviderKind::HeroSms);
        m.activation = None;
        let mock = Arc::new(m);
        let (mut app, _) = app_with(
            config_with_keys(&[ProviderKind::HeroSms]),
            vec![(ProviderKind::HeroSms, mock)],
        );
        app.tick();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        assert_eq!(app.numbers.len(), 1);
        app.tick();
        assert!(app.numbers.is_empty());
        assert_eq!(
            app.snack_text(),
            Some(
                "Hero SMS: no numbers available for Telegram · United States. Try another country."
            )
        );
        assert_eq!(app.snack_kind(), Some(SnackKind::Error));
    }

    #[test]
    fn cancel_waits_out_the_provider_grace_period() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        let n = &app.numbers[0];
        assert_eq!(n.status, NumberStatus::Waiting);
        assert_eq!(
            n.cancelable_at,
            Some(app.now + Duration::from_secs(120)),
            "Hero SMS refuses cancels for two minutes, so the button waits from the start"
        );
        // Clicking during the grace period is a no-op: no request, no snack.
        app.apply(Action::CancelNumber(id));
        app.tick();
        assert!(!app.numbers[0].cancel_pending);
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        assert!(!mock.calls().iter().any(|c| c.starts_with("cancel")));
        assert!(app.snack_text().is_none());
        // One second short of the deadline still waits …
        app.fast_forward(Duration::from_secs(119));
        app.apply(Action::CancelNumber(id));
        assert!(!mock.calls().iter().any(|c| c.starts_with("cancel")));
        // … and once it passes the cancel goes through.
        app.fast_forward(Duration::from_secs(120));
        app.apply(Action::CancelNumber(id));
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Cancelled);
        assert!(mock.calls().iter().any(|c| c == "cancel 777"));
    }

    #[test]
    fn providers_without_a_grace_period_cancel_at_once() {
        let mock = Arc::new(MockBackend::new(ProviderKind::FiveSim));
        let (mut app, _) = app_with(
            config_with_keys(&[ProviderKind::FiveSim]),
            vec![(ProviderKind::FiveSim, mock.clone())],
        );
        app.tick();
        app.walk_to_offer(ProviderKind::FiveSim);
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        assert_eq!(app.numbers[0].cancelable_at, None);
        app.apply(Action::CancelNumber(id));
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Cancelled);
        assert!(mock.calls().iter().any(|c| c == "cancel 777"));
    }

    #[test]
    fn cancel_success_and_early_denied() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.fast_forward(CANCEL_GRACE);
        app.apply(Action::CancelNumber(id));
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Cancelled);
        assert!(!app.numbers[0].cancel_pending);
        assert_eq!(
            app.snack_text(),
            Some("Number cancelled · $0.25 refunded to Hero SMS")
        );
        assert!(mock.calls().iter().any(|c| c == "cancel 777"));

        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.handle_event(Event::CancelDone {
            local_id: id,
            result: Err(ApiError::EarlyCancelDenied),
        });
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        assert!(app.numbers[0].cancelable_at.is_some_and(|t| t > app.now));
        assert_eq!(
            app.snack_text(),
            Some("Hero SMS: number can be cancelled in 2:00")
        );
        assert_eq!(app.snack_kind(), Some(SnackKind::Info));
        // Still in the grace period → the click is ignored.
        let cancels = mock
            .calls()
            .iter()
            .filter(|c| c.starts_with("cancel"))
            .count();
        app.apply(Action::CancelNumber(id));
        assert_eq!(
            mock.calls()
                .iter()
                .filter(|c| c.starts_with("cancel"))
                .count(),
            cancels
        );
        app.handle_event(Event::CancelDone {
            local_id: id,
            result: Err(ApiError::Other("NO_WAY".into())),
        });
        assert_eq!(app.snack_kind(), Some(SnackKind::Error));
    }

    #[test]
    fn dismiss_received_completes_the_activation() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        mock.set_status(ActivationStatus::Ok {
            code: "1111".into(),
        });
        app.fast_forward(Duration::from_secs(6));
        assert_eq!(app.numbers[0].status, NumberStatus::Received);
        app.apply(Action::DismissNumber(id));
        app.tick();
        assert!(app.numbers.is_empty());
        assert!(mock.calls().iter().any(|c| c == "complete 777"));
        // Expired numbers are dismissed without a provider call.
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.handle_event(Event::Polled {
            local_id: id,
            result: Ok(ActivationStatus::Expired),
        });
        app.apply(Action::DismissNumber(id));
        assert_eq!(
            mock.calls()
                .iter()
                .filter(|c| c.starts_with("complete"))
                .count(),
            1
        );
    }

    #[test]
    fn stale_wizard_responses_are_ignored() {
        let (mut app, _, _) = hero_app();
        app.apply(Action::PickProvider(ProviderKind::HeroSms));
        app.tick();
        let old_gen = app.generation;
        app.apply(Action::PickProvider(ProviderKind::HeroSms));
        assert!(app.loading_services);
        app.handle_event(Event::ServicesLoaded {
            kind: ProviderKind::HeroSms,
            generation: old_gen,
            result: Ok(Vec::new()),
        });
        assert!(
            app.loading_services,
            "old generation must not clear the loader"
        );
        app.tick();
        assert!(!app.loading_services);
        assert_eq!(app.services.len(), 2);
        app.handle_event(Event::ServicesLoaded {
            kind: ProviderKind::FiveSim,
            generation: app.generation,
            result: Ok(Vec::new()),
        });
        assert_eq!(app.services.len(), 2);
    }

    #[test]
    fn settings_connect_stores_key_only_on_success() {
        let good = Arc::new(MockBackend::new(ProviderKind::TigerSms));
        let bad = Arc::new(MockBackend::new(ProviderKind::FiveSim).failing("nope"));
        let (mut app, path) = app_with(
            Config::default(),
            vec![
                (ProviderKind::TigerSms, good.clone()),
                (ProviderKind::FiveSim, bad),
            ],
        );
        app.apply(Action::SetKeyInput(ProviderKind::TigerSms, "short".into()));
        app.apply(Action::Connect(ProviderKind::TigerSms));
        assert!(!app.slot(ProviderKind::TigerSms).connecting);
        assert_eq!(
            app.snack_text(),
            Some("API key looks too short. Check it in your Tiger SMS dashboard.")
        );

        app.apply(Action::SetKeyInput(
            ProviderKind::TigerSms,
            "  tiger-key-1234  ".into(),
        ));
        app.apply(Action::Connect(ProviderKind::TigerSms));
        assert!(app.slot(ProviderKind::TigerSms).connecting);
        app.tick();
        let s = app.slot(ProviderKind::TigerSms);
        assert!(s.connected && s.balance == Some(10.0));
        assert_eq!(s.key.as_deref(), Some("tiger-key-1234"));
        assert_eq!(
            app.config
                .keys
                .get(&ProviderKind::TigerSms)
                .map(String::as_str),
            Some("tiger-key-1234")
        );
        assert_eq!(app.snack_text(), Some("Tiger SMS connected."));
        let on_disk = Config::load_from(&path);
        assert_eq!(
            on_disk
                .keys
                .get(&ProviderKind::TigerSms)
                .map(String::as_str),
            Some("tiger-key-1234")
        );

        // 5SIM needs 32+ chars and fails on the wire: nothing stored.
        let key: String = "f".repeat(40);
        app.apply(Action::SetKeyInput(ProviderKind::FiveSim, key));
        app.apply(Action::Connect(ProviderKind::FiveSim));
        app.tick();
        let s = app.slot(ProviderKind::FiveSim);
        assert!(!s.connected && s.backend.is_none());
        assert!(!app.config.keys.contains_key(&ProviderKind::FiveSim));
        assert_eq!(app.snack_text(), Some("5SIM: provider error `nope`"));

        app.apply(Action::Disconnect(ProviderKind::TigerSms));
        let s = app.slot(ProviderKind::TigerSms);
        assert!(!s.connected && s.backend.is_none() && s.balance.is_none());
        assert!(!app.config.keys.contains_key(&ProviderKind::TigerSms));
        assert!(
            !Config::load_from(&path)
                .keys
                .contains_key(&ProviderKind::TigerSms)
        );
        assert_eq!(app.snack_text(), Some("Tiger SMS disconnected."));
        assert!(app.balances().is_empty());
    }

    #[test]
    fn favorites_round_trip() {
        let (mut app, mock, path) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        let tier = app.offer_groups[0].tiers[0].clone();
        let fav = app.favorite_for(ANY_OPERATOR, &tier).unwrap();
        assert_eq!(fav.country_code, "US");
        assert_eq!(fav.dial.as_deref(), Some("+1"));
        assert!(!app.is_fav(&fav));
        app.apply(Action::ToggleFav(fav.clone()));
        assert!(app.is_fav(&fav));
        assert_eq!(Config::load_from(&path).favorites, vec![fav.clone()]);
        app.apply(Action::ToggleFav(fav.clone()));
        assert!(!app.is_fav(&fav));
        app.apply(Action::ToggleFav(fav.clone()));

        // Request from the favorite reuses the stored selector and dialling prefix, even when
        // the wizard no longer shows that country.
        app.apply(Action::PickProvider(ProviderKind::HeroSms));
        app.tick();
        assert!(app.countries.is_empty());
        app.apply(Action::RequestFav(0));
        assert_eq!(app.numbers[0].status, NumberStatus::Requesting);
        assert_eq!(app.numbers[0].dial.as_deref(), Some("+1"));
        app.tick();
        assert_eq!(app.numbers[0].status, NumberStatus::Waiting);
        assert!(
            mock.calls()
                .iter()
                .any(|c| c == "buy tg 187 MaxPrice(0.25)")
        );

        // Favorite for a provider that is not connected.
        let mut other = fav.clone();
        other.provider = ProviderKind::SmsBower;
        app.favorites.push(other);
        app.apply(Action::RequestFav(1));
        assert_eq!(app.numbers.len(), 1);
        assert_eq!(
            app.snack_text(),
            Some("SMSBower is not connected. Add its API key in Settings.")
        );
        app.apply(Action::RemoveFav(1));
        app.apply(Action::RemoveFav(0));
        assert!(app.favorites.is_empty());
        assert!(Config::load_from(&path).favorites.is_empty());
    }

    #[test]
    fn config_persistence_restores_numbers_and_prefs() {
        let (mut app, mock, path) = hero_app();
        app.apply(Action::TogglePref(PrefKey::StripDial));
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        app.apply(Action::RequestNumber);
        app.tick();
        // One waiting, one already past its deadline, one still requesting (must be dropped).
        app.numbers[1].expires_at = Some(SystemTime::now() - Duration::from_secs(5));
        app.numbers.push(Number::requesting(
            99,
            ProviderKind::HeroSms,
            ServiceCode::from("tg"),
            "Telegram",
            CountryRef::Id(187),
            "United States",
            None,
            0.1,
        ));
        app.persist();
        let saved = Config::load_from(&path);
        assert!(saved.prefs.strip_dial);
        assert_eq!(saved.numbers.len(), 3);
        assert_eq!(saved.next_number_id, app.config.next_number_id);

        let polls = mock.calls().iter().filter(|c| c == &"status 777").count();
        let (mut again, _) = app_with(saved, vec![(ProviderKind::HeroSms, mock.clone())]);
        again.tick();
        assert!(again.prefs.strip_dial);
        assert_eq!(again.numbers.len(), 2);
        assert_eq!(again.numbers[0].status, NumberStatus::Waiting);
        assert_eq!(again.numbers[1].status, NumberStatus::Expired);
        assert_eq!(again.config.next_number_id, app.config.next_number_id);
        // The live one was polled on the very first tick.
        assert_eq!(
            mock.calls().iter().filter(|c| c == &"status 777").count(),
            polls + 1
        );
        // Ids keep growing rather than colliding.
        again.apply(Action::RequestFav(0));
        again.walk_to_offer(ProviderKind::HeroSms);
        again.apply(Action::RequestNumber);
        let ids: Vec<u32> = again.numbers.iter().map(|n| n.id).collect();
        let mut dedup = ids.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(ids.len(), dedup.len());
    }

    #[test]
    fn copy_respects_strip_dial_pref() {
        let (mut app, mock, _) = hero_app();
        app.walk_to_offer(ProviderKind::HeroSms);
        app.apply(Action::RequestNumber);
        app.tick();
        let id = app.numbers[0].id;
        app.apply(Action::CopyPhone(id));
        assert_eq!(app.take_clipboard().as_deref(), Some("+1 202 555 0123"));
        assert!(app.copied_is(&format!("{id}-p")));
        app.apply(Action::TogglePref(PrefKey::StripDial));
        app.apply(Action::CopyPhone(id));
        assert_eq!(app.take_clipboard().as_deref(), Some("202 555 0123"));
        mock.set_status(ActivationStatus::Ok {
            code: "39 284".into(),
        });
        app.fast_forward(Duration::from_secs(6));
        app.apply(Action::CopyCode(id));
        assert_eq!(app.take_clipboard().as_deref(), Some("39284"));
        app.fast_forward(Duration::from_secs(2));
        assert!(app.copied.is_none());
    }

    #[test]
    fn sort_cycles_and_orders_countries() {
        let mut m = MockBackend::new(ProviderKind::HeroSms);
        m.countries.push(CountryRow {
            key: CountryRef::Id(16),
            name: "England".into(),
            code: "GB".into(),
            dial: Some("+44".into()),
            price: 0.1,
            count: 0,
        });
        m.countries.push(CountryRow {
            key: CountryRef::Id(48),
            name: "Netherlands".into(),
            code: "NL".into(),
            dial: None,
            price: 0.9,
            count: 12,
        });
        let (mut app, _) = app_with(
            config_with_keys(&[ProviderKind::HeroSms]),
            vec![(ProviderKind::HeroSms, Arc::new(m))],
        );
        app.tick();
        app.apply(Action::PickProvider(ProviderKind::HeroSms));
        app.tick();
        let tg = app.services[0].clone();
        app.apply(Action::PickService(tg));
        app.tick();
        app.apply(Action::ToggleSort);
        assert_eq!(app.sort_dir, Some(SortDir::Asc));
        let asc: Vec<f64> = app.country_rows().iter().map(|c| c.price).collect();
        assert!(asc.windows(2).all(|w| w[0] <= w[1]));
        app.apply(Action::ToggleSort);
        let desc: Vec<f64> = app.country_rows().iter().map(|c| c.price).collect();
        assert!(desc.windows(2).all(|w| w[0] >= w[1]));
        app.apply(Action::ToggleSort);
        assert_eq!(app.sort_dir, None);
        app.apply(Action::SetSearch("gb".into()));
        assert_eq!(app.country_rows().len(), 1);
        assert_eq!(app.country_rows()[0].name, "England");
    }

    #[test]
    fn snack_expires_and_helpers_format() {
        let (mut app, _, _) = hero_app();
        app.toast("hi", SnackKind::Info);
        assert!(app.busy());
        app.fast_forward(Duration::from_secs(5));
        assert!(app.snack.is_none());
        assert_eq!(with_plus("123"), "+123");
        assert_eq!(with_plus("+123"), "+123");
        assert_eq!(
            provider_error(ProviderKind::FiveSim, &ApiError::NoBalance),
            "5SIM: insufficient balance"
        );
        assert_eq!(app.step_title(), "Choose a provider");
        assert_eq!(app.search_placeholder(), "Search providers");
        assert_eq!(app.provider_rows().len(), 4);
        app.apply(Action::SetSearch("sim".into()));
        assert_eq!(app.provider_rows().len(), 1);
    }
}
