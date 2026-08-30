# Gatewave

A Rust/[egui](https://github.com/emilk/egui) desktop client for buying SMS-verification numbers from
Hero SMS, 5SIM, Tiger SMS and SMSBower. The UI follows the Claude Design file in
[`design/Number Desk.dc.html`](design/Number%20Desk.dc.html); the data comes from the real providers
through the workspace crate `sms-activate`.

```sh
cargo run
```

## Setup

Put your API keys in a git-ignored `.env` in the working directory (or export them):

```sh
HERO_SMS_API_KEY=…
FIVESIM_API_KEY=…
TIGER_SMS_API_KEY=…
SMSBOWER_API_KEY=…
```

On every launch, any provider that has no key in the config file gets it from the environment / `.env`
and the key is stored in `~/.config/gatewave/config.json` (`$XDG_CONFIG_HOME` is honoured, and
`GATEWAVE_CONFIG=/path/to/file.json` overrides the location). Keys can also be pasted into
**Settings → Providers**; they are stored only after the provider accepts them. **Disconnect** removes a
key from the config, but while it is still in `.env` it is picked up again at the next start — delete it
from `.env` too to make the disconnect stick. The config file also keeps preferences, favorites and the
active numbers, so a restart resumes polling of numbers that are still waiting for an SMS.

Every provider connection is logged on stderr at startup, e.g. `gatewave: Hero SMS connected, balance 12.51`
or `gatewave: 5SIM: invalid API key`.

## What the app does

- Three-column layout: sidebar (nav · 4-step tracker · balances), main content, "Active numbers" panel.
- 4-step wizard: provider → service (searchable, can be ~1000 rows) → country (search + price sort, sold-out
  countries faded) → operator / partner price tiers, with skeleton loaders, a sticky request bar and starrable offers.
- **Request number** buys a number (this spends money on your provider balance). The card then polls the
  provider every few seconds, shows the code when it arrives (optionally copying it to the clipboard),
  and lets you cancel & refund while waiting — providers that impose a grace period show a "Cancel in m:ss" countdown.
  When the 15-minute window runs out without an SMS the app asks the provider to cancel the activation
  (refund when it is still open); activations the provider still lists as active at the next start are
  picked up again as waiting cards.
- Favorites remember provider · service · country · offer (including how the offer is bought) for one-click requests.
- Settings: behaviour toggles (sound, auto-copy, snackbar, strip dialling prefix) and per-provider connect / disconnect
  with masked keys.

Provider calls run on background threads (`worker.rs`); the UI never blocks on the network.

## Provider clients (`crates/sms-activate`)

Real provider integrations live in the workspace crate `sms-activate`, behind one object-safe trait:

```rust
use sms_activate::{SmsActivateApi, NumberRequest, providers::hero_sms::HeroSms};

let api: Box<dyn SmsActivateApi> = Box::new(HeroSms::with_api_key(std::env::var("HERO_SMS_API_KEY")?));
println!("{} balance: {}", api.provider(), api.get_balance()?);
let number = api.get_number(&NumberRequest::new("tg", 187).max_price(0.5))?; // spends money
```

| Provider | Module | Protocol | Notes |
| --- | --- | --- | --- |
| Hero-SMS | `providers::hero_sms` | sms-activate dialect | JSON error envelopes, `getNumbersStatus`/`getOperators`, opt-in retry transport |
| SMSBower | `providers::smsbower` | sms-activate dialect | slug-keyed top countries, `getPricesV2/V3`, provider filters |
| Tiger SMS | `providers::tiger_sms` | sms-activate dialect | OpenAPI-backed extras (`getOffers`, `getProviders`, `getStatusV2`…) |
| 5SIM | `providers::fivesim` | own REST API | countries/products are names; implements the trait directly |

Country keys are provider-native (`CountryRef::Id(187)` for the sms-activate family, `CountryRef::Slug("england")` for 5SIM) — resolve them through `get_countries()`.

`cargo test -p sms-activate` is fully offline (fixture-driven); the `tests/*_live.rs` suites run read-only calls against the real APIs only when the matching key is exported, and never buy numbers.

## Layout of the code

| Module        | Role                                                                 |
| ------------- | -------------------------------------------------------------------- |
| `backend.rs`  | `Backend` trait the app talks to, `RealBackend` over the `sms-activate` clients, a scriptable mock for tests |
| `domain.rs`   | App-level types: numbers, favorites, preferences, formatting helpers |
| `config.rs`   | JSON config (keys, prefs, favorites, numbers) and `.env` seeding     |
| `worker.rs`   | Background job runner (results arrive as events) and wall-clock timers |
| `app.rs`      | App state machine: provider slots, wizard, purchase / poll / cancel lifecycle, persistence |
| `theme.rs`    | Palette, IBM Plex Sans/Mono (embedded, OFL), text sizes, global style |
| `ui/`         | One file per region: sidebar, wizard, favorites, settings, numbers, snackbar; `widgets.rs` holds the custom widgets |
| `assets/`     | Embedded IBM Plex fonts and the app icon (`icon.png` 1024 px master, `icon-512.png` is what the binary embeds) |

`cargo test` covers the domain types, the app state machine over a mock backend (no network, nothing is ever
bought) and runs headless egui frames over every screen.

