use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;

use crate::{
    discovery::Discovery,
    error::Error,
    model::{GitBlob, GitTree, ManifestStatus, ManifestSummary, ProjectOverview, Repository},
};

const MANIFEST_LIMIT: usize = 256 * 1024;

pub(crate) struct Inspection {
    pub overview: ProjectOverview,
    pub root_files: Vec<String>,
    pub dependencies: Vec<crate::compare::DependencyManifest>,
}

impl Discovery {
    pub async fn get_project_overview(&self, repository: &str) -> Result<ProjectOverview, Error> {
        Ok(self.inspect_project(repository).await?.overview)
    }

    pub(crate) async fn inspect_project(&self, repository: &str) -> Result<Inspection, Error> {
        validate_repository_name(repository)?;
        let config = &self.config.github;
        let repo = self
            .github
            .repositories(&config.owner)
            .await?
            .into_iter()
            .find(|r| {
                r.name.eq_ignore_ascii_case(repository)
                    && !r.private
                    && (config.include_archived || !r.archived)
                    && r.topics.contains(&config.topic)
            })
            .ok_or(Error::ProjectNotFound)?;
        self.inspect_repository(repo).await
    }

    // Callers must first select this metadata from scoped public discovery.
    pub(crate) async fn inspect_repository(&self, repo: Repository) -> Result<Inspection, Error> {
        let config = &self.config.github;
        let tree: GitTree = self
            .github
            .public_json(&[
                "repos",
                &config.owner,
                &repo.name,
                "git",
                "trees",
                &repo.default_branch,
            ])
            .await?;
        if tree.truncated {
            return Err(Error::TruncatedTree);
        }
        let mut root_files: Vec<_> = tree
            .tree
            .iter()
            .filter(|entry| {
                entry.kind == "blob" && matches!(entry.mode.as_str(), "100644" | "100755")
            })
            .map(|entry| entry.path.clone())
            .collect();
        root_files.sort();
        let mut dependencies = Vec::new();
        let mut manifests = Vec::new();
        let mut configuration_files = Vec::new();
        for entry in tree.tree {
            if matches!(entry.path.as_str(), "package.json" | "Cargo.toml") {
                let summary = if entry.kind != "blob"
                    || !matches!(entry.mode.as_str(), "100644" | "100755")
                {
                    empty_summary(&entry.path, ManifestStatus::UnsupportedFileType)
                } else if entry.size.is_none_or(|size| size > MANIFEST_LIMIT) {
                    empty_summary(&entry.path, ManifestStatus::TooLarge)
                } else {
                    let blob: GitBlob = self
                        .github
                        .public_json(&[
                            "repos",
                            &config.owner,
                            &repo.name,
                            "git",
                            "blobs",
                            &entry.sha,
                        ])
                        .await?;
                    if blob.sha != entry.sha || blob.encoding != "base64" {
                        return Err(Error::InvalidResponse);
                    }
                    if blob.size > MANIFEST_LIMIT {
                        empty_summary(&entry.path, ManifestStatus::TooLarge)
                    } else {
                        let encoded: String = blob
                            .content
                            .chars()
                            .filter(|c| !c.is_ascii_whitespace())
                            .collect();
                        let bytes = STANDARD
                            .decode(encoded)
                            .map_err(|_| Error::InvalidResponse)?;
                        if bytes.len() != blob.size {
                            return Err(Error::InvalidResponse);
                        }
                        dependencies.push(crate::compare::parse_dependencies(&entry.path, &bytes));
                        parse_manifest(&entry.path, &bytes)
                    }
                };
                manifests.push(summary);
            } else if entry.kind == "blob"
                && matches!(entry.mode.as_str(), "100644" | "100755")
                && is_configuration(&entry.path)
            {
                configuration_files.push(entry.path);
            }
        }
        manifests.sort_by(|a, b| a.path.cmp(&b.path));
        configuration_files.sort();
        Ok(Inspection {
            root_files,
            dependencies,
            overview: ProjectOverview {
                topics: repo.topics.clone(),
                project: repo.into(),
                tree_sha: tree.sha,
                manifests,
                configuration_files,
            },
        })
    }
}

fn empty_summary(path: &str, status: ManifestStatus) -> ManifestSummary {
    ManifestSummary {
        path: path.into(),
        status,
        name: None,
        version: None,
        package_manager: None,
        workspace: false,
        scripts: BTreeMap::new(),
    }
}

#[derive(Deserialize)]
struct NpmManifest {
    name: Option<String>,
    version: Option<String>,
    #[serde(rename = "packageManager")]
    package_manager: Option<String>,
    workspaces: Option<serde_json::Value>,
    #[serde(default)]
    scripts: BTreeMap<String, String>,
}

fn parse_manifest(path: &str, bytes: &[u8]) -> ManifestSummary {
    let mut result = empty_summary(path, ManifestStatus::Invalid);
    if path == "package.json" {
        if let Ok(npm) = serde_json::from_slice::<NpmManifest>(bytes) {
            result.name = npm.name;
            result.version = npm.version;
            result.package_manager = npm.package_manager;
            result.workspace = npm.workspaces.is_some();
            result.scripts = npm.scripts;
            result.status = ManifestStatus::Parsed;
        }
    } else if let Ok(text) = std::str::from_utf8(bytes)
        && let Ok(cargo) = toml::from_str::<toml::Table>(text)
    {
        let package = cargo.get("package").and_then(toml::Value::as_table);
        result.workspace = cargo.get("workspace").is_some_and(toml::Value::is_table);
        if package.is_some() || result.workspace {
            result.name = package
                .and_then(|p| p.get("name"))
                .and_then(toml::Value::as_str)
                .map(str::to_owned);
            // Inherited workspace versions deliberately remain null, not fabricated.
            result.version = package
                .and_then(|p| p.get("version"))
                .and_then(toml::Value::as_str)
                .map(str::to_owned);
            result.status = ManifestStatus::Parsed;
        }
    }
    result
}

fn is_configuration(path: &str) -> bool {
    matches!(
        path,
        ".editorconfig"
            | ".gitignore"
            | ".npmrc"
            | ".nvmrc"
            | ".node-version"
            | "rust-toolchain"
            | "rust-toolchain.toml"
            | "rustfmt.toml"
            | ".rustfmt.toml"
            | "clippy.toml"
            | ".clippy.toml"
            | "Cargo.lock"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "pnpm-workspace.yaml"
            | "lerna.json"
            | "turbo.json"
            | "nx.json"
            | "tsconfig.json"
            | "jsconfig.json"
            | ".prettierignore"
            | ".eslintignore"
            | ".eslintrc"
            | ".prettierrc"
            | "babel.config.json"
            | ".babelrc"
            | "Makefile"
            | "Dockerfile"
    ) || [
        "eslint.config.",
        ".eslintrc.",
        "prettier.config.",
        ".prettierrc.",
        "jest.config.",
        "vitest.config.",
        "vite.config.",
        "webpack.config.",
        "rollup.config.",
        "babel.config.",
    ]
    .iter()
    .any(|prefix| {
        path.strip_prefix(prefix).is_some_and(|ext| {
            matches!(
                ext,
                "js" | "cjs" | "mjs" | "ts" | "cts" | "mts" | "json" | "yaml" | "yml"
            )
        })
    })
}

pub(crate) fn validate_repository_name(repository: &str) -> Result<(), Error> {
    if repository.is_empty()
        || repository.len() > 100
        || repository == "."
        || repository == ".."
        || !repository
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(Error::InvalidRepositoryName);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_npm_scripts_as_data() {
        let parsed = parse_manifest("package.json", br#"{"name":"demo","version":"1.0.0","packageManager":"pnpm@10","workspaces":["packages/*"],"scripts":{"test":"echo test","prepare":"do-not-execute"}}"#);
        assert!(matches!(parsed.status, ManifestStatus::Parsed));
        assert_eq!(parsed.name.as_deref(), Some("demo"));
        assert_eq!(parsed.package_manager.as_deref(), Some("pnpm@10"));
        assert!(parsed.workspace);
        assert_eq!(parsed.scripts["prepare"], "do-not-execute");
    }
    #[test]
    fn handles_cargo_workspace_and_inherited_version() {
        for source in [
            "[workspace]\nmembers=['crates/*']",
            "[package]\nname='demo'\nversion.workspace=true",
        ] {
            let summary = parse_manifest("Cargo.toml", source.as_bytes());
            assert!(matches!(summary.status, ManifestStatus::Parsed));
            assert!(summary.version.is_none());
        }
        let summary = parse_manifest("Cargo.toml", b"[package]\nname='demo'\nversion='0.1.0'");
        assert_eq!(summary.version.as_deref(), Some("0.1.0"));
    }
    #[test]
    fn reports_invalid_manifests_without_echoing_source() {
        for (path, source) in [
            ("package.json", "secret"),
            ("package.json", "{\"scripts\":{\"test\":42}}"),
            ("Cargo.toml", "bad secret"),
            ("Cargo.toml", ""),
        ] {
            let summary = parse_manifest(path, source.as_bytes());
            assert!(matches!(summary.status, ManifestStatus::Invalid));
            assert!(
                !serde_json::to_string(&summary)
                    .expect("serialize")
                    .contains("secret")
            );
        }
    }
    #[test]
    fn inventory_is_limited_to_known_root_configs() {
        assert!(is_configuration("eslint.config.mjs"));
        assert!(is_configuration("rust-toolchain.toml"));
        assert!(!is_configuration(".env"));
        assert!(!is_configuration("nested/package.json"));
        assert!(!is_configuration("eslint.config.js.backup"));
    }
}
