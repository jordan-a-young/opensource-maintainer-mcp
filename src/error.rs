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
    #[error("repository must be a simple GitHub repository name, not an owner/name or URL")]
    InvalidRepositoryName,
    #[error("comparison requires 2–5 distinct repository names (case-insensitive)")]
    InvalidComparison,
    #[error("comparison exceeds 500 rows; choose repositories with smaller root manifests")]
    ComparisonTooLarge,
    #[error(
        "CI analysis exceeds the limit of 20 workflows, 50 jobs per workflow, 200 steps per job, or 1000 checks total"
    )]
    CiAnalysisLimit,
    #[error(
        "repository is not a public project matching the configured owner, topic, and archive policy"
    )]
    ProjectNotFound,
    #[error("GitHub returned a truncated tree; overview would be incomplete")]
    TruncatedTree,
    #[error("GitHub response exceeded the 2 MiB overview response limit")]
    ResponseTooLarge,
}
