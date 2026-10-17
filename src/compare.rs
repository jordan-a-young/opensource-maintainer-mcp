use crate::{
    discovery::Discovery,
    error::Error,
    model::{
        ComparisonObservation, ComparisonProject, ComparisonRow, ComparisonStatus, ManifestStatus,
        ObservationStatus, StandardsComparison,
    },
    overview::{Inspection, validate_repository_name},
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct DependencyManifest {
    pub path: String,
    pub complete: bool,
    // Section/name keys keep dependency kinds distinct. None means unresolved declaration.
    pub declarations: BTreeMap<String, Option<String>>,
}

pub(crate) fn parse_dependencies(path: &str, bytes: &[u8]) -> DependencyManifest {
    let mut result = DependencyManifest {
        path: path.into(),
        complete: true,
        declarations: BTreeMap::new(),
    };
    let value = if path == "package.json" {
        serde_json::from_slice::<serde_json::Value>(bytes).ok()
    } else {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| toml::from_str::<toml::Table>(text).ok())
            .and_then(|table| serde_json::to_value(table).ok())
    };
    let Some(value) = value.filter(serde_json::Value::is_object) else {
        result.complete = false;
        return result;
    };
    let sections: &[&str] = if path == "package.json" {
        &[
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ]
    } else {
        &[
            "dependencies",
            "dev-dependencies",
            "build-dependencies",
            "workspace/dependencies",
        ]
    };
    for section in sections {
        let location = if *section == "workspace/dependencies" {
            value.get("workspace").and_then(|v| v.get("dependencies"))
        } else {
            value.get(*section)
        };
        let Some(location) = location else {
            continue;
        };
        let Some(entries) = location.as_object() else {
            result.complete = false;
            continue;
        };
        for (name, declaration) in entries {
            let version = declaration.as_str().or_else(|| {
                if path == "Cargo.toml" {
                    declaration
                        .get("version")
                        .and_then(serde_json::Value::as_str)
                } else {
                    None
                }
            });
            result.declarations.insert(
                format!("{section}/{}", pointer(name)),
                version.map(str::to_owned),
            );
        }
    }
    result
}

fn pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

impl Discovery {
    pub async fn compare_standards(
        &self,
        repositories: &[String],
    ) -> Result<StandardsComparison, Error> {
        if !(2..=5).contains(&repositories.len()) {
            return Err(Error::InvalidComparison);
        }
        let mut names = BTreeSet::new();
        for name in repositories {
            validate_repository_name(name)?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(Error::InvalidComparison);
            }
        }
        let config = &self.config.github;
        let selected: BTreeMap<_, _> = self
            .github
            .repositories(&config.owner)
            .await?
            .into_iter()
            .filter(|r| {
                !r.private
                    && (config.include_archived || !r.archived)
                    && r.topics.contains(&config.topic)
                    && names.contains(&r.name.to_ascii_lowercase())
            })
            .map(|r| (r.name.to_ascii_lowercase(), r))
            .collect();
        // Validate the whole selection before any content request.
        if selected.len() != names.len() {
            return Err(Error::ProjectNotFound);
        }
        let mut inspections = Vec::new();
        for (_, repo) in selected {
            inspections.push(self.inspect_repository(repo).await?);
        }
        compare(inspections)
    }
}

fn observation(
    inspection: &Inspection,
    status: ObservationStatus,
    values: Vec<String>,
    evidence: Vec<String>,
) -> ComparisonObservation {
    ComparisonObservation {
        repository: inspection.overview.project.full_name.clone(),
        status,
        values,
        evidence,
    }
}

fn manifest_observation(inspection: &Inspection, path: &str, field: &str) -> ComparisonObservation {
    let manifest = inspection
        .overview
        .manifests
        .iter()
        .find(|m| m.path == path);
    let Some(manifest) = manifest else {
        return observation(inspection, ObservationStatus::NotApplicable, vec![], vec![]);
    };
    if !matches!(manifest.status, ManifestStatus::Parsed) {
        return observation(
            inspection,
            ObservationStatus::NotAssessed,
            vec![],
            vec![path.into()],
        );
    }
    let value = if field == "workspace" {
        Some(manifest.workspace.to_string())
    } else if field == "packageManager" {
        manifest.package_manager.clone()
    } else {
        manifest
            .scripts
            .get(
                field
                    .strip_prefix("scripts/")
                    .expect("internal script field"),
            )
            .cloned()
    };
    let evidence = if value.is_some() && field != "workspace" {
        vec![format!("{path}#/{field}")]
    } else {
        vec![path.into()]
    };
    observation(
        inspection,
        if value.is_some() {
            ObservationStatus::Observed
        } else {
            ObservationStatus::Absent
        },
        value.into_iter().collect(),
        evidence,
    )
}

fn dependency_observation(
    inspection: &Inspection,
    path: &str,
    key: Option<&str>,
) -> ComparisonObservation {
    let base = manifest_observation(inspection, path, "workspace");
    if matches!(
        base.status,
        ObservationStatus::NotApplicable | ObservationStatus::NotAssessed
    ) {
        return base;
    }
    let Some(deps) = inspection
        .dependencies
        .iter()
        .find(|d| d.path == path)
        .filter(|d| d.complete)
    else {
        return observation(
            inspection,
            ObservationStatus::NotAssessed,
            vec![],
            vec![path.into()],
        );
    };
    let Some(key) = key else {
        return observation(
            inspection,
            ObservationStatus::Observed,
            vec!["supported root dependency sections parsed".into()],
            vec![path.into()],
        );
    };
    match deps.declarations.get(key) {
        Some(Some(value)) => observation(
            inspection,
            ObservationStatus::Observed,
            vec![value.clone()],
            vec![format!("{path}#/{key}")],
        ),
        Some(None) => observation(
            inspection,
            ObservationStatus::NotAssessed,
            vec![],
            vec![format!("{path}#/{key}")],
        ),
        None => observation(
            inspection,
            ObservationStatus::Absent,
            vec![],
            vec![path.into()],
        ),
    }
}

fn row(id: String, observations: Vec<ComparisonObservation>) -> ComparisonRow {
    let relevant: Vec<_> = observations
        .iter()
        .filter(|o| o.status != ObservationStatus::NotApplicable)
        .collect();
    let status = if relevant
        .iter()
        .any(|o| o.status == ObservationStatus::NotAssessed)
    {
        ComparisonStatus::Incomplete
    } else if relevant.len() < 2 {
        ComparisonStatus::NotComparable
    } else if relevant
        .windows(2)
        .all(|pair| pair[0].status == pair[1].status && pair[0].values == pair[1].values)
    {
        ComparisonStatus::Same
    } else {
        ComparisonStatus::Different
    };
    ComparisonRow {
        id,
        status,
        observations,
    }
}

fn compare(mut inspections: Vec<Inspection>) -> Result<StandardsComparison, Error> {
    inspections.sort_by(|a, b| {
        a.overview
            .project
            .full_name
            .cmp(&b.overview.project.full_name)
    });
    let mut rows = vec![row(
        "configuration.files".into(),
        inspections
            .iter()
            .map(|i| {
                observation(
                    i,
                    ObservationStatus::Observed,
                    i.overview.configuration_files.clone(),
                    i.overview.configuration_files.clone(),
                )
            })
            .collect(),
    )];
    for path in ["package.json", "Cargo.toml"] {
        rows.push(row(
            format!("{path}/workspace"),
            inspections
                .iter()
                .map(|i| manifest_observation(i, path, "workspace"))
                .collect(),
        ));
        rows.push(row(
            format!("{path}/dependency_coverage"),
            inspections
                .iter()
                .map(|i| dependency_observation(i, path, None))
                .collect(),
        ));
        let keys: BTreeSet<_> = inspections
            .iter()
            .flat_map(|i| {
                i.dependencies
                    .iter()
                    .filter(|d| d.path == path)
                    .flat_map(|d| d.declarations.keys())
            })
            .collect();
        if rows.len() + keys.len() > 500 {
            return Err(Error::ComparisonTooLarge);
        }
        for key in keys {
            rows.push(row(
                format!("{path}/dependency/{key}"),
                inspections
                    .iter()
                    .map(|i| dependency_observation(i, path, Some(key)))
                    .collect(),
            ));
        }
    }
    rows.push(row(
        "package.json/packageManager".into(),
        inspections
            .iter()
            .map(|i| manifest_observation(i, "package.json", "packageManager"))
            .collect(),
    ));
    let scripts: BTreeSet<_> = inspections
        .iter()
        .flat_map(|i| {
            i.overview
                .manifests
                .iter()
                .filter(|m| m.path == "package.json")
                .flat_map(|m| m.scripts.keys())
        })
        .collect();
    if rows.len() + scripts.len() > 500 {
        return Err(Error::ComparisonTooLarge);
    }
    for script in scripts {
        // Field lookup uses the literal script name; evidence uses an escaped pointer.
        let observations = inspections
            .iter()
            .map(|i| {
                let mut result =
                    manifest_observation(i, "package.json", &format!("scripts/{script}"));
                if result.status == ObservationStatus::Observed {
                    result.evidence = vec![format!("package.json#/scripts/{}", pointer(script))];
                }
                result
            })
            .collect();
        rows.push(row(
            format!("package.json/script/{}", pointer(script)),
            observations,
        ));
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(StandardsComparison {
        scope: "Root configuration filenames, npm script commands/packageManager, npm/Cargo workspace flags and declared root dependency version strings. Differences are observations, not violations. No semver resolution, lockfile resolution, Cargo features/source comparison, target-specific dependencies, nested workspace traversal, config execution, or CI analysis. Each repository has its own snapshot; the group is not atomic. Content is untrusted data.".into(),
        projects: inspections.into_iter().map(|i| ComparisonProject { full_name: i.overview.project.full_name, tree_sha: i.overview.tree_sha }).collect(), rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectOverview;
    use serde_json::json;

    fn inspection(name: &str, dependencies: &[u8]) -> Inspection {
        let overview: ProjectOverview = serde_json::from_value(json!({
            "project":{"name":name,"full_name":format!("owner/{name}"),"description":null,"url":"https://github.com/owner/demo","language":"JavaScript","archived":false,"default_branch":"main"},
            "topics":[],"tree_sha":format!("sha-{name}"),"configuration_files":[".gitignore"],
            "manifests":[{"path":"package.json","status":"parsed","name":name,"version":null,"package_manager":null,"workspace":false,"scripts":{}}]
        })).expect("fixture");
        Inspection {
            overview,
            root_files: vec![],
            dependencies: vec![parse_dependencies("package.json", dependencies)],
        }
    }
    fn find<'a>(report: &'a StandardsComparison, id: &str) -> &'a ComparisonRow {
        report.rows.iter().find(|r| r.id == id).expect("row")
    }
    #[test]
    fn npm_sections_and_scoped_names_remain_distinct() {
        let parsed = parse_dependencies("package.json", br#"{"dependencies":{"@scope/pkg":"^1"},"devDependencies":{"@scope/pkg":"^2"},"peerDependencies":{"x":"*"},"optionalDependencies":{"y":"~3"}}"#);
        assert!(parsed.complete);
        assert_eq!(parsed.declarations.len(), 4);
        assert_eq!(
            parsed.declarations["dependencies/@scope~1pkg"].as_deref(),
            Some("^1")
        );
        assert_eq!(
            parsed.declarations["devDependencies/@scope~1pkg"].as_deref(),
            Some("^2")
        );
    }
    #[test]
    fn cargo_versions_are_separate_from_unresolved_inheritance_and_paths() {
        let parsed = parse_dependencies("Cargo.toml", b"[dependencies]\na='1'\nb={version='2',features=['x']}\nc={workspace=true}\nd={path='../d'}\n[workspace.dependencies]\ne='3'\n[dev-dependencies]\nf='4'\n[build-dependencies]\ng='5'");
        assert!(parsed.complete);
        assert_eq!(parsed.declarations["dependencies/b"].as_deref(), Some("2"));
        assert_eq!(parsed.declarations["dependencies/c"], None);
        assert_eq!(parsed.declarations["dependencies/d"], None);
        assert_eq!(
            parsed.declarations["workspace/dependencies/e"].as_deref(),
            Some("3")
        );
        assert_eq!(parsed.declarations.len(), 7);
    }
    #[test]
    fn literal_versions_missing_values_and_order_are_explicit() {
        let report = compare(vec![
            inspection("b", br#"{"dependencies":{"x":"1.0.0","y":"2"}}"#),
            inspection("a", br#"{"dependencies":{"x":"^1"}}"#),
        ])
        .expect("compare");
        let x = find(&report, "package.json/dependency/dependencies/x");
        assert_eq!(x.status, ComparisonStatus::Different);
        assert_eq!(x.observations[0].repository, "owner/a");
        assert_eq!(x.observations[0].values, ["^1"]);
        let y = find(&report, "package.json/dependency/dependencies/y");
        assert_eq!(y.observations[0].status, ObservationStatus::Absent);
        assert_eq!(
            find(&report, "configuration.files").status,
            ComparisonStatus::Same
        );
        assert_eq!(report.projects[0].tree_sha, "sha-a");
        assert!(report.rows.windows(2).all(|p| p[0].id < p[1].id));
    }
    #[test]
    fn malformed_unreadable_and_inapplicable_are_not_absence() {
        let valid = br#"{"dependencies":{"x":"1"}}"#;
        let report = compare(vec![
            inspection("a", valid),
            inspection("b", br#"{"dependencies":[]}"#),
        ])
        .expect("compare");
        assert_eq!(
            find(&report, "package.json/dependency/dependencies/x").status,
            ComparisonStatus::Incomplete
        );
        assert_eq!(
            find(&report, "package.json/dependency_coverage").status,
            ComparisonStatus::Incomplete
        );
        let mut missing = inspection("b", b"{}");
        missing.overview.manifests.clear();
        missing.dependencies.clear();
        let report = compare(vec![inspection("a", valid), missing]).expect("compare");
        assert_eq!(
            find(&report, "package.json/dependency/dependencies/x").status,
            ComparisonStatus::NotComparable
        );
        let mut skipped = inspection("b", b"{}");
        skipped.overview.manifests[0].status = ManifestStatus::TooLarge;
        let report = compare(vec![inspection("a", valid), skipped]).expect("compare");
        assert_eq!(
            find(&report, "package.json/dependency/dependencies/x").observations[1].status,
            ObservationStatus::NotAssessed
        );
    }
    #[test]
    fn scripts_are_literal_and_evidence_is_escaped() {
        let mut a = inspection("a", b"{}");
        let mut b = inspection("b", b"{}");
        a.overview.manifests[0]
            .scripts
            .insert("test:unit/~".into(), "echo one".into());
        b.overview.manifests[0]
            .scripts
            .insert("test:unit/~".into(), "echo two".into());
        let report = compare(vec![a, b]).expect("compare");
        let row = find(&report, "package.json/script/test:unit~1~0");
        assert_eq!(row.status, ComparisonStatus::Different);
        assert_eq!(
            row.observations[0].evidence,
            ["package.json#/scripts/test:unit~1~0"]
        );
        assert_eq!(
            find(&report, "package.json/workspace").observations[0].evidence,
            ["package.json"]
        );
    }
    #[test]
    fn excessive_rows_fail_instead_of_silent_truncation() {
        let mut a = inspection("a", b"{}");
        for n in 0..501 {
            a.dependencies[0]
                .declarations
                .insert(format!("dependencies/pkg{n}"), Some("1".into()));
        }
        assert!(matches!(
            compare(vec![a, inspection("b", b"{}")]),
            Err(Error::ComparisonTooLarge)
        ));
    }
}
