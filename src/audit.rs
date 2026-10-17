use crate::{
    discovery::Discovery,
    error::Error,
    model::{AuditCheck, CheckStatus, ManifestStatus, ManifestSummary, RepositoryAudit},
    overview::Inspection,
};

impl Discovery {
    pub async fn audit_repository(&self, repository: &str) -> Result<RepositoryAudit, Error> {
        Ok(evaluate(self.inspect_project(repository).await?))
    }
}

fn check(
    id: &str,
    status: CheckStatus,
    summary: &str,
    evidence: Vec<String>,
    recommendation: Option<&str>,
) -> AuditCheck {
    AuditCheck {
        id: id.into(),
        status,
        summary: summary.into(),
        evidence,
        recommendation: recommendation.map(str::to_owned),
    }
}

fn root_document(files: &[String], names: &[&str]) -> Vec<String> {
    files
        .iter()
        .filter(|file| {
            let lower = file.to_ascii_lowercase();
            names.iter().any(|name| {
                lower == *name
                    || lower.strip_prefix(name).is_some_and(|suffix| {
                        matches!(suffix, ".md" | ".markdown" | ".rst" | ".txt" | ".adoc")
                    })
            })
        })
        .cloned()
        .collect()
}

fn evaluate(inspection: Inspection) -> RepositoryAudit {
    let Inspection {
        overview,
        root_files,
    } = inspection;
    let mut checks = Vec::new();
    for (id, names, missing, recommendation) in [
        (
            "documentation.readme",
            &["readme"][..],
            "No recognized regular README file at the repository root.",
            "Add root setup/usage documentation or document where it lives.",
        ),
        (
            "documentation.license",
            &["license", "licence", "copying"][..],
            "No recognized regular license file at the repository root.",
            "Review and document the intended license; this check does not determine legal licensing status.",
        ),
    ] {
        let evidence = root_document(&root_files, names);
        checks.push(if evidence.is_empty() {
            check(
                id,
                CheckStatus::Finding,
                missing,
                evidence,
                Some(recommendation),
            )
        } else {
            check(
                id,
                CheckStatus::Passed,
                "A recognized root filename is present; its content was not assessed.",
                evidence,
                None,
            )
        });
    }
    let ignored = root_files.iter().any(|file| file == ".gitignore");
    checks.push(if ignored {
        check(
            "configuration.gitignore",
            CheckStatus::Passed,
            "A regular root .gitignore exists; ignore patterns were not assessed.",
            vec![".gitignore".into()],
            None,
        )
    } else {
        check(
            "configuration.gitignore",
            CheckStatus::Finding,
            "No regular root .gitignore was found.",
            vec![],
            Some("Consider documenting ignore rules for generated files and local credentials."),
        )
    });

    for path in ["package.json", "Cargo.toml"] {
        let manifest = overview.manifests.iter().find(|m| m.path == path);
        checks.push(match manifest {
            None => check(&format!("manifest.{path}"), CheckStatus::NotApplicable, "This root manifest is absent; other ecosystems and nested packages are outside scope.", vec![], None),
            Some(m) => match m.status {
                ManifestStatus::Parsed => check(&format!("manifest.{path}"), CheckStatus::Passed, "Supported manifest fields were parsed; this is not full package-manager validation.", vec![path.into()], None),
                ManifestStatus::Invalid => check(&format!("manifest.{path}"), CheckStatus::Finding, "The root manifest could not be parsed into the supported fields.", vec![path.into()], Some("Check manifest syntax and field types with the package manager.")),
                ManifestStatus::TooLarge | ManifestStatus::UnsupportedFileType => check(&format!("manifest.{path}"), CheckStatus::NotAssessed, "The manifest was skipped because it exceeds the size limit or is not a regular file.", vec![path.into()], Some("Review this manifest manually; dependent checks cannot draw conclusions.")),
            }
        });
    }

    let npm = overview.manifests.iter().find(|m| m.path == "package.json");
    checks.push(script_check(npm, "test"));
    checks.push(script_check(npm, "lint"));
    let lockfiles: Vec<String> = [
        "package-lock.json",
        "npm-shrinkwrap.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lock",
        "bun.lockb",
    ]
    .into_iter()
    .filter(|name| root_files.iter().any(|file| file == name))
    .map(str::to_owned)
    .collect();
    let managers: std::collections::BTreeSet<_> =
        lockfiles.iter().map(|file| lock_manager(file)).collect();
    checks.push(if managers.len() > 1 {
        check("configuration.multiple_package_managers", CheckStatus::Finding, "Root lockfiles from multiple package managers are present.", lockfiles.clone(), Some("Confirm whether multiple managers are intentional; otherwise keep the authoritative lockfile."))
    } else {
        check("configuration.multiple_package_managers", CheckStatus::Passed, "No conflicting package-manager lockfile families were observed at the root; lockfile contents were not inspected.", lockfiles.clone(), None)
    });
    checks.push(match npm {
        Some(m) if matches!(m.status, ManifestStatus::Parsed) => match m.package_manager.as_deref().and_then(|value| value.split_once('@')) {
            Some((manager, _)) if ["npm", "yarn", "pnpm", "bun"].contains(&manager) => {
                let mut evidence = vec!["package.json#/packageManager".into()];
                evidence.extend(lockfiles);
                if managers.iter().any(|observed| *observed != manager) {
                    check("configuration.package_manager", CheckStatus::Finding, "The declared package manager differs from an observed lockfile family.", evidence, Some("Align packageManager with the intended lockfiles or document the intentional difference."))
                } else if managers.is_empty() {
                    check("configuration.package_manager", CheckStatus::NotAssessed, "A package manager is declared but no recognized root lockfile is available for comparison.", evidence, None)
                } else {
                    check("configuration.package_manager", CheckStatus::Passed, "The declared package manager matches the observed lockfile family.", evidence, None)
                }
            }
            _ => check("configuration.package_manager", CheckStatus::NotAssessed, "No supported packageManager declaration was available for comparison.", vec!["package.json".into()], None),
        },
        Some(_) => check("configuration.package_manager", CheckStatus::NotAssessed, "The npm manifest was not parsed, so its package manager cannot be compared.", vec!["package.json".into()], None),
        None => check("configuration.package_manager", CheckStatus::NotApplicable, "No root npm manifest is present.", vec![], None),
    });
    checks.push(check("coverage.limitations", CheckStatus::NotAssessed,
        "Test contents, test execution, Rust test/lint setup, configuration contents, nested packages, CI, and inherited organization policies were not assessed.", vec![], None));
    checks.sort_by(|a, b| a.id.cmp(&b.id));
    RepositoryAudit { project: overview.project, tree_sha: overview.tree_sha,
        scope: "Root regular filenames and bounded npm/Cargo manifest fields from one Git tree snapshot. Findings describe observations, not proof of repository-wide absence or quality.".into(), checks }
}

fn lock_manager(file: &str) -> &'static str {
    match file {
        "package-lock.json" | "npm-shrinkwrap.json" => "npm",
        "yarn.lock" => "yarn",
        "pnpm-lock.yaml" => "pnpm",
        _ => "bun",
    }
}

fn script_check(manifest: Option<&ManifestSummary>, kind: &str) -> AuditCheck {
    let id = format!("npm.{kind}_script");
    let Some(manifest) = manifest else {
        return check(
            &id,
            CheckStatus::NotApplicable,
            "No root npm manifest is present.",
            vec![],
            None,
        );
    };
    if !matches!(manifest.status, ManifestStatus::Parsed) {
        return check(
            &id,
            CheckStatus::NotAssessed,
            "The root npm manifest was not parsed; scripts cannot be assessed.",
            vec!["package.json".into()],
            None,
        );
    }
    let evidence: Vec<_> = manifest
        .scripts
        .iter()
        .filter(|(name, command)| {
            (name.as_str() == kind || name.starts_with(&format!("{kind}:")))
                && !command.trim().is_empty()
        })
        .map(|(name, _)| {
            format!(
                "package.json#/scripts/{}",
                name.replace('~', "~0").replace('/', "~1")
            )
        })
        .collect();
    if !evidence.is_empty() {
        check(
            &id,
            CheckStatus::Passed,
            "A nonempty conventional script entry exists; its behavior and effectiveness were not assessed.",
            evidence,
            None,
        )
    } else if manifest.workspace {
        check(
            &id,
            CheckStatus::NotAssessed,
            "No conventional root script entry was found; workspace members may define their own tooling.",
            vec!["package.json".into()],
            Some("Inspect workspace member scripts or document the root orchestration command."),
        )
    } else {
        check(
            &id,
            CheckStatus::Finding,
            "No nonempty conventional root script entry was found; tooling may exist elsewhere.",
            vec!["package.json".into()],
            Some(
                "Add a conventional script entry or document the actual command used for this check.",
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Project, ProjectOverview};
    use std::collections::BTreeMap;

    fn audit(files: &[&str], manifest: Option<ManifestSummary>) -> RepositoryAudit {
        evaluate(Inspection {
            root_files: files.iter().map(|name| (*name).into()).collect(),
            overview: ProjectOverview {
                project: Project {
                    name: "demo".into(),
                    full_name: "owner/demo".into(),
                    description: None,
                    url: "https://github.com/owner/demo".into(),
                    language: None,
                    archived: false,
                    default_branch: "main".into(),
                },
                topics: vec![],
                tree_sha: "snapshot".into(),
                manifests: manifest.into_iter().collect(),
                configuration_files: vec![],
            },
        })
    }
    fn npm() -> ManifestSummary {
        ManifestSummary {
            path: "package.json".into(),
            status: ManifestStatus::Parsed,
            name: None,
            version: None,
            package_manager: None,
            workspace: false,
            scripts: BTreeMap::new(),
        }
    }
    fn result<'a>(audit: &'a RepositoryAudit, id: &str) -> &'a AuditCheck {
        audit
            .checks
            .iter()
            .find(|check| check.id == id)
            .expect("check exists")
    }
    #[test]
    fn documentation_checks_use_exact_root_names_and_observed_evidence() {
        let report = audit(&["rEaDmE.MD", "COPYING", ".gitignore"], None);
        for id in [
            "documentation.readme",
            "documentation.license",
            "configuration.gitignore",
        ] {
            assert_eq!(result(&report, id).status, CheckStatus::Passed);
            assert_eq!(result(&report, id).evidence.len(), 1);
        }
        let report = audit(&["docs/README.md", "README.md.bak", "LICENSE.js"], None);
        assert_eq!(
            result(&report, "documentation.readme").status,
            CheckStatus::Finding
        );
        assert!(result(&report, "documentation.readme").evidence.is_empty());
        assert_eq!(
            result(&report, "documentation.license").status,
            CheckStatus::Finding
        );
    }
    #[test]
    fn distinguishes_missing_scripts_from_workspace_uncertainty() {
        let mut manifest = npm();
        manifest.scripts.insert("test".into(), "   ".into());
        manifest.scripts.insert("pretest".into(), "setup".into());
        let report = audit(&[], Some(manifest));
        assert_eq!(
            result(&report, "npm.test_script").status,
            CheckStatus::Finding
        );
        let mut manifest = npm();
        manifest.workspace = true;
        let report = audit(&[], Some(manifest));
        assert_eq!(
            result(&report, "npm.test_script").status,
            CheckStatus::NotAssessed
        );
        assert_eq!(
            result(&report, "npm.lint_script").status,
            CheckStatus::NotAssessed
        );
    }
    #[test]
    fn script_presence_does_not_claim_execution_or_expose_commands() {
        let mut manifest = npm();
        manifest
            .scripts
            .insert("test:unit".into(), "SECRET_DO_NOT_EXECUTE".into());
        manifest
            .scripts
            .insert("lint:ci".into(), "echo no-op".into());
        let report = audit(&[], Some(manifest));
        assert_eq!(
            result(&report, "npm.test_script").evidence,
            ["package.json#/scripts/test:unit"]
        );
        assert_eq!(
            result(&report, "npm.lint_script").status,
            CheckStatus::Passed
        );
        assert!(
            !serde_json::to_string(&report)
                .expect("json")
                .contains("SECRET")
        );
    }
    #[test]
    fn unparsed_manifests_never_generate_missing_script_findings() {
        for status in [
            ManifestStatus::Invalid,
            ManifestStatus::TooLarge,
            ManifestStatus::UnsupportedFileType,
        ] {
            let mut manifest = npm();
            manifest.status = status;
            let report = audit(&[], Some(manifest));
            assert_eq!(
                result(&report, "npm.test_script").status,
                CheckStatus::NotAssessed
            );
            assert_eq!(
                result(&report, "configuration.package_manager").status,
                CheckStatus::NotAssessed
            );
        }
    }
    #[test]
    fn lockfiles_compare_families_not_just_file_count() {
        let mut manifest = npm();
        manifest.package_manager = Some("npm@11".into());
        let report = audit(
            &["package-lock.json", "npm-shrinkwrap.json"],
            Some(manifest),
        );
        assert_eq!(
            result(&report, "configuration.multiple_package_managers").status,
            CheckStatus::Passed
        );
        assert_eq!(
            result(&report, "configuration.package_manager").status,
            CheckStatus::Passed
        );
        let mut manifest = npm();
        manifest.package_manager = Some("pnpm@10".into());
        let report = audit(&["yarn.lock", "package-lock.json"], Some(manifest));
        assert_eq!(
            result(&report, "configuration.multiple_package_managers").status,
            CheckStatus::Finding
        );
        assert_eq!(
            result(&report, "configuration.package_manager").status,
            CheckStatus::Finding
        );
    }
    #[test]
    fn absent_lockfiles_or_unsupported_managers_are_not_assessed() {
        for manager in [None, Some("unknown@1"), Some("yarn@4")] {
            let mut manifest = npm();
            manifest.package_manager = manager.map(str::to_owned);
            let report = audit(&[], Some(manifest));
            assert_eq!(
                result(&report, "configuration.package_manager").status,
                CheckStatus::NotAssessed
            );
        }
    }
    #[test]
    fn rust_and_unsupported_ecosystems_do_not_require_npm_scripts() {
        let report = audit(&["Cargo.toml"], None);
        assert_eq!(
            result(&report, "npm.test_script").status,
            CheckStatus::NotApplicable
        );
        assert_eq!(
            result(&report, "coverage.limitations").status,
            CheckStatus::NotAssessed
        );
        assert_eq!(report.tree_sha, "snapshot");
        assert!(report.checks.windows(2).all(|pair| pair[0].id < pair[1].id));
    }
}
