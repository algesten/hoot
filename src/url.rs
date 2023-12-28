//!

///
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UrlError {
    TooShort,
    MissingScheme,
    PortNotANumber,
    FragmentBeforeQuery,
    TooShortUserPass,
    BadPassword,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url<'a> {
    buffer: &'a str,

    // Components
    scheme_end: u16,           // Before ':'
    username_end: Option<u16>, // Before ':' (if a password is given) or '@' (if not)
    host_start: u16,
    host_end: u16,
    port: Option<u16>,
    path_start: u16,             // Before initial '/', if any
    query_start: Option<u16>,    // Before '?', unlike Position::QueryStart
    fragment_start: Option<u16>, // Before '#', unlike Position::FragmentStart
}

impl<'a> Url<'a> {
    pub fn parse_str(s: &'a str) -> Result<Self, UrlError> {
        // x://a
        if s.len() < 5 {
            return Err(UrlError::TooShort);
        }

        let scheme_end = s.find("://").ok_or(UrlError::MissingScheme)?;
        let scheme_end_and_delimiter = scheme_end + 3;
        // All indexes will be relative to _after_ :// and we adjust at the end.
        let x = &s[scheme_end_and_delimiter..];

        let (query_start, fragment_start) = (x.find("?"), x.find("#"));

        let query_or_fragment = match (query_start, fragment_start) {
            (None, None) => None,
            (None, Some(m)) => Some(m),
            (Some(n), None) => Some(n),
            (Some(n), Some(m)) => {
                if m < n {
                    return Err(UrlError::FragmentBeforeQuery);
                }
                Some(n)
            }
        };

        // Either where the path starts, or the end. All the following have the same path_start:
        // https://foo.com
        // https://foo.com?a=b
        // https://foo.com#a=b
        // https://foo.com/
        // https://foo.com/path
        let path_start = x.find("/").or(query_or_fragment).unwrap_or(x.len());

        // Limit buffer to be between '://' and the start of the path '/'
        let x = &x[..path_start];

        fn split_username(x: &str) -> Result<usize, UrlError> {
            // Need at least one char: http://a@foo.bar
            if x.is_empty() {
                return Err(UrlError::TooShortUserPass);
            }

            let n = if let Some((username, password)) = x.split_once(":") {
                if username.is_empty() || password.is_empty() {
                    return Err(UrlError::TooShortUserPass);
                }
                if let Some(_) = password.find(":") {
                    return Err(UrlError::BadPassword);
                }
                username.len()
            } else {
                x.len()
            };

            Ok(n)
        }

        let maybe_upass = x.find("@");
        let username_end = maybe_upass.map(|n| split_username(&x[..n])).transpose()?;

        let host_start = maybe_upass.map(|n| n + 1).unwrap_or(0);

        let port_start = x[host_start..path_start].find(":").map(|n| n + host_start);

        let host_end = port_start.unwrap_or(path_start);

        let mut port = None;

        if let Some(port_start) = port_start {
            let port_str = &x[(port_start + 1)..path_start];
            let p: u16 = port_str.parse().map_err(|_| UrlError::PortNotANumber)?;
            port = Some(p);
        }

        Ok(Url {
            buffer: s,
            scheme_end: scheme_end as u16,
            username_end: username_end.map(|n| (n + scheme_end_and_delimiter) as u16),
            host_start: (host_start + scheme_end_and_delimiter) as u16,
            host_end: (host_end + scheme_end_and_delimiter) as u16,
            port,
            path_start: (path_start + scheme_end_and_delimiter) as u16,
            query_start: query_start.map(|n| (n + scheme_end_and_delimiter) as u16),
            fragment_start: fragment_start.map(|n| (n + scheme_end_and_delimiter) as u16),
        })
    }

    pub fn scheme(&self) -> &str {
        &self.buffer[..self.scheme_end as usize]
    }

    pub fn username(&self) -> &str {
        self.username_end
            .map(|u| &self.buffer[(self.scheme_end as usize + 3)..u as usize])
            .unwrap_or(&"")
    }

    pub fn password(&self) -> &str {
        self.username_end
            .map(|u| &self.buffer[(u as usize + 1)..self.host_start as usize - 1])
            .unwrap_or(&"")
    }

    pub fn host(&self) -> &str {
        &self.buffer[self.host_start as usize..self.path_start as usize]
    }

    pub fn hostname(&self) -> &str {
        &self.buffer[self.host_start as usize..self.host_end as usize]
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn pathname(&self) -> &str {
        let end = self
            .query_start
            .or(self.fragment_start)
            .unwrap_or(self.path_start) as usize;

        &self.buffer[self.path_start as usize..end]
    }

    pub fn query(&self) -> Option<&str> {
        let start = self.query_start? as usize;
        let end = self
            .fragment_start
            .map(|n| n as usize)
            .unwrap_or(self.buffer.len());
        Some(&self.buffer[start..end])
    }

    pub fn fragment(&self) -> Option<&str> {
        self.fragment_start.map(|s| &self.buffer[s as usize..])
    }
}

impl<'a> TryFrom<&'a str> for Url<'a> {
    type Error = UrlError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::parse_str(value)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_url() {
        let s = "https://martin:secret@host.test:1234/abc?foo=bar#baz";
        let u = Url::parse_str(s).unwrap();
        println!("{:?}", u);
        println!("{}", u.scheme());
        println!("{}", u.username());
        println!("{}", u.password());
        println!("{}", u.host());
        println!("{}", u.hostname());
        println!("{:?}", u.port());
        println!("{}", u.pathname());
        println!("{:?}", u.query());
        println!("{:?}", u.fragment());
    }
}
