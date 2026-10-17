use crate::{error::Error, model::Repository};
use reqwest::{
    Client, Url,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT},
};
use std::time::Duration;

#[derive(Clone)]
pub struct GithubClient {
    client: Client,
    base_url: Url,
}

impl GithubClient {
    pub fn new(token: Option<&str>) -> Result<Self, Error> {
        Self::with_endpoint("https://api.github.com/", token, Duration::from_secs(30))
    }

    // Endpoint injection supports deterministic offline tests, not user configuration.
    pub fn with_endpoint(
        endpoint: &str,
        token: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let base_url = Url::parse(endpoint).map_err(|_| Error::Config("invalid GitHub API URL"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("opensource-maintainer-mcp/0.1.0"),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            HeaderValue::from_static("2022-11-28"),
        );
        if let Some(token) = token {
            if token.trim().is_empty() {
                return Err(Error::Config("GITHUB_TOKEN must not be empty"));
            }
            let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| Error::Config("GITHUB_TOKEN is not a valid HTTP header value"))?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        let client = Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::Network)?;
        Ok(Self { client, base_url })
    }

    pub async fn repositories(&self, owner: &str) -> Result<Vec<Repository>, Error> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| Error::Config("invalid GitHub API URL"))?
            .pop_if_empty()
            .extend(["users", owner, "repos"]);
        let mut repositories = Vec::new();
        for page in 1..=1000 {
            // This endpoint lists public repositories for either users or organizations.
            let response = self
                .client
                .get(url.clone())
                .query(&[
                    ("per_page", "100"),
                    ("page", &page.to_string()),
                    ("sort", "full_name"),
                    ("direction", "asc"),
                ])
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        Error::Timeout
                    } else {
                        Error::Network
                    }
                })?;
            let status = response.status();
            let number = |name: &str| {
                response
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
            };
            if status.as_u16() == 429
                || (status.as_u16() == 403
                    && (number("x-ratelimit-remaining") == Some(0)
                        || number("retry-after").is_some()))
            {
                return Err(Error::RateLimited {
                    retry_after: number("retry-after"),
                    reset: number("x-ratelimit-reset"),
                });
            }
            if status.as_u16() == 401 {
                return Err(Error::Authentication);
            }
            if !status.is_success() {
                return Err(Error::Http(status.as_u16()));
            }
            let batch: Vec<Repository> = response.json().await.map_err(|e| {
                if e.is_timeout() {
                    Error::Timeout
                } else {
                    Error::InvalidResponse
                }
            })?;
            let done = batch.len() < 100;
            repositories.extend(batch);
            if done {
                return Ok(repositories);
            }
        }
        Err(Error::PaginationLimit)
    }
}
