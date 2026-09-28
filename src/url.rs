use std::net::IpAddr;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

impl Url {
    pub fn parse(input: &str) -> Result<Self, Error> {
        let (scheme, rest) = if input
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        {
            (Scheme::Http, &input[7..])
        } else if input
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        {
            (Scheme::Https, &input[8..])
        } else {
            return Err(Error::InvalidInput(
                "expected an http:// or https:// URL".into(),
            ));
        };
        let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if authority.is_empty() || authority.contains('@') {
            return Err(Error::InvalidInput("invalid URL authority".into()));
        }
        let (host, port_text) = if let Some(bracketed) = authority.strip_prefix('[') {
            let end = bracketed
                .find(']')
                .ok_or_else(|| Error::InvalidInput("invalid IPv6 host".into()))?;
            let host = &bracketed[..end];
            if !matches!(host.parse::<IpAddr>(), Ok(IpAddr::V6(_))) {
                return Err(Error::InvalidInput("invalid IPv6 host".into()));
            }
            let suffix = &bracketed[end + 1..];
            let port = if suffix.is_empty() {
                None
            } else {
                Some(
                    suffix
                        .strip_prefix(':')
                        .ok_or_else(|| Error::InvalidInput("invalid URL port".into()))?,
                )
            };
            (host.to_string(), port)
        } else {
            let (host, port) = authority
                .rsplit_once(':')
                .map_or((authority, None), |(host, port)| (host, Some(port)));
            if host.is_empty()
                || host.len() > 253
                || !host
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
                || host.starts_with('-')
                || host.ends_with('-')
            {
                return Err(Error::InvalidInput("unsupported URL host".into()));
            }
            (host.to_ascii_lowercase(), port)
        };
        let port = port_text.map_or(Ok(scheme.default_port()), |text| {
            text.parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| Error::InvalidInput("invalid URL port".into()))
        })?;
        let suffix = &rest[authority_end..];
        let path_and_query = if suffix.is_empty() {
            "/".to_string()
        } else if suffix.starts_with('?') {
            format!("/{suffix}")
        } else {
            suffix.to_string()
        };
        if !path_and_query
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'#')
        {
            return Err(Error::InvalidInput(
                "URL paths must contain visible ASCII or percent escapes".into(),
            ));
        }
        Ok(Self {
            scheme,
            host,
            port,
            path_and_query,
        })
    }

    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == self.scheme.default_port() {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    pub fn as_string(&self) -> String {
        format!(
            "{}://{}{}",
            self.scheme.as_str(),
            self.authority(),
            self.path_and_query
        )
    }

    pub fn join(&self, reference: &str) -> Result<Self, Error> {
        if reference
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
            || reference
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        {
            return Self::parse(reference);
        }
        if reference.starts_with("//") {
            return Self::parse(&format!("{}:{reference}", self.scheme.as_str()));
        }
        if reference.contains(':') && !reference.starts_with(['/', '?', '#']) {
            let prefix = reference.split(['/', '?', '#']).next().unwrap_or("");
            if prefix.contains(':') {
                return Err(Error::InvalidInput("unsupported URL scheme".into()));
            }
        }
        let reference = reference
            .split_once('#')
            .map_or(reference, |(before, _)| before);
        if reference.is_empty() {
            return Ok(self.clone());
        }
        let path = if reference.starts_with('/') {
            reference.to_string()
        } else if reference.starts_with('?') {
            let base = self.path_and_query.split('?').next().unwrap_or("/");
            format!("{base}{reference}")
        } else {
            let base = self.path_and_query.split('?').next().unwrap_or("/");
            let directory = base.rsplit_once('/').map_or("/", |(prefix, _)| prefix);
            format!("{directory}/{reference}")
        };
        let (path_only, query) = path
            .split_once('?')
            .map_or((path.as_str(), ""), |(a, b)| (a, b));
        let mut segments = Vec::new();
        for segment in path_only.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    segments.pop();
                }
                _ => segments.push(segment),
            }
        }
        let mut normalized = format!("/{}", segments.join("/"));
        if path_only.ends_with('/') && !normalized.ends_with('/') {
            normalized.push('/');
        }
        if path.contains('?') {
            normalized.push('?');
            normalized.push_str(query);
        }
        Self::parse(&format!(
            "{}://{}{}",
            self.scheme.as_str(),
            self.authority(),
            normalized
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_resolves_references() {
        let base = Url::parse("https://example.com:8443/a/b/page.html?q=1").unwrap();
        assert_eq!(base.authority(), "example.com:8443");
        assert_eq!(
            base.join("../style.css").unwrap().path_and_query,
            "/a/style.css"
        );
        assert_eq!(
            base.join("?q=2").unwrap().path_and_query,
            "/a/b/page.html?q=2"
        );
        assert_eq!(base.join("/x").unwrap().path_and_query, "/x");
    }

    #[test]
    fn rejects_credentials_and_control_characters() {
        assert!(Url::parse("https://user:pass@example.com/").is_err());
        assert!(Url::parse("https://example.com/\r\nHeader: bad").is_err());
        assert_eq!(
            Url::parse("HTTPS://Example.COM/").unwrap().host,
            "example.com"
        );
    }
}
