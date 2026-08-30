//! HTTP transport abstraction. The sms-activate family only needs `GET url → (status, body)`;
//! REST-style providers (5SIM) also need methods, headers and bodies, hence [`HttpRequest`].

use std::collections::VecDeque;
use std::sync::Mutex;
#[cfg(feature = "ureq")]
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

/// A full HTTP request. Build with [`HttpRequest::get`] / [`HttpRequest::new`] and the chainable helpers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl HttpRequest {
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new(Method::Get, url)
    }

    pub fn post(url: impl Into<String>) -> Self {
        Self::new(Method::Post, url)
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// `Authorization: Bearer <token>`
    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", format!("Bearer {token}"))
    }

    /// `Accept: application/json`
    pub fn accept_json(self) -> Self {
        self.header("Accept", "application/json")
    }

    /// Sets a JSON body (and `Content-Type: application/json`).
    pub fn json_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self.header("Content-Type", "application/json")
    }

    /// Value of a header, case-insensitively.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// True for a bare `GET` with no headers and no body — what [`Transport::get`] can serve.
    pub fn is_plain_get(&self) -> bool {
        self.method == Method::Get && self.headers.is_empty() && self.body.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }
}

/// Network-level failure (DNS, TCP, TLS, timeout). HTTP error statuses are *not* transport errors —
/// they are returned as [`HttpResponse`] so dialects can read the provider's error body.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct TransportError(pub String);

pub trait Transport: Send + Sync {
    /// Plain `GET` — everything the sms-activate protocol family needs.
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError>;

    /// Full request with method, headers and body. The default serves plain GETs through
    /// [`Transport::get`] and refuses anything else; real transports override it.
    fn request(&self, req: &HttpRequest) -> Result<HttpResponse, TransportError> {
        if req.is_plain_get() {
            self.get(&req.url)
        } else {
            Err(TransportError(
                "this transport only supports plain GET requests (no headers/body)".into(),
            ))
        }
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        (**self).get(url)
    }
    fn request(&self, req: &HttpRequest) -> Result<HttpResponse, TransportError> {
        (**self).request(req)
    }
}

impl<T: Transport + ?Sized> Transport for std::sync::Arc<T> {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        (**self).get(url)
    }
    fn request(&self, req: &HttpRequest) -> Result<HttpResponse, TransportError> {
        (**self).request(req)
    }
}

/// Default blocking transport built on `ureq`.
#[cfg(feature = "ureq")]
#[derive(Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

#[cfg(feature = "ureq")]
impl UreqTransport {
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    pub fn new() -> Self {
        Self::with_timeout(Self::DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .user_agent("sms-activate-rs/0.1")
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }

    pub fn from_agent(agent: ureq::Agent) -> Self {
        Self { agent }
    }

    fn finish(mut resp: ureq::http::Response<ureq::Body>) -> Result<HttpResponse, TransportError> {
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| TransportError(e.to_string()))?;
        Ok(HttpResponse { status, body })
    }
}

#[cfg(feature = "ureq")]
impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "ureq")]
impl Transport for UreqTransport {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        let resp = self
            .agent
            .get(url)
            .call()
            .map_err(|e| TransportError(e.to_string()))?;
        Self::finish(resp)
    }

    fn request(&self, req: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let err = |e: ureq::Error| TransportError(e.to_string());
        let resp = match req.method {
            // 5SIM's `DELETE /user/max-prices` carries a JSON body; ureq needs an explicit opt-in.
            Method::Delete if req.body.is_some() => {
                let mut b = self.agent.delete(&req.url).force_send_body();
                for (k, v) in &req.headers {
                    b = b.header(k.as_str(), v.as_str());
                }
                b.send(req.body.as_deref().unwrap_or("")).map_err(err)?
            }
            Method::Get | Method::Delete => {
                let mut b = if req.method == Method::Get {
                    self.agent.get(&req.url)
                } else {
                    self.agent.delete(&req.url)
                };
                for (k, v) in &req.headers {
                    b = b.header(k.as_str(), v.as_str());
                }
                b.call().map_err(err)?
            }
            Method::Post | Method::Put => {
                let mut b = if req.method == Method::Post {
                    self.agent.post(&req.url)
                } else {
                    self.agent.put(&req.url)
                };
                for (k, v) in &req.headers {
                    b = b.header(k.as_str(), v.as_str());
                }
                b.send(req.body.as_deref().unwrap_or("")).map_err(err)?
            }
        };
        Self::finish(resp)
    }
}

/// Scripted transport for tests: hands out queued responses in order and records every request.
#[derive(Default)]
pub struct FakeTransport {
    responses: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a response (builder style).
    pub fn push(self, status: u16, body: impl Into<String>) -> Self {
        self.responses
            .lock()
            .unwrap()
            .push_back(Ok(HttpResponse::new(status, body)));
        self
    }

    /// Queue a transport failure.
    pub fn push_error(self, msg: impl Into<String>) -> Self {
        self.responses
            .lock()
            .unwrap()
            .push_back(Err(TransportError(msg.into())));
        self
    }

    /// URLs requested so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.url.clone())
            .collect()
    }

    /// Full requests (method, headers, body) so far, in order.
    pub fn recorded(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Transport for FakeTransport {
    fn get(&self, url: &str) -> Result<HttpResponse, TransportError> {
        self.request(&HttpRequest::get(url))
    }

    fn request(&self, req: &HttpRequest) -> Result<HttpResponse, TransportError> {
        self.requests.lock().unwrap().push(req.clone());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(TransportError(format!(
                    "FakeTransport: no response queued for {}",
                    req.url
                )))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_builder_and_fake_recording() {
        let req = HttpRequest::get("https://x.test/v1/user/profile")
            .bearer("tok")
            .accept_json();
        assert_eq!(req.header_value("authorization"), Some("Bearer tok"));
        assert!(!req.is_plain_get());
        let t = FakeTransport::new().push(200, "{}");
        t.request(&req).unwrap();
        assert_eq!(t.requests(), vec!["https://x.test/v1/user/profile"]);
        assert_eq!(t.recorded()[0].headers.len(), 2);
        let p = HttpRequest::post("https://x.test").json_body("{\"a\":1}");
        assert_eq!(p.header_value("Content-Type"), Some("application/json"));
    }

    #[test]
    fn default_request_refuses_headers() {
        struct GetOnly;
        impl Transport for GetOnly {
            fn get(&self, _url: &str) -> Result<HttpResponse, TransportError> {
                Ok(HttpResponse::new(200, "ok"))
            }
        }
        assert!(GetOnly.request(&HttpRequest::get("u")).is_ok());
        assert!(GetOnly.request(&HttpRequest::get("u").bearer("t")).is_err());
    }
}
