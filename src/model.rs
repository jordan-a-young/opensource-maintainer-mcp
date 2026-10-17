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
