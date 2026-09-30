use std::collections::HashSet;
use std::io::{self, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::Error;
use crate::url::{Scheme, Url};

const MAX_HEADER: usize = 64 * 1024;
const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

enum Connection {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for Connection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Connection {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub final_url: Url,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn text(&self) -> Result<String, Error> {
        let content_type = self.header("content-type").unwrap_or("");
        if let Some(charset) = content_type.split(';').find_map(|part| {
            part.trim()
                .to_ascii_lowercase()
                .strip_prefix("charset=")
                .map(str::to_string)
        }) && charset.trim_matches('"') != "utf-8"
        {
            return Err(Error::Network(format!(
                "unsupported character encoding: {charset}"
            )));
        }
        String::from_utf8(self.body.clone())
            .map_err(|_| Error::Network("response is not valid UTF-8".into()))
    }
}

pub struct Client {
    tls: Arc<ClientConfig>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Self { tls: Arc::new(tls) }
    }

    fn connect(&self, url: &Url) -> Result<Connection, Error> {
        let addresses = (url.host.as_str(), url.port).to_socket_addrs()?;
        let mut last_error = None;
        for address in addresses.take(4) {
            match TcpStream::connect_timeout(&address, Duration::from_secs(8)) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(Duration::from_secs(8)))?;
                    stream.set_write_timeout(Some(Duration::from_secs(8)))?;
                    if url.scheme == Scheme::Http {
                        return Ok(Connection::Plain(stream));
                    }
                    let server_name = ServerName::try_from(url.host.clone())
                        .map_err(|_| Error::Network("invalid TLS server name".into()))?;
                    let session = ClientConnection::new(self.tls.clone(), server_name)
                        .map_err(|error| Error::Network(format!("TLS setup failed: {error}")))?;
                    return Ok(Connection::Tls(Box::new(StreamOwned::new(session, stream))));
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(Error::Network(format!(
            "connection failed: {}",
            last_error.map_or_else(|| "no resolved addresses".into(), |error| error.to_string())
        )))
    }

    fn fetch_once(&self, url: &Url, max_body: usize) -> Result<Response, Error> {
        let mut connection = self.connect(url)?;
        write!(
            connection,
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Scarlite/0.1 Phos/0.1\r\nAccept: text/html, text/css;q=0.9, */*;q=0.1\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
            url.path_and_query,
            url.authority()
        )?;
        connection.flush()?;
        read_response_limited(connection, url.clone(), max_body)
    }

    pub fn fetch(&self, url: &Url) -> Result<Response, Error> {
        self.fetch_limited(url, MAX_BODY)
    }

    pub fn fetch_limited(&self, url: &Url, max_body: usize) -> Result<Response, Error> {
        let max_body = max_body.min(MAX_BODY);
        let mut current = url.clone();
        let mut visited = HashSet::new();
        for _ in 0..=MAX_REDIRECTS {
            if !visited.insert(current.clone()) {
                return Err(Error::Network("redirect loop detected".into()));
            }
            let response = self.fetch_once(&current, max_body)?;
            if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
                let location = response
                    .header("location")
                    .ok_or_else(|| Error::Network("redirect missing Location header".into()))?;
                let next = current.join(location)?;
                if current.scheme == Scheme::Https && next.scheme == Scheme::Http {
                    return Err(Error::Network("HTTPS redirect to HTTP refused".into()));
                }
                current = next;
                continue;
            }
            if !(200..300).contains(&response.status) {
                return Err(Error::Network(format!("HTTP status {}", response.status)));
            }
            return Ok(response);
        }
        Err(Error::Network("too many redirects".into()))
    }
}

fn read_header<R: Read>(reader: &mut BufReader<R>) -> Result<Vec<u8>, Error> {
    let mut header = Vec::new();
    let mut byte = [0];
    while header.len() < MAX_HEADER {
        reader.read_exact(&mut byte)?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            return Ok(header);
        }
    }
    Err(Error::Network("HTTP headers exceed 64 KiB".into()))
}

fn parse_header(header: &[u8]) -> Result<(u16, Vec<(String, String)>), Error> {
    let text = std::str::from_utf8(header)
        .map_err(|_| Error::Network("HTTP headers are not ASCII".into()))?;
    if !text.is_ascii() {
        return Err(Error::Network("HTTP headers are not ASCII".into()));
    }
    let mut lines = text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| Error::Network("missing HTTP status".into()))?;
    let mut parts = status_line.split_ascii_whitespace();
    let version = parts.next().unwrap_or("");
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(Error::Network("unsupported HTTP version".into()));
    }
    let status: u16 = parts
        .next()
        .ok_or_else(|| Error::Network("missing HTTP status code".into()))?
        .parse()
        .map_err(|_| Error::Network("invalid HTTP status code".into()))?;
    if !(100..600).contains(&status) {
        return Err(Error::Network("invalid HTTP status code".into()));
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| Error::Network("malformed HTTP header".into()))?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(Error::Network("invalid HTTP header name".into()));
        }
        let value = value.trim();
        if value
            .bytes()
            .any(|byte| byte.is_ascii_control() && byte != b'\t')
        {
            return Err(Error::Network("invalid HTTP header value".into()));
        }
        headers.push((name.to_ascii_lowercase(), value.to_string()));
    }
    Ok((status, headers))
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn read_line<R: Read>(reader: &mut BufReader<R>) -> Result<String, Error> {
    let mut line = Vec::new();
    let mut byte = [0];
    while line.len() < 8192 {
        reader.read_exact(&mut byte)?;
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            line.truncate(line.len() - 2);
            return String::from_utf8(line)
                .map_err(|_| Error::Network("invalid chunk header".into()));
        }
    }
    Err(Error::Network("HTTP line exceeds 8 KiB".into()))
}

fn read_chunked<R: Read>(reader: &mut BufReader<R>, max_body: usize) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    loop {
        let line = read_line(reader)?;
        let size = line.split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size.trim(), 16)
            .map_err(|_| Error::Network("invalid chunk size".into()))?;
        if size == 0 {
            let mut trailer_bytes = 0;
            loop {
                let trailer = read_line(reader)?;
                trailer_bytes += trailer.len() + 2;
                if trailer_bytes > MAX_HEADER {
                    return Err(Error::Network("HTTP trailers exceed 64 KiB".into()));
                }
                if trailer.is_empty() {
                    return Ok(body);
                }
            }
        }
        if size > max_body - body.len() {
            return Err(Error::Network("HTTP body exceeds resource limit".into()));
        }
        let offset = body.len();
        body.resize(offset + size, 0);
        reader.read_exact(&mut body[offset..])?;
        let mut terminator = [0; 2];
        reader.read_exact(&mut terminator)?;
        if terminator != *b"\r\n" {
            return Err(Error::Network("invalid chunk terminator".into()));
        }
    }
}

#[cfg(test)]
fn read_response<R: Read>(source: R, final_url: Url) -> Result<Response, Error> {
    read_response_limited(source, final_url, MAX_BODY)
}

fn read_response_limited<R: Read>(
    source: R,
    final_url: Url,
    max_body: usize,
) -> Result<Response, Error> {
    let mut reader = BufReader::new(source);
    let mut interim_responses = 0;
    let (status, headers) = loop {
        let header = read_header(&mut reader)?;
        let parsed = parse_header(&header)?;
        if !(100..200).contains(&parsed.0) {
            break parsed;
        }
        if parsed.0 == 101 {
            return Err(Error::Network(
                "HTTP protocol upgrade is unsupported".into(),
            ));
        }
        interim_responses += 1;
        if interim_responses > 4 {
            return Err(Error::Network("too many interim HTTP responses".into()));
        }
    };
    let encoding = header_value(&headers, "content-encoding").unwrap_or("identity");
    if !encoding.eq_ignore_ascii_case("identity") {
        return Err(Error::Network(format!(
            "unsupported Content-Encoding: {encoding}"
        )));
    }
    let transfer = header_value(&headers, "transfer-encoding");
    let lengths: Vec<_> = headers
        .iter()
        .filter(|(name, _)| name == "content-length")
        .map(|(_, value)| value.as_str())
        .collect();
    if lengths.windows(2).any(|pair| pair[0] != pair[1]) {
        return Err(Error::Network("conflicting Content-Length headers".into()));
    }
    if transfer.is_some() && !lengths.is_empty() {
        return Err(Error::Network("conflicting HTTP body framing".into()));
    }
    let body = if matches!(status, 204 | 304) {
        Vec::new()
    } else if let Some(transfer) = transfer {
        if !transfer.eq_ignore_ascii_case("chunked") {
            return Err(Error::Network("unsupported Transfer-Encoding".into()));
        }
        read_chunked(&mut reader, max_body)?
    } else if let Some(length) = lengths.first() {
        let length: usize = length
            .parse()
            .map_err(|_| Error::Network("invalid Content-Length".into()))?;
        if length > max_body {
            return Err(Error::Network("HTTP body exceeds resource limit".into()));
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        body
    } else {
        let mut body = Vec::new();
        reader.take(max_body as u64 + 1).read_to_end(&mut body)?;
        if body.len() > max_body {
            return Err(Error::Network("HTTP body exceeds resource limit".into()));
        }
        body
    };
    Ok(Response {
        final_url,
        status,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chunked_response() {
        let bytes = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        let response = read_response(&bytes[..], Url::parse("http://localhost/").unwrap()).unwrap();
        assert_eq!(response.body, b"hello world");
    }

    #[test]
    fn rejects_conflicting_body_framing() {
        let bytes = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n";
        assert!(read_response(&bytes[..], Url::parse("http://localhost/").unwrap()).is_err());
    }

    #[test]
    fn resource_limit_rejects_large_content_length_before_body_read() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5000000\r\n\r\n";
        let error = read_response_limited(
            &response[..],
            Url::parse("http://localhost/").unwrap(),
            4 * 1024 * 1024,
        )
        .unwrap_err();
        assert!(error.to_string().contains("resource limit"));
    }

    #[test]
    fn accepts_early_hints_before_final_response() {
        let bytes = b"HTTP/1.1 103 Early Hints\r\nLink: </site.css>; rel=preload\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
        let response = read_response(&bytes[..], Url::parse("http://localhost/").unwrap()).unwrap();
        assert_eq!(response.body, b"ok");
    }
}
