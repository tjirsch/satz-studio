//! The latest release of a GitHub repository, read from GitHub's REST API without a token.
//!
//! Two callers: the look for a newer satz-studio ([`look_for_studio_update`]) — a notice,
//! never an update: nothing is downloaded, run or written — and satz's installer
//! (`satz::install`), which takes the installer and its checksum from ONE release object so
//! a release published between two downloads cannot pair one release's script with
//! another's checksum.
//!
//! The API is always asked for `releases/latest`, never for a tag. Unauthenticated, it
//! allows sixty requests an hour per address, shared with everything else on that address
//! (`satz self-update` included); a refusal for that reason is [`GithubError::RateLimited`]
//! and says so, rather than a status number.

use std::time::Duration;

/// GitHub's REST API.
pub const API: &str = "https://api.github.com";

/// This app's repository, whose latest release the update look reads.
pub const STUDIO_REPO: &str = "tjirsch/satz-studio";

/// A request, connection to the last body byte.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// A release as the API returns it; only what is read here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Release {
    pub tag_name: String,
    /// the release's page on github.com
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

impl Release {
    /// The tag as a version: `v0.3.0` and `0.3.0` are both `0.3.0`.
    pub fn version(&self) -> Result<semver::Version, GithubError> {
        semver::Version::parse(self.tag_name.trim_start_matches('v'))
            .map_err(|_| GithubError::Tag(self.tag_name.clone()))
    }

    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    #[error("{url} could not be reached — offline, or a proxy or firewall is in the way: {detail}")]
    Unreachable { url: String, detail: String },
    #[error(
        "GitHub refused the request: the unauthenticated API allows 60 requests an hour from this address and they are used up{}",
        match resets_in { Some(d) => format!("; the limit resets in {} min", d.as_secs().div_ceil(60)), None => String::new() }
    )]
    RateLimited { resets_in: Option<Duration> },
    #[error("{repo} has no published release")]
    NoRelease { repo: String },
    #[error("{url} answered {status}: {body}")]
    Status {
        url: String,
        status: u16,
        body: String,
    },
    #[error("{url}: the request failed: {detail}")]
    Request { url: String, detail: String },
    #[error("{url} answered with JSON that is not a release: {detail}")]
    Json { url: String, detail: String },
    #[error("the latest release is tagged `{0}`, which is not a version")]
    Tag(String),
}

/// The one HTTP client of these calls: GitHub's API refuses a request without a
/// `User-Agent`, so it names the app and its version.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("satz-studio/", env!("CARGO_PKG_VERSION")))
        .timeout(TIMEOUT)
        .build()
        .expect("the HTTP client builds: no proxy or TLS setting of this app can fail")
}

/// `GET {api}/repos/{repo}/releases/latest`.
pub async fn latest_release(
    client: &reqwest::Client,
    api: &str,
    repo: &str,
) -> Result<Release, GithubError> {
    let url = format!("{}/repos/{repo}/releases/latest", api.trim_end_matches('/'));
    let response = send(
        client
            .get(&url)
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28"),
        &url,
        Quota::Api,
    )
    .await
    .map_err(|e| match e {
        GithubError::Status { status: 404, .. } => GithubError::NoRelease {
            repo: repo.to_string(),
        },
        e => e,
    })?;
    let bytes = body(response, &url).await?;
    serde_json::from_slice(&bytes).map_err(|e| GithubError::Json {
        url,
        detail: e.to_string(),
    })
}

/// The bytes at `url`, redirects followed — a release asset's `browser_download_url`
/// redirects to GitHub's object storage.
pub async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, GithubError> {
    let response = send(client.get(url), url, Quota::None).await?;
    body(response, url).await
}

/// What the look for a newer satz-studio found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StudioUpdate {
    /// a release newer than the running app: its version and its page on github.com
    Available {
        version: semver::Version,
        page: String,
    },
    /// the latest release is not newer than the running app
    Latest { latest: semver::Version },
}

/// Read the latest release of [`STUDIO_REPO`] and compare it with `running`. A look:
/// nothing is downloaded, nothing runs, nothing is written.
pub async fn look_for_studio_update(
    client: &reqwest::Client,
    api: &str,
    running: &semver::Version,
) -> Result<StudioUpdate, GithubError> {
    let release = latest_release(client, api, STUDIO_REPO).await?;
    let latest = release.version()?;
    Ok(if latest > *running {
        StudioUpdate::Available {
            version: latest,
            page: release.html_url,
        }
    } else {
        StudioUpdate::Latest { latest }
    })
}

/// Whether a request counts against the API's hourly quota. A release asset is served from
/// github.com and its object storage, so a 403 there is a refusal of that file and not the
/// API's limit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Quota {
    Api,
    None,
}

async fn send(
    request: reqwest::RequestBuilder,
    url: &str,
    quota: Quota,
) -> Result<reqwest::Response, GithubError> {
    let response = request.send().await.map_err(|e| {
        if e.is_connect() || e.is_timeout() {
            GithubError::Unreachable {
                url: url.to_string(),
                detail: e.to_string(),
            }
        } else {
            GithubError::Request {
                url: url.to_string(),
                detail: e.to_string(),
            }
        }
    })?;
    let status = response.status();
    if quota == Quota::Api
        && (status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
    {
        return Err(GithubError::RateLimited {
            resets_in: resets_in(response.headers()),
        });
    }
    if status.is_success() {
        return Ok(response);
    }
    let text = response.text().await.unwrap_or_default();
    Err(GithubError::Status {
        url: url.to_string(),
        status: status.as_u16(),
        body: text.chars().take(300).collect(),
    })
}

async fn body(response: reqwest::Response, url: &str) -> Result<Vec<u8>, GithubError> {
    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| GithubError::Request {
            url: url.to_string(),
            detail: e.to_string(),
        })
}

/// `x-ratelimit-reset` is the instant the quota refills, in Unix seconds. A 403 or 429 is
/// read as the rate limit whether or not the header came with it: without a token, that is
/// what GitHub refuses a read of a public release for.
fn resets_in(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let reset: u64 = headers
        .get("x-ratelimit-reset")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(Duration::from_secs(reset.saturating_sub(now)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.to_string(),
            html_url: format!("https://github.com/{STUDIO_REPO}/releases/tag/{tag}"),
            assets: Vec::new(),
        }
    }

    #[test]
    fn a_tag_with_or_without_its_v_is_the_version() {
        assert_eq!(
            release("v0.3.0").version().unwrap(),
            semver::Version::new(0, 3, 0)
        );
        assert_eq!(
            release("0.3.0").version().unwrap(),
            semver::Version::new(0, 3, 0)
        );
        assert!(matches!(
            release("nightly").version(),
            Err(GithubError::Tag(t)) if t == "nightly"
        ));
    }

    #[test]
    fn a_rate_limit_says_it_is_one_and_when_it_resets() {
        let said = GithubError::RateLimited {
            resets_in: Some(Duration::from_secs(61)),
        }
        .to_string();
        assert!(said.contains("60 requests an hour"), "{said}");
        assert!(said.contains("resets in 2 min"), "{said}");
        let said = GithubError::RateLimited { resets_in: None }.to_string();
        assert!(!said.contains("resets"), "{said}");
    }
}
