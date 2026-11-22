//! HTTP helpers shared by every catalog: one configured agent, JSON GET/POST,
//! and transparent handling of servers that gzip a body without saying so.

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use serde::de::DeserializeOwned;
use std::io::Read;
use std::time::Duration;

const MB: u64 = 1024 * 1024;
pub const USER_AGENT: &str = concat!("Decanter/", env!("CARGO_PKG_VERSION"));

/// How long to wait for a server to start answering once connected. Only the
/// wait for the reply to begin, so big downloads aren't cut short.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

pub fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(REPLY_TIMEOUT))
        // Total budget for reading one body. Generous for slow connections.
        .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
        .user_agent(USER_AGENT)
        .build();
    ureq::Agent::new_with_config(config)
}

/// GET a small body (API replies, manifests). Bodies that start with the gzip
/// magic bytes are decompressed: Kuro's CDN serves gzip without a
/// `Content-Encoding` header.
pub fn get_bytes(agent: &ureq::Agent, url: &str, limit_mb: u64) -> Result<Vec<u8>> {
    let body = agent
        .get(url)
        .call()
        .with_context(|| format!("GET {url}"))?
        .body_mut()
        .with_config()
        .limit(limit_mb * MB)
        .read_to_vec()
        .with_context(|| format!("reading {url}"))?;
    gunzip_if_needed(body, limit_mb)
}

pub fn get_json<T: DeserializeOwned>(agent: &ureq::Agent, url: &str) -> Result<T> {
    let bytes = get_bytes(agent, url, 64)?;
    serde_json::from_slice(&bytes).with_context(|| format!("unexpected reply shape from {url}"))
}

pub fn post_json<T: DeserializeOwned>(
    agent: &ureq::Agent,
    url: &str,
    body: &serde_json::Value,
) -> Result<T> {
    let bytes = agent
        .post(url)
        .header("content-type", "application/json")
        .send(body.to_string())
        .with_context(|| format!("POST {url}"))?
        .body_mut()
        .with_config()
        .limit(16 * MB)
        .read_to_vec()?;
    let bytes = gunzip_if_needed(bytes, 16)?;
    serde_json::from_slice(&bytes).with_context(|| format!("unexpected reply shape from {url}"))
}

fn gunzip_if_needed(body: Vec<u8>, limit_mb: u64) -> Result<Vec<u8>> {
    if !body.starts_with(&[0x1f, 0x8b]) {
        return Ok(body);
    }
    let mut out = Vec::new();
    GzDecoder::new(body.as_slice())
        .take(limit_mb * MB + 1)
        .read_to_end(&mut out)
        .context("decompressing gzip body")?;
    if out.len() as u64 > limit_mb * MB {
        bail!("decompressed body is over {limit_mb} MB");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    #[test]
    fn plain_body_is_untouched() {
        assert_eq!(gunzip_if_needed(b"{\"a\":1}".to_vec(), 1).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn gzip_body_is_decompressed() {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(b"{\"a\":1}").unwrap();
        let gz = enc.finish().unwrap();
        assert_eq!(gunzip_if_needed(gz, 1).unwrap(), b"{\"a\":1}");
    }
}
