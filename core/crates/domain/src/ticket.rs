//! The generic ticket provider every workspace gets from its settings:
//! `ticket_pattern` spots ticket numbers in text, `ticket_url` builds the link
//! to a ticket with `{field}` substituted from the story's frontmatter, and
//! `hosts` say which workspace a captured URL belongs to.
//!
//! This is the base version's only ticket provider. A feature set (ServiceNow,
//! Azure DevOps) plugs a richer one into the registry's ticket-provider slot
//! later; nothing here depends on one.

use std::collections::BTreeMap;

use regex::Regex;

use crate::error::{DomainError, Result};
use crate::task::StoryNumber;
use crate::workspace::WorkspaceId;

/// The frontmatter field that always holds the ticket number in a URL template.
pub const TICKET_FIELD: &str = "ticket";

/// A compiled `ticket_pattern`.
#[derive(Debug, Clone)]
pub struct TicketPattern {
    source: String,
    anywhere: Regex,
    whole: Regex,
}

impl TicketPattern {
    /// Compiles a workspace's `ticket_pattern`.
    ///
    /// # Errors
    /// [`DomainError::InvalidTicketPattern`] when the regex does not compile.
    pub fn new(pattern: &str) -> Result<Self> {
        let compile = |text: &str| {
            Regex::new(text).map_err(|error| DomainError::InvalidTicketPattern {
                pattern: pattern.to_owned(),
                reason: error.to_string(),
            })
        };
        Ok(Self {
            source: pattern.to_owned(),
            anywhere: compile(pattern)?,
            whole: compile(&format!("^(?:{pattern})$"))?,
        })
    }

    /// The pattern as configured.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Whether the whole token is a ticket number. An empty token never is,
    /// whatever the pattern accepts.
    pub fn matches(&self, token: &str) -> bool {
        !token.is_empty() && self.whole.is_match(token)
    }

    /// Every ticket number in `text`, left to right, without overlaps. Empty
    /// matches of a pattern that accepts nothing are left out.
    pub fn find_all<'t>(&self, text: &'t str) -> Vec<&'t str> {
        self.anywhere
            .find_iter(text)
            .map(|found| found.as_str())
            .filter(|found| !found.is_empty())
            .collect()
    }
}

impl PartialEq for TicketPattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for TicketPattern {}

/// A workspace's ticket settings, compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketProvider {
    pattern: Option<TicketPattern>,
    url_template: Option<String>,
    hosts: Vec<String>,
}

impl TicketProvider {
    /// Builds the provider from the three settings; each may be absent.
    ///
    /// # Errors
    /// [`DomainError::InvalidTicketPattern`] when the pattern does not compile.
    pub fn from_settings(
        ticket_pattern: Option<&str>,
        ticket_url: Option<&str>,
        hosts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self> {
        Ok(Self {
            pattern: ticket_pattern.map(TicketPattern::new).transpose()?,
            url_template: ticket_url.map(str::to_owned),
            hosts: hosts.into_iter().map(Into::into).collect(),
        })
    }

    /// The compiled pattern, when the workspace has one.
    pub fn pattern(&self) -> Option<&TicketPattern> {
        self.pattern.as_ref()
    }

    /// The hosts this workspace's links point at.
    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }

    /// Every ticket number in `text`; empty without a pattern.
    pub fn find_tickets<'t>(&self, text: &'t str) -> Vec<&'t str> {
        self.pattern
            .as_ref()
            .map_or_else(Vec::new, |pattern| pattern.find_all(text))
    }

    /// Whether the whole token is a ticket number; false without a pattern.
    pub fn is_ticket(&self, token: &str) -> bool {
        self.pattern
            .as_ref()
            .is_some_and(|pattern| pattern.matches(token))
    }

    /// The link to a ticket: the template with `{ticket}` and every other
    /// `{field}` substituted from the story's frontmatter. `None` without a template.
    ///
    /// # Errors
    /// [`DomainError::MissingTicketField`] when the template names a field the
    /// frontmatter lacks.
    pub fn ticket_url(
        &self,
        ticket: &StoryNumber,
        frontmatter: &BTreeMap<String, String>,
    ) -> Result<Option<String>> {
        let Some(template) = &self.url_template else {
            return Ok(None);
        };
        let lookup = |field: &str| {
            if field == TICKET_FIELD {
                Some(ticket.as_str().to_owned())
            } else {
                frontmatter.get(field).cloned()
            }
        };
        substitute_fields(template, lookup).map(Some)
    }

    /// Whether `host` is one of this workspace's hosts or a subdomain of one.
    pub fn owns_host(&self, host: &str) -> bool {
        self.hosts.iter().any(|known| host_matches(host, known))
    }

    /// Whether the URL's host belongs to this workspace.
    pub fn owns_url(&self, url: &str) -> bool {
        host_of(url).is_some_and(|host| self.owns_host(host))
    }
}

/// Fills a URL template: each `{field}` becomes `lookup(field)`. Braces that
/// do not close, or an empty `{}`, stay as written.
///
/// # Errors
/// [`DomainError::MissingTicketField`] when `lookup` has no value for a field.
pub fn substitute_fields(
    template: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<String> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let (before, from_brace) = rest.split_at(open);
        output.push_str(before);
        let Some(close) = from_brace.find('}') else {
            output.push_str(from_brace);
            return Ok(output);
        };
        let field = &from_brace[1..close];
        if field.is_empty() {
            output.push_str("{}");
        } else {
            let value = lookup(field).ok_or_else(|| DomainError::MissingTicketField {
                field: field.to_owned(),
            })?;
            output.push_str(&value);
        }
        rest = &from_brace[close + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

/// The workspace whose hosts a URL points at; the first match wins.
pub fn route_url<'a>(
    url: &str,
    workspaces: impl IntoIterator<Item = (&'a WorkspaceId, &'a TicketProvider)>,
) -> Option<&'a WorkspaceId> {
    let host = host_of(url)?;
    workspaces
        .into_iter()
        .find(|(_, provider)| provider.owns_host(host))
        .map(|(workspace, _)| workspace)
}

/// The host of a URL, without scheme, user info, port, path or query. `None`
/// when the text has no `scheme://host` shape.
pub fn host_of(url: &str) -> Option<&str> {
    let (scheme, after_scheme) = url.split_once("://")?;
    let is_scheme = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !is_scheme {
        return None;
    }
    // A backslash ends the authority too: browsers read `\` as `/` there.
    let authority = after_scheme
        .split(['/', '\\', '?', '#'])
        .next()
        .unwrap_or_default();
    let host_and_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    // A port is digits or empty; a bracketed IPv6 host without a port fails this and stays whole.
    let host = host_and_port
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
        .map_or(host_and_port, |(host, _)| host);
    (!host.is_empty()).then_some(host)
}

fn host_matches(host: &str, known: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let known = known.to_ascii_lowercase();
    host == known || host.ends_with(&format!(".{known}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_provider() -> TicketProvider {
        TicketProvider::from_settings(
            Some(r"(WEB|ACME)-\d+"),
            Some("https://tracker.example.test/browse/{ticket}"),
            ["tracker.example.test"],
        )
        .unwrap()
    }

    #[test]
    fn the_pattern_spots_ticket_numbers_in_text_and_whole_tokens() {
        let provider = mock_provider();
        assert_eq!(
            provider.find_tickets("WEB-142 blocks ACME-9; see WEB-157."),
            ["WEB-142", "ACME-9", "WEB-157"]
        );
        assert!(provider.is_ticket("WEB-142"));
        assert!(!provider.is_ticket("WEB-142,"));
        assert!(!provider.is_ticket("xWEB-142"));
        assert!(!provider.is_ticket("STRY0601234"));
        assert_eq!(provider.pattern().unwrap().source(), r"(WEB|ACME)-\d+");
    }

    #[test]
    fn an_invalid_pattern_is_refused_and_a_missing_one_matches_nothing() {
        let error = TicketPattern::new("(WEB").unwrap_err();
        assert!(matches!(
            error,
            DomainError::InvalidTicketPattern { ref pattern, .. } if pattern == "(WEB"
        ));
        let empty_ok = TicketPattern::new(r"\d*").unwrap();
        assert!(
            empty_ok
                .find_all("ab 12 c")
                .iter()
                .all(|found| !found.is_empty())
        );
        assert_eq!(empty_ok.find_all("ab 12 c"), ["12"]);
        assert!(!empty_ok.matches(""));
        let bare = TicketProvider::from_settings(None, None, Vec::<String>::new()).unwrap();
        assert!(bare.find_tickets("WEB-142").is_empty());
        assert!(!bare.is_ticket("WEB-142"));
        assert_eq!(
            bare.ticket_url(&StoryNumber::new("WEB-142"), &BTreeMap::new())
                .unwrap(),
            None
        );
    }

    #[test]
    fn ticket_urls_substitute_the_number_and_frontmatter_fields() {
        let provider = mock_provider();
        assert_eq!(
            provider
                .ticket_url(&StoryNumber::new("WEB-142"), &BTreeMap::new())
                .unwrap(),
            Some("https://tracker.example.test/browse/WEB-142".to_owned())
        );

        let by_fields = TicketProvider::from_settings(
            Some(r"STRY\d{7}"),
            Some("https://desk.example.test/nav_to.do?uri={source_kind}.do?sys_id={source_id}"),
            ["desk.example.test"],
        )
        .unwrap();
        let frontmatter: BTreeMap<String, String> =
            [("source_kind", "rm_story"), ("source_id", "3f9a1c2b")]
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect();
        assert_eq!(
            by_fields
                .ticket_url(&StoryNumber::new("STRY0601234"), &frontmatter)
                .unwrap(),
            Some("https://desk.example.test/nav_to.do?uri=rm_story.do?sys_id=3f9a1c2b".to_owned())
        );
        assert_eq!(
            by_fields.ticket_url(&StoryNumber::new("STRY0601234"), &BTreeMap::new()),
            Err(DomainError::MissingTicketField {
                field: "source_kind".to_owned()
            })
        );
    }

    #[test]
    fn odd_braces_in_a_template_stay_as_written() {
        let lookup = |field: &str| (field == "a").then(|| "1".to_owned());
        assert_eq!(substitute_fields("x{a}y", lookup).unwrap(), "x1y");
        assert_eq!(substitute_fields("x{}y{a}", lookup).unwrap(), "x{}y1");
        assert_eq!(substitute_fields("x{a", lookup).unwrap(), "x{a");
        assert_eq!(substitute_fields("plain", lookup).unwrap(), "plain");
        assert_eq!(substitute_fields("", lookup).unwrap(), "");
    }

    #[test]
    fn hosts_route_a_url_to_its_workspace() {
        let acme = mock_provider();
        let home =
            TicketProvider::from_settings(None, None, ["github.com", "Example.org"]).unwrap();
        let acme_id = WorkspaceId::new("acme");
        let home_id = WorkspaceId::new("home");
        let workspaces = [(&acme_id, &acme), (&home_id, &home)];

        assert_eq!(
            route_url("https://tracker.example.test/browse/WEB-142", workspaces),
            Some(&acme_id)
        );
        assert_eq!(
            route_url(
                "https://user:pw@api.GitHub.com:443/repos?x=1#frag",
                workspaces
            ),
            Some(&home_id)
        );
        assert_eq!(
            route_url("https://www.example.org/", workspaces),
            Some(&home_id)
        );
        assert_eq!(route_url("https://notexample.org/", workspaces), None);
        assert_eq!(route_url("https://elsewhere.test/", workspaces), None);
        assert_eq!(route_url("not a url", workspaces), None);
        assert!(acme.owns_url("https://tracker.example.test"));
        assert!(!acme.owns_url("tracker.example.test"));
    }

    #[test]
    fn host_extraction_handles_the_usual_shapes() {
        assert_eq!(host_of("https://a.b.test/path"), Some("a.b.test"));
        assert_eq!(host_of("https://a.b.test"), Some("a.b.test"));
        assert_eq!(host_of("http://a.b.test:8080/"), Some("a.b.test"));
        assert_eq!(host_of("http://[::1]:8080/"), Some("[::1]"));
        assert_eq!(host_of("http://[2001:db8::1]:/x"), Some("[2001:db8::1]"));
        assert_eq!(host_of("http://[2001:db8::1]/x"), Some("[2001:db8::1]"));
        assert_eq!(host_of("http://a.b.test:/x"), Some("a.b.test"));
        assert_eq!(host_of("see ://tracker.example.test"), None);
        assert_eq!(
            host_of("https://evil.test\\@tracker.example.test/"),
            Some("evil.test")
        );
        assert_eq!(host_of("https://"), None);
        assert_eq!(host_of("a.b.test/path"), None);
        assert_eq!(host_of("mailto:dana@example.test"), None);
    }
}
