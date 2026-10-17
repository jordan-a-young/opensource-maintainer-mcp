#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(&'static str),
    #[error("GitHub authentication failed; check GITHUB_TOKEN")]
    Authentication,
    #[error(
        "GitHub rate limit reached; retry later (retry-after seconds: {retry_after:?}, reset Unix timestamp: {reset:?})"
    )]
    RateLimited {
        retry_after: Option<u64>,
        reset: Option<u64>,
    },
    #[error("GitHub returned HTTP {0}")]
    Http(u16),
    #[error("GitHub request timed out")]
    Timeout,
    #[error("could not connect to GitHub; check network connectivity")]
    Network,
    #[error("GitHub returned an invalid repository response")]
    InvalidResponse,
    #[error("GitHub pagination exceeded the safety limit; results were not returned")]
    PaginationLimit,
}
