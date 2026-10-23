//! HTTP for Graph and the OAuth endpoints, behind a small port so tests run
//! against an in-process server and the core never links an HTTP stack
//! twice. [`UreqTransport`] is the real one: rustls with the Mozilla roots.

use std::fmt;
use std::time::Duration;

use crate::error::MailError;

/// The methods the sources use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `GET`.
    Get,
    /// `POST` with a form body.
    Post,
}

/// One request.
#[derive(Clone)]
pub struct HttpRequest {
    /// The method.
    pub method: Method,
    /// The full URL.
    pub url: String,
    /// Extra headers; `Authorization` among them is never printed.
    pub headers: Vec<(String, String)>,
    /// The `application/x-www-form-urlencoded` body of a `POST`.
    pub form: Vec<(String, String)>,
}

/// One response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// The status code.
    pub status: u16,
    /// The `Retry-After` delay, when the server sent one in seconds.
    pub retry_after: Option<Duration>,
    /// The body as text.
    pub body: String,
}

/// Sends requests. Implementations block until the response is read.
pub trait HttpTransport: Send + Sync {
    /// Sends `request`; any status is a response, only transport failures are errors.
    ///
    /// # Errors
    /// [`MailError::Network`] when the server cannot be reached or the
    /// connection fails.
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, MailError>;
}

/// HTTP over ureq.
#[derive(Debug)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl HttpRequest {
    /// A `GET` of `url`.
    pub fn get(url: impl Into<String>) -> HttpRequest {
        HttpRequest {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            form: Vec::new(),
        }
    }

    /// A `POST` of `form` to `url`.
    pub fn post_form(url: impl Into<String>, form: &[(&str, &str)]) -> HttpRequest {
        HttpRequest {
            method: Method::Post,
            url: url.into(),
            headers: Vec::new(),
            form: form
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }

    /// The request with one more header.
    #[must_use]
    pub fn header(mut self, name: &str, value: impl Into<String>) -> HttpRequest {
        self.headers.push((name.to_owned(), value.into()));
        self
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        let form: Vec<&str> = self.form.iter().map(|(key, _)| key.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("form", &form)
            .finish()
    }
}

impl UreqTransport {
    /// A transport with a 30-second limit per request; statuses are responses.
    #[must_use]
    pub fn new() -> UreqTransport {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build();
        UreqTransport {
            agent: config.into(),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        UreqTransport::new()
    }
}

impl HttpTransport for UreqTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, MailError> {
        let outcome = match request.method {
            Method::Get => {
                let mut builder = self.agent.get(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(name.as_str(), value.as_str());
                }
                builder.call()
            }
            Method::Post => {
                let mut builder = self.agent.post(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(name.as_str(), value.as_str());
                }
                builder.send_form(
                    request
                        .form
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.as_str())),
                )
            }
        };
        let mut response = outcome.map_err(transport_error)?;
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(transport_error)?;
        Ok(HttpResponse {
            status,
            retry_after,
            body,
        })
    }
}

fn transport_error(error: ureq::Error) -> MailError {
    MailError::network(format!("the request failed: {error}"))
}
