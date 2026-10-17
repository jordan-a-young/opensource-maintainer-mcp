use crate::{error::Error, model::Repository};
use reqwest::{
    Client, Url,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT},
};
use std::time::Duration;

#[derive(Clone)]
pub struct GithubClient {
    client: Client,
    public_client: Client,
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
        let public_client = build_client(headers.clone(), timeout)?;
        if let Some(token) = token {
            if token.trim().is_empty() {
                return Err(Error::Config("GITHUB_TOKEN must not be empty"));
            }
            let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| Error::Config("GITHUB_TOKEN is not a valid HTTP header value"))?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        let client = build_client(headers, timeout)?;
        Ok(Self {
            client,
            public_client,
            base_url,
        })
    }

    pub(crate) async fn public_json<T: serde::de::DeserializeOwned>(
        &self,
        segments: &[&str],
    ) -> Result<T, Error> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| Error::InvalidResponse)?
            .pop_if_empty()
            .extend(segments.iter().copied());
        // Anonymous reads cannot expose private contents even if visibility changes
        // after discovery. Never follow blob URLs supplied by a response.
        let mut response = checked(self.public_client.get(url)).await?;
        const LIMIT: usize = 2 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|size| size > LIMIT as u64)
        {
            return Err(Error::ResponseTooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(network_error)? {
            if body.len() + chunk.len() > LIMIT {
                return Err(Error::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|_| Error::InvalidResponse)
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
            let response = checked(self.client.get(url.clone()).query(&[
                ("per_page", "100"),
                ("page", &page.to_string()),
                ("sort", "full_name"),
                ("direction", "asc"),
            ]))
            .await?;
            let body = response.bytes().await.map_err(network_error)?;
            let batch: Vec<Repository> =
                serde_json::from_slice(&body).map_err(|_| Error::InvalidResponse)?;
            let done = batch.len() < 100;
            repositories.extend(batch);
            if done {
                return Ok(repositories);
            }
        }
        Err(Error::PaginationLimit)
    }
}

fn build_client(headers: HeaderMap, timeout: Duration) -> Result<Client, Error> {
    Client::builder()
        .default_headers(headers)
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Error::Network)
}

fn network_error(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Network
    }
}

async fn checked(request: reqwest::RequestBuilder) -> Result<reqwest::Response, Error> {
    let response = request.send().await.map_err(network_error)?;
    let status = response.status().as_u16();
    let number = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
    };
    if status == 429
        || (status == 403
            && (number("x-ratelimit-remaining") == Some(0) || number("retry-after").is_some()))
    {
        return Err(Error::RateLimited {
            retry_after: number("retry-after"),
            reset: number("x-ratelimit-reset"),
        });
    }
    if status == 401 {
        return Err(Error::Authentication);
    }
    if !response.status().is_success() {
        return Err(Error::Http(status));
    }
    Ok(response)
}
