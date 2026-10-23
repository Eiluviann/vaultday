//! HTTP behind a small port, so every test runs against a loopback mock
//! server. [`UreqTransport`] is the real one: rustls over ring with the
//! Mozilla roots. A request's `Authorization` header is never printed.

use std::fmt;
use std::time::Duration;

use crate::error::TogglError;

/// The methods the Toggl client uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `GET`.
    Get,
    /// `POST` with a JSON body.
    Post,
    /// `PUT` with a JSON body.
    Put,
    /// `DELETE`.
    Delete,
}

/// One request.
#[derive(Clone)]
pub struct HttpRequest {
    /// The method.
    pub method: Method,
    /// The full URL.
    pub url: String,
    /// Headers; `Authorization` among them is never printed.
    pub headers: Vec<(String, String)>,
    /// The JSON body of a `POST` or `PUT`.
    pub body: Option<String>,
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
pub trait TogglTransport: Send + Sync {
    /// Sends `request`; any status is a response, only transport failures are errors.
    ///
    /// # Errors
    /// [`TogglError::Network`] when the server cannot be reached.
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, TogglError>;
}

/// HTTP over ureq.
#[derive(Debug)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body_bytes", &self.body.as_ref().map_or(0, String::len))
            .finish()
    }
}

impl UreqTransport {
    /// A transport with a 30-second limit per request that follows no
    /// redirect; statuses are responses.
    #[must_use]
    pub fn new() -> UreqTransport {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            // A redirect is never followed: a client aimed at the loopback
            // mock must not be led to any other host, the live service
            // included. Toggl's v9 API answers in place.
            .max_redirects(0)
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

impl TogglTransport for UreqTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, TogglError> {
        let body = request.body.clone().unwrap_or_default();
        let outcome = match request.method {
            Method::Get => {
                let mut builder = self.agent.get(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(name.as_str(), value.as_str());
                }
                builder.call()
            }
            Method::Delete => {
                let mut builder = self.agent.delete(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(name.as_str(), value.as_str());
                }
                builder.call()
            }
            Method::Post | Method::Put => {
                let mut builder = if request.method == Method::Post {
                    self.agent.post(&request.url)
                } else {
                    self.agent.put(&request.url)
                };
                for (name, value) in &request.headers {
                    builder = builder.header(name.as_str(), value.as_str());
                }
                builder.send(body)
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

fn transport_error(error: ureq::Error) -> TogglError {
    TogglError::Network(format!("the request failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_debug_form_never_prints_header_values() {
        let request = HttpRequest {
            method: Method::Get,
            url: "http://127.0.0.1:1/api/v9/me".to_owned(),
            headers: vec![("Authorization".to_owned(), "Basic c2VjcmV0".to_owned())],
            body: None,
        };
        let printed = format!("{request:?}");
        assert!(printed.contains("Authorization"));
        assert!(!printed.contains("c2VjcmV0"));
    }
}
