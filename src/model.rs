use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct Project {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub url: String,
    pub language: Option<String>,
    pub archived: bool,
    pub default_branch: String,
}

#[derive(Debug, Deserialize)]
pub struct Repository {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub language: Option<String>,
    pub archived: bool,
    pub default_branch: String,
    pub topics: Vec<String>,
    pub private: bool,
}

impl From<Repository> for Project {
    fn from(repo: Repository) -> Self {
        Self {
            name: repo.name,
            full_name: repo.full_name,
            description: repo.description,
            url: repo.html_url,
            language: repo.language,
            archived: repo.archived,
            default_branch: repo.default_branch,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Projects {
    pub projects: Vec<Project>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_nullable_metadata_and_url() {
        let repo: Repository = serde_json::from_value(serde_json::json!({
            "name":"demo", "full_name":"owner/demo", "description":null,
            "html_url":"https://github.com/owner/demo", "language":null,
            "archived":false, "default_branch":"main", "topics":[], "private":false
        }))
        .expect("fixture");
        let project = Project::from(repo);
        assert_eq!(project.name, "demo");
        assert_eq!(project.full_name, "owner/demo");
        assert_eq!(project.url, "https://github.com/owner/demo");
        assert_eq!(project.default_branch, "main");
        assert_eq!(project.description, None);
        assert_eq!(project.language, None);
        assert!(!project.archived);
    }
}

/// Root-level observations; no configuration or script is executed.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectOverview {
    pub project: Project,
    pub topics: Vec<String>,
    pub tree_sha: String,
    pub manifests: Vec<ManifestSummary>,
    pub configuration_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManifestStatus {
    Parsed,
    Invalid,
    TooLarge,
    UnsupportedFileType,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ManifestSummary {
    pub path: String,
    pub status: ManifestStatus,
    pub name: Option<String>,
    pub version: Option<String>,
    pub package_manager: Option<String>,
    pub workspace: bool,
    pub scripts: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitTree {
    pub sha: String,
    pub truncated: bool,
    pub tree: Vec<GitEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitEntry {
    pub path: String,
    pub mode: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha: String,
    pub size: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitBlob {
    pub sha: String,
    pub content: String,
    pub encoding: String,
    pub size: usize,
}

/// Heuristic observations of one root tree snapshot, not a quality certification.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryAudit {
    pub project: Project,
    pub tree_sha: String,
    pub scope: String,
    pub checks: Vec<AuditCheck>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Finding,
    NotAssessed,
    NotApplicable,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuditCheck {
    pub id: String,
    pub status: CheckStatus,
    pub summary: String,
    /// Observed paths or manifest field locations; never fabricated for missing files.
    pub evidence: Vec<String>,
    pub recommendation: Option<String>,
}
