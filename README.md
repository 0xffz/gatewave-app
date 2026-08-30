# Number Desk — egui mock-up

A Rust/[egui](https://github.com/emilk/egui) mock-up of the **Number Desk** SMS-number client, ported 1:1 from the
Claude Design file in [`design/Number Desk.dc.html`](design/Number%20Desk.dc.html). Everything is simulated in-process:
no network, no real providers.

```sh
cargo run
```

## What's in the mock

- Three-column layout: sidebar (nav · 4-step tracker · balances), main content, "Active numbers" panel.
- 4-step wizard: provider → service → country (search + price sort) → operator/price tiers, with skeleton loaders,
  a sticky request bar and starrable offers.
- Favorites and Settings (behaviour toggles, provider API-key connect/disconnect with masked keys).
- Number lifecycle: requesting → waiting (15-min countdown, cancel & refund) → received / expired / cancelled,
  with snackbar toasts and real clipboard copy.

Simulation knobs live in `sim::SimConfig` (code delay, failure rate, inverted "received" card).

## Jump straight to a screen

`NUMBER_DESK_SCENE` starts the app in a given state — handy for demos and screenshots:

```sh
NUMBER_DESK_SCENE=offer cargo run
```

Scenes: `step2`, `step3`, `step4`, `offer`, `requesting`, `favorites`, `settings`, `connecting`, `snack`, `empty`.

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

API keys are read from the git-ignored `.env` (`HERO_SMS_API_KEY`, `SMSBOWER_API_KEY`, `TIGER_SMS_API_KEY`, `FIVESIM_API_KEY`). `cargo test -p sms-activate` is fully offline (fixture-driven); the `tests/*_live.rs` suites run read-only calls against the real APIs only when the matching key is exported, and never buy numbers.

## Layout of the code

| Module        | Role                                                                 |
| ------------- | -------------------------------------------------------------------- |
| `model.rs`    | Pure data + deterministic pricing/offer logic (unit-tested)          |
| `sim.rs`      | Deferred-event scheduler (stands in for `setTimeout`) and a tiny RNG |
| `app.rs`      | App state, actions, simulated behaviour, demo scenes                 |
| `theme.rs`    | Palette, fonts (Space Grotesk / JetBrains Mono, embedded), style     |
| `ui/`         | One file per region: sidebar, wizard, favorites, settings, numbers, snackbar; `widgets.rs` holds the custom widgets |

`cargo test` covers the model, the app behaviour and runs headless egui frames over every scene.

Fonts in `assets/fonts/` are OFL-licensed (Space Grotesk, JetBrains Mono).
