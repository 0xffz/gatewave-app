//! Concrete providers. sms-activate-compatible providers define a [`crate::Dialect`] plus a type
//! alias over [`crate::Client`]; REST providers (5SIM) implement [`crate::SmsActivateApi`] directly.
//! All expose `with_api_key` constructors over the default transport.

pub mod fivesim;
pub mod hero_sms;
pub mod smsbower;
pub mod tiger_sms;
