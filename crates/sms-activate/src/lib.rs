//! Client for the **sms-activate API family** — the de-facto protocol (`handler_api.php?action=…`)
//! that most virtual-number providers clone — with per-provider dialects.
//!
//! ```text
//!  SmsActivateApi (trait)  ◄── what the app uses (object-safe)
//!        ▲                    ▲
//!  Client<T: Transport, D: Dialect>      FiveSim<T>  ◄── REST providers implement the trait directly
//!        │            │
//!   UreqTransport   HeroSmsDialect / SmsBowerDialect / TigerSmsDialect  ◄── per-provider quirks
//! ```
//!
//! ```no_run
//! use sms_activate::{SmsActivateApi, providers::hero_sms::HeroSms};
//!
//! let hero = HeroSms::with_api_key(std::env::var("HERO_SMS_API_KEY").unwrap());
//! println!("balance: {}", hero.get_balance().unwrap());
//! ```

pub mod api;
pub mod error;
pub mod protocol;
pub mod providers;
pub mod transport;
pub mod types;

pub use api::{Client, Dialect, SmsActivateApi};
pub use error::{ApiError, ApiResult};
#[cfg(feature = "ureq")]
pub use transport::UreqTransport;
pub use transport::{FakeTransport, HttpRequest, HttpResponse, Method, Transport, TransportError};
pub use types::*;
