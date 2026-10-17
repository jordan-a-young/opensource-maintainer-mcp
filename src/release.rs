use crate::{
    discovery::Discovery,
    error::Error,
    model::{
        AuditCheck, CheckStatus, GitBlob, GitTree, ObservationStatus, ReleaseJob, ReleasePackage,
        ReleaseReview, ReleaseWorkflow,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
const PACKAGE_LIMIT: usize = 256 * 1024;
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
fn empty_package(status: ObservationStatus) -> ReleasePackage {
    ReleasePackage {
        status,
        name: None,
        version: None,
        private: None,
        workspace: false,
        publish_access: None,
        registry: "not_assessed".into(),
        provenance: None,
        script_names: vec![],
        tooling: vec![],
    }
}
impl Discovery {
    pub async fn review_release_config(&self, repository: &str) -> Result<ReleaseReview, Error> {
        let repo = self.scoped_repository(repository).await?;
        let owner = &self.config.github.owner;
        let tree: GitTree = self
            .github
            .public_json(&[
                "repos",
                owner,
                &repo.name,
                "git",
                "trees",
                &repo.default_branch,
            ])
            .await?;
        if tree.truncated {
            return Err(Error::TruncatedTree);
        }
        let mut package = empty_package(ObservationStatus::NotApplicable);
        let mut checks = Vec::new();
        if let Some(entry) = tree.tree.iter().find(|e| e.path == "package.json") {
            if entry.kind != "blob"
                || !matches!(entry.mode.as_str(), "100644" | "100755")
                || entry.size.is_none_or(|s| s > PACKAGE_LIMIT)
            {
                package = empty_package(ObservationStatus::NotAssessed);
            } else {
                let blob: GitBlob = self
                    .github
                    .public_json(&["repos", owner, &repo.name, "git", "blobs", &entry.sha])
                    .await?;
                if blob.sha != entry.sha || blob.encoding != "base64" {
                    return Err(Error::InvalidResponse);
                }
                if blob.size > PACKAGE_LIMIT {
                    package = empty_package(ObservationStatus::NotAssessed);
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
                    (package, checks) = review_package(&bytes, &repo.full_name);
                }
            }
        }
        if package.status != ObservationStatus::Observed {
            checks.push(check("package.coverage", if package.status == ObservationStatus::NotApplicable { CheckStatus::NotApplicable } else { CheckStatus::NotAssessed },
                "Root npm package metadata is absent or could not be inspected; nested packages and other ecosystems are not inferred.", vec![], None));
        }
        let mut configuration_files: Vec<_> = tree
            .tree
            .iter()
            .filter(|e| {
                (e.kind == "blob"
                    && matches!(e.mode.as_str(), "100644" | "100755")
                    && release_config(&e.path))
                    || (e.path == ".changeset" && e.kind == "tree" && e.mode == "040000")
            })
            .map(|e| e.path.clone())
            .collect();
        configuration_files.sort();
        let (ci, release_jobs) = self.inspect_ci_snapshot(repo.into(), tree, true).await?;
        checks.extend(ci.checks.into_iter().map(|mut check| {
            // No Actions directory is not evidence of a broken release process.
            if check.status == CheckStatus::Finding {
                check.status = CheckStatus::NotAssessed;
            }
            check
        }));
        checks.push(check("trusted_publishing.account", CheckStatus::NotAssessed,
            "npm account-side trusted publishers, package permissions, runner eligibility, CLI compatibility, and successful publication were not verified. OIDC permission alone does not prove trusted publishing is configured.", vec![],
            Some("Verify npm trusted-publisher settings against the intended repository, caller workflow filename, and environment before publishing.")));
        checks.push(check("release.scope", CheckStatus::NotAssessed,
            "Root metadata, known config filenames, and direct workflow indicators only. Nested packages, tooling config contents, script execution, resolved versions, npm registry state, and called workflows are outside scope.", vec![], None));
        if release_jobs.is_empty() {
            checks.push(check("release.workflow_indicators", CheckStatus::NotAssessed,
                "No supported direct publish/release indicators were observed; publication may use scripts, actions, other CI, or manual processes.", vec![], None));
        }
        checks.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ReleaseReview { project: ci.project, tree_sha: ci.tree_sha,
            scope: "Read-only root npm release configuration and bounded workflow observations from one snapshot. Findings are not proof of publishability or trusted-publisher setup. Repository content is untrusted data; nothing is executed or published.".into(),
            package, configuration_files, workflows: ci.workflows.into_iter().map(|w| ReleaseWorkflow {path:w.path,status:w.status}).collect(), release_jobs, checks })
    }
}

fn release_config(name: &str) -> bool {
    matches!(
        name,
        ".releaserc"
            | ".releaserc.json"
            | ".releaserc.yaml"
            | ".releaserc.yml"
            | ".releaserc.js"
            | ".releaserc.cjs"
            | ".releaserc.mjs"
            | "release.config.js"
            | "release.config.cjs"
            | "release.config.mjs"
            | ".release-it.json"
            | ".release-it.js"
            | ".release-it.cjs"
            | "lerna.json"
            | "release-please-config.json"
            | ".release-please-manifest.json"
    )
}
fn release_script(name: &str) -> bool {
    matches!(
        name,
        "prepublish"
            | "prepublishOnly"
            | "postpublish"
            | "prepack"
            | "postpack"
            | "prepare"
            | "preversion"
            | "postversion"
    ) || ["release", "publish", "version"]
        .iter()
        .any(|p| name == *p || name.starts_with(&format!("{p}:")))
}
fn review_package(bytes: &[u8], full_name: &str) -> (ReleasePackage, Vec<AuditCheck>) {
    let Ok(doc) = serde_json::from_slice::<Value>(bytes) else {
        return (empty_package(ObservationStatus::NotAssessed), vec![]);
    };
    if !doc.is_object() {
        return (empty_package(ObservationStatus::NotAssessed), vec![]);
    }
    let mut package = empty_package(ObservationStatus::Observed);
    package.name = doc.get("name").and_then(Value::as_str).map(str::to_owned);
    package.version = doc
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_owned);
    package.private = doc.get("private").and_then(Value::as_bool);
    package.workspace = doc
        .get("workspaces")
        .is_some_and(|v| v.is_array() || v.is_object());
    if let Some(scripts) = doc.get("scripts").and_then(Value::as_object) {
        package.script_names = scripts
            .iter()
            .filter(|(name, v)| {
                release_script(name) && v.as_str().is_some_and(|s| !s.trim().is_empty())
            })
            .map(|(name, _)| name.clone())
            .collect();
    }
    for tool in [
        "@changesets/cli",
        "semantic-release",
        "release-it",
        "standard-version",
        "lerna",
    ] {
        if ["dependencies", "devDependencies"].iter().any(|section| {
            doc.get(*section)
                .and_then(Value::as_object)
                .is_some_and(|m| m.contains_key(tool))
        }) {
            package.tooling.push(tool.into());
        }
    }
    let mut checks = Vec::new();
    let private_unknown = doc.get("private").is_some() && package.private.is_none();
    checks.push(check("package.private", if private_unknown {CheckStatus::NotAssessed}else{CheckStatus::Passed},
        if private_unknown {"The private field has an unsupported type; publication intent is unknown."}else if package.private == Some(true){"private:true blocks publication of this root package; workspace member publication is separate."}else{"No private:true block is declared for the root package; this does not establish intent or ability to publish."},vec!["package.json".into()],None));
    for (field, value) in [
        ("name", package.name.as_deref()),
        ("version", package.version.as_deref()),
    ] {
        let status = if package.private == Some(true) {
            CheckStatus::NotApplicable
        } else if private_unknown || doc.get(field).is_some_and(|value| !value.is_string()) {
            CheckStatus::NotAssessed
        } else if value.is_some_and(|s| !s.trim().is_empty()) {
            CheckStatus::Passed
        } else {
            CheckStatus::Finding
        };
        checks.push(check(&format!("package.{field}"),status,
            "Public npm publication requires a name and version. This check observes nonempty strings only; it does not validate npm naming, SemVer, or registry availability. Private-root requirements are not applied to workspace members.",vec!["package.json".into()],None));
    }
    let publish = doc.get("publishConfig");
    if publish.is_some_and(|v| !v.is_object()) {
        checks.push(check(
            "package.publish_config",
            CheckStatus::NotAssessed,
            "publishConfig is not a supported object.",
            vec!["package.json#/publishConfig".into()],
            None,
        ));
    } else {
        package.publish_access = publish
            .and_then(|p| p.get("access"))
            .and_then(Value::as_str)
            .filter(|s| matches!(*s, "public" | "restricted"))
            .map(str::to_owned);
        package.provenance = publish
            .and_then(|p| p.get("provenance"))
            .and_then(Value::as_bool);
        package.registry = registry_class(publish.and_then(|p| p.get("registry"))).into();
        checks.push(check("package.registry", if package.registry=="insecure_or_credentialed"{CheckStatus::Finding}else{CheckStatus::NotAssessed},
            "Registry classification reflects only root publishConfig. Raw URLs are withheld; npmrc/environment/CLI overrides and actual destinations are unverified.",vec!["package.json".into()],
            if package.registry=="insecure_or_credentialed"{Some("Review registry transport and embedded credentials; keep credentials out of committed configuration.")}else{None}));
        if publish.and_then(|p| p.get("access")).is_some() && package.publish_access.is_none() {
            checks.push(check(
                "package.access",
                CheckStatus::NotAssessed,
                "Unrecognized publishConfig.access declaration.",
                vec!["package.json#/publishConfig/access".into()],
                None,
            ));
        }
        if package.name.as_deref().is_some_and(|n| n.starts_with('@'))
            && package.publish_access.is_none()
            && package.private != Some(true)
        {
            checks.push(check("package.scoped_access",CheckStatus::NotAssessed,"Scoped package publication access is not explicit in recognized root settings; CLI or existing registry state may supply it.",vec!["package.json".into()],Some("Confirm intended public/restricted access before publication.")));
        }
    }
    let repository = doc
        .get("repository")
        .and_then(|v| v.as_str().or_else(|| v.get("url").and_then(Value::as_str)));
    let matches = repository
        .and_then(github_repository)
        .map(|r| r.eq_ignore_ascii_case(full_name));
    checks.push(check("package.repository",match matches{Some(true)=>CheckStatus::Passed,Some(false)=>CheckStatus::Finding,None=>CheckStatus::NotAssessed},
        "Recognized repository coordinates are compared with the inspected GitHub project; this alone does not validate npm provenance or account-side trusted-publisher settings.",vec!["package.json".into()],None));
    (package, checks)
}
fn registry_class(value: Option<&Value>) -> &'static str {
    let Some(value) = value else {
        return "not_declared";
    };
    let Some(raw) = value.as_str() else {
        return "unrecognized";
    };
    let Ok(url) = reqwest::Url::parse(raw) else {
        return "unrecognized";
    };
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return "insecure_or_credentialed";
    }
    if url.host_str() == Some("registry.npmjs.org")
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none()
    {
        "npm_https"
    } else {
        "other_https"
    }
}
fn github_repository(raw: &str) -> Option<&str> {
    let path = raw
        .strip_prefix("git+https://github.com/")
        .or_else(|| raw.strip_prefix("https://github.com/"))
        .or_else(|| raw.strip_prefix("git@github.com:"))
        .or_else(|| raw.strip_prefix("github:"))?;
    let path = path
        .trim_end_matches('/')
        .strip_suffix(".git")
        .unwrap_or(path.trim_end_matches('/'));
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty() || name.is_empty() || path.contains(['?', '#', '@']) || name.contains('/') {
        None
    } else {
        Some(path)
    }
}
fn pointer(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn oidc(value: Option<&Value>) -> ObservationStatus {
    match value {
        Some(Value::String(s)) if s == "write-all" => ObservationStatus::Observed,
        Some(Value::String(s)) if s == "read-all" => ObservationStatus::Absent,
        Some(Value::Object(m)) => match m.get("id-token") {
            Some(Value::String(s)) if s == "write" => ObservationStatus::Observed,
            None => ObservationStatus::Absent,
            Some(Value::String(s)) if s == "none" => ObservationStatus::Absent,
            _ => ObservationStatus::NotAssessed,
        },
        _ => ObservationStatus::NotAssessed,
    }
}
fn token_env(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_object)
        .is_some_and(|m| m.contains_key("NODE_AUTH_TOKEN") || m.contains_key("NPM_TOKEN"))
}
fn command_indicator(command: &str) -> bool {
    command.lines().any(|line| {
        let words: Vec<_> = line.split_whitespace().collect();
        match words.as_slice() {
            ["npm" | "pnpm", "publish", ..] | ["yarn", "npm", "publish", ..] => true,
            ["npm" | "pnpm", "run", script, ..] => release_script(script),
            ["npx", "semantic-release" | "release-it" | "changeset", ..] => true,
            _ => false,
        }
    })
}

pub(crate) fn workflow_release_jobs(path: &str, doc: &Value) -> Vec<ReleaseJob> {
    let Some(jobs) = doc.get("jobs").and_then(Value::as_object) else {
        return vec![];
    };
    let mut result = Vec::new();
    for (id, job) in jobs {
        let location = format!("{path}#/jobs/{}", pointer(id));
        let mut indicators = Vec::new();
        let mut evidence = Vec::new();
        if job.get("uses").is_some() {
            indicators.push("reusable_workflow_not_traversed".into());
            evidence.push(format!("{location}/uses"));
        }
        let mut token = token_env(doc.get("env")) || token_env(job.get("env"));
        if let Some(steps) = job.get("steps").and_then(Value::as_array) {
            for (n, step) in steps.iter().enumerate() {
                token |= token_env(step.get("env"));
                if step
                    .get("run")
                    .and_then(Value::as_str)
                    .is_some_and(command_indicator)
                {
                    indicators.push("direct_publish_or_release_command".into());
                    evidence.push(format!("{location}/steps/{n}/run"));
                }
                if step.get("uses").and_then(Value::as_str).is_some_and(|s| {
                    s.starts_with("changesets/action@")
                        || s.starts_with("googleapis/release-please-action@")
                }) {
                    indicators.push("release_tool_action".into());
                    evidence.push(format!("{location}/steps/{n}/uses"));
                }
            }
        }
        if indicators.is_empty() {
            continue;
        }
        indicators.sort();
        indicators.dedup();
        let permission = oidc(job.get("permissions").or_else(|| doc.get("permissions")));
        let permission_evidence = if job.get("permissions").is_some() {
            format!("{location}/permissions")
        } else if doc.get("permissions").is_some() {
            format!("{path}#/permissions")
        } else {
            location.clone()
        };
        let checks = vec![
            check(
                "release.indicators",
                CheckStatus::NotAssessed,
                "Static release-related indicators are present; execution, target package, and actual publication were not verified.",
                evidence,
                None,
            ),
            check(
                "trusted_publishing.oidc_permission",
                match permission {
                    ObservationStatus::Observed => CheckStatus::Passed,
                    ObservationStatus::Absent => CheckStatus::NotAssessed,
                    _ => CheckStatus::NotAssessed,
                },
                "OIDC write permission is a prerequisite for npm trusted publishing, not proof of its configuration. Missing grants may be intentional for token-based or non-publishing jobs.",
                vec![permission_evidence],
                Some(
                    "If npm trusted publishing is intended, confirm effective id-token: write and account-side publisher settings.",
                ),
            ),
            check(
                "release.token_environment",
                CheckStatus::NotAssessed,
                if token {
                    "A token-named environment key was observed; it may be for installs or another step, and no value or use was inspected."
                } else {
                    "No direct token-named environment key was observed; this does not establish tokenless publication."
                },
                vec![location],
                None,
            ),
        ];
        result.push(ReleaseJob {
            workflow: path.into(),
            job: id.clone(),
            indicators,
            oidc_permission: permission,
            token_environment_present: token,
            checks,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn status<'a>(checks: &'a [AuditCheck], id: &str) -> &'a CheckStatus {
        &checks.iter().find(|c| c.id == id).expect("check").status
    }
    #[test]
    fn private_workspace_does_not_require_root_publication_metadata() {
        let (package, checks) = review_package(
            br#"{"private":true,"workspaces":["packages/*"]}"#,
            "owner/demo",
        );
        assert!(package.workspace);
        assert_eq!(
            status(&checks, "package.version"),
            &CheckStatus::NotApplicable
        );
        let (_, checks) = review_package(b"{}", "owner/demo");
        assert_eq!(status(&checks, "package.name"), &CheckStatus::Finding);
        let (_, checks) = review_package(br#"{"private":"true"}"#, "owner/demo");
        assert_eq!(
            status(&checks, "package.version"),
            &CheckStatus::NotAssessed
        );
    }
    #[test]
    fn publication_fields_distinguish_unsupported_absent_and_empty_values() {
        for field in ["name", "version"] {
            for value in [json!(42), json!({}), json!([]), json!(false), Value::Null] {
                let mut doc = json!({"name":"demo", "version":"1.0.0"});
                doc[field] = value;
                let bytes = serde_json::to_vec(&doc).expect("fixture");
                let (_, checks) = review_package(&bytes, "owner/demo");
                assert_eq!(
                    status(&checks, &format!("package.{field}")),
                    &CheckStatus::NotAssessed
                );
                doc["private"] = json!(true);
                let (_, checks) =
                    review_package(&serde_json::to_vec(&doc).expect("fixture"), "owner/demo");
                assert_eq!(
                    status(&checks, &format!("package.{field}")),
                    &CheckStatus::NotApplicable
                );
            }
            for value in ["", "   "] {
                let mut doc = json!({"name":"demo", "version":"1.0.0"});
                doc[field] = json!(value);
                let (_, checks) =
                    review_package(&serde_json::to_vec(&doc).expect("fixture"), "owner/demo");
                assert_eq!(
                    status(&checks, &format!("package.{field}")),
                    &CheckStatus::Finding
                );
            }
        }
        let (_, checks) = review_package(b"{}", "owner/demo");
        for field in ["name", "version"] {
            assert_eq!(
                status(&checks, &format!("package.{field}")),
                &CheckStatus::Finding
            );
        }
    }
    #[test]
    fn projects_metadata_without_script_bodies_or_registry_credentials() {
        let doc = json!({"name":"@owner/demo","version":"1.0.0","repository":{"url":"git+https://github.com/owner/demo.git"},"scripts":{"release":"SECRET_COMMAND","test":"test"},"devDependencies":{"semantic-release":"*"},"publishConfig":{"access":"public","provenance":true,"registry":"https://user:SECRET_PASSWORD@registry.npmjs.org"}});
        let (package, checks) = review_package(
            &serde_json::to_vec(&doc).expect("valid fixture result"),
            "owner/demo",
        );
        assert_eq!(package.script_names, ["release"]);
        assert_eq!(package.tooling, ["semantic-release"]);
        assert_eq!(package.publish_access.as_deref(), Some("public"));
        assert_eq!(package.provenance, Some(true));
        assert_eq!(status(&checks, "package.registry"), &CheckStatus::Finding);
        assert_eq!(status(&checks, "package.repository"), &CheckStatus::Passed);
        let output = serde_json::to_string(&(package, checks)).expect("valid fixture result");
        assert!(!output.contains("SECRET"));
    }
    #[test]
    fn malformed_and_unsupported_metadata_remain_unknown() {
        for bytes in [b"[".as_slice(), b"[]"] {
            assert_eq!(
                review_package(bytes, "owner/demo").0.status,
                ObservationStatus::NotAssessed
            );
        }
        let (_, checks) = review_package(
            br#"{"publishConfig":false,"repository":"https://github.com/other/repo"}"#,
            "owner/demo",
        );
        assert_eq!(
            status(&checks, "package.publish_config"),
            &CheckStatus::NotAssessed
        );
        assert_eq!(status(&checks, "package.repository"), &CheckStatus::Finding);
        assert_eq!(
            github_repository("https://github.com/owner/demo?secret=x"),
            None
        );
        assert_eq!(
            registry_class(Some(&json!("https://registry.npmjs.org/"))),
            "npm_https"
        );
        assert_eq!(
            registry_class(Some(&json!("https://example.com/"))),
            "other_https"
        );
        assert_eq!(registry_class(None), "not_declared");
    }
    #[test]
    fn job_permission_override_and_secrets_are_preserved_safely() {
        let doc = json!({"permissions":{"id-token":"write"},"jobs":{
            "publish":{"permissions":{"contents":"read"},"steps":[{"run":"npm publish --tag SECRET_COMMAND","env":{"NODE_AUTH_TOKEN":"SECRET_TOKEN"}}]},
            "inherit":{"steps":[{"uses":"changesets/action@v1"}]},
            "ordinary":{"steps":[{"run":"echo npm publish"}]}
        }});
        let jobs = workflow_release_jobs("release.yml", &doc);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].oidc_permission, ObservationStatus::Observed);
        assert_eq!(jobs[1].oidc_permission, ObservationStatus::Absent);
        assert!(jobs[1].token_environment_present);
        assert!(
            !serde_json::to_string(&jobs)
                .expect("valid fixture result")
                .contains("SECRET")
        );
        let jobs = workflow_release_jobs(
            "ci.yml",
            &json!({"jobs":{"call":{"uses":"./.github/workflows/other.yml"}}}),
        );
        assert_eq!(jobs[0].oidc_permission, ObservationStatus::NotAssessed);
        assert_eq!(jobs[0].indicators, ["reusable_workflow_not_traversed"]);
    }
    #[test]
    fn command_detection_is_conservative_and_never_executes() {
        for command in [
            "npm publish",
            "pnpm run release:next",
            "yarn npm publish",
            "npx semantic-release",
        ] {
            assert!(command_indicator(command));
        }
        for command in [
            "# npm publish",
            "echo npm publish",
            "npm test",
            "./release.sh",
        ] {
            assert!(!command_indicator(command));
        }
    }
}
