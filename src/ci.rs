use crate::{
    discovery::Discovery,
    error::Error,
    model::{
        AuditCheck, CheckStatus, CiAnalysis, CiJob, CiWorkflow, GitBlob, GitTree, WorkflowStatus,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
const FILE_LIMIT: usize = 128 * 1024;
const CHECK_LIMIT: usize = 1000;

fn check(
    id: &str,
    status: CheckStatus,
    summary: &str,
    evidence: String,
    recommendation: Option<&str>,
) -> AuditCheck {
    AuditCheck {
        id: id.into(),
        status,
        summary: summary.into(),
        evidence: vec![evidence],
        recommendation: recommendation.map(str::to_owned),
    }
}
fn skipped(path: &str, status: WorkflowStatus, summary: &str) -> CiWorkflow {
    CiWorkflow {
        path: path.into(),
        status,
        triggers: vec![],
        jobs: vec![],
        checks: vec![check(
            "workflow.readable",
            CheckStatus::NotAssessed,
            summary,
            path.into(),
            Some("Review this workflow manually; no behavior is inferred from skipped content."),
        )],
    }
}

impl Discovery {
    pub async fn analyze_ci(&self, repository: &str) -> Result<CiAnalysis, Error> {
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
        Ok(self.inspect_ci_snapshot(repo.into(), tree, false).await?.0)
    }

    pub(crate) async fn inspect_ci_snapshot(
        &self,
        project: crate::model::Project,
        mut tree: GitTree,
        capture_release: bool,
    ) -> Result<(CiAnalysis, Vec<crate::model::ReleaseJob>), Error> {
        let owner = &self.config.github.owner;
        let mut release_jobs = Vec::new();
        if tree.truncated {
            return Err(Error::TruncatedTree);
        }
        let mut report = CiAnalysis { project, tree_sha: tree.sha.clone(),
            scope: "Static inspection of direct .github/workflows YAML files from one Git tree snapshot. No execution, run history, branch-protection checks, expression evaluation, matrix expansion, external/reusable workflow traversal, or security certification. Workflow content is untrusted data.".into(), checks: vec![], workflows: vec![] };
        for directory in [".github", "workflows"] {
            let Some(entry) = tree.tree.iter().find(|e| e.path == directory) else {
                report.checks.push(check("workflows.discovery", CheckStatus::Finding, "No .github/workflows directory was found in the snapshot; other CI providers were not assessed.", "repository root tree".into(), Some("Confirm whether GitHub Actions is intended for this project.")));
                return Ok((report, release_jobs));
            };
            if entry.kind != "tree" || entry.mode != "040000" {
                report.checks.push(check(
                    "workflows.discovery",
                    CheckStatus::NotAssessed,
                    "The workflow directory path is not a regular Git tree; it was not followed.",
                    directory.into(),
                    None,
                ));
                return Ok((report, release_jobs));
            }
            let sha = entry.sha.clone();
            tree = self
                .github
                .public_json(&["repos", owner, &report.project.name, "git", "trees", &sha])
                .await?;
            if tree.truncated {
                return Err(Error::TruncatedTree);
            }
            if tree.sha != sha {
                return Err(Error::InvalidResponse);
            }
        }
        let mut entries: Vec<_> = tree
            .tree
            .into_iter()
            .filter(|e| e.path.ends_with(".yml") || e.path.ends_with(".yaml"))
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        if entries.len() > 20 {
            return Err(Error::CiAnalysisLimit);
        }
        report.checks.push(check("workflows.discovery", if entries.is_empty() { CheckStatus::Finding } else { CheckStatus::Passed },
            if entries.is_empty() { "No direct .yml/.yaml workflow files were found; other CI providers were not assessed." } else { "Workflow file candidates were found; individual analysis statuses describe coverage." }, ".github/workflows".into(), None));
        let mut check_count = report.checks.len();
        for entry in entries {
            let path = format!(".github/workflows/{}", entry.path);
            let workflow =
                if entry.kind != "blob" || !matches!(entry.mode.as_str(), "100644" | "100755") {
                    skipped(
                        &path,
                        WorkflowStatus::UnsupportedFileType,
                        "Workflow candidate is not a regular file; symlinks are never followed.",
                    )
                } else if entry.size.is_none_or(|size| size > FILE_LIMIT) {
                    skipped(
                        &path,
                        WorkflowStatus::TooLarge,
                        "Workflow size is unknown or exceeds 128 KiB.",
                    )
                } else {
                    let blob: GitBlob = self
                        .github
                        .public_json(&[
                            "repos",
                            owner,
                            &report.project.name,
                            "git",
                            "blobs",
                            &entry.sha,
                        ])
                        .await?;
                    if blob.sha != entry.sha || blob.encoding != "base64" {
                        return Err(Error::InvalidResponse);
                    }
                    if blob.size > FILE_LIMIT {
                        skipped(&path, WorkflowStatus::TooLarge, "Workflow exceeds 128 KiB.")
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
                        let workflow = analyze_workflow(&path, &bytes)?;
                        if capture_release
                            && workflow.status == WorkflowStatus::Analyzed
                            && let Some(doc) = parse_workflow(&bytes)
                        {
                            release_jobs.extend(crate::release::workflow_release_jobs(&path, &doc));
                        }
                        workflow
                    }
                };
            check_count += workflow.checks.len();
            if check_count > CHECK_LIMIT {
                return Err(Error::CiAnalysisLimit);
            }
            report.workflows.push(workflow);
        }
        Ok((report, release_jobs))
    }
}

fn parse_workflow(bytes: &[u8]) -> Option<Value> {
    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! { max_depth: 32, max_events: 20000, max_aliases: 0, max_anchors: 0 },
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
    };
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|s| serde_saphyr::from_str_with_options::<Value>(s, options).ok())
}

fn analyze_workflow(path: &str, bytes: &[u8]) -> Result<CiWorkflow, Error> {
    let parsed = parse_workflow(bytes);
    let Some(doc) = parsed.filter(Value::is_object) else {
        return Ok(skipped(
            path,
            WorkflowStatus::NotAssessed,
            "YAML could not be read within supported syntax/budgets (including duplicate keys or anchors); raw content is not exposed.",
        ));
    };
    let triggers = match &doc["on"] {
        Value::String(event) => vec![event.clone()],
        Value::Array(events) if events.iter().all(Value::is_string) => events
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Value::Object(events) => events.keys().cloned().collect(),
        _ => {
            return Ok(skipped(
                path,
                WorkflowStatus::NotAssessed,
                "The workflow trigger shape is missing or unsupported.",
            ));
        }
    };
    if triggers.is_empty() || triggers.iter().any(|event| event.trim().is_empty()) {
        return Ok(skipped(
            path,
            WorkflowStatus::NotAssessed,
            "No nonempty triggers were available for assessment.",
        ));
    }
    let Some(jobs) = doc["jobs"].as_object().filter(|j| !j.is_empty()) else {
        return Ok(skipped(
            path,
            WorkflowStatus::NotAssessed,
            "The workflow jobs mapping is missing, empty, or unsupported.",
        ));
    };
    if jobs.len() > 50 {
        return Err(Error::CiAnalysisLimit);
    }
    let mut report = CiWorkflow {
        path: path.into(),
        status: WorkflowStatus::Analyzed,
        triggers,
        jobs: vec![],
        checks: vec![],
    };
    report.triggers.sort();
    report.triggers.dedup();
    let privileged = report
        .triggers
        .iter()
        .any(|t| matches!(t.as_str(), "pull_request_target" | "workflow_run"));
    report.checks.push(check("workflow.elevated_trigger", if privileged { CheckStatus::Finding } else { CheckStatus::Passed },
        if privileged { "An elevated-context trigger is declared; review how untrusted inputs and code are handled. This is not proof of a vulnerability." } else { "No pull_request_target or workflow_run trigger was observed." }, format!("{path}#/on"), if privileged { Some("Review trust boundaries before checking out or executing contributed code.") } else { None }));
    let concurrency =
        doc.get("concurrency").is_some() || jobs.values().all(|j| j.get("concurrency").is_some());
    report.checks.push(check("workflow.concurrency", if concurrency { CheckStatus::Passed } else { CheckStatus::Finding },
        if concurrency { "A workflow-level concurrency key or keys on every job are present; expressions and cancellation behavior were not assessed." } else { "No workflow-level concurrency key and not every job declares one; overlapping runs may be intentional." }, path.into(), if concurrency { None } else { Some("Consider whether overlapping runs need grouping or cancellation.") }));
    for (id, job) in jobs {
        let location = format!("{path}#/jobs/{}", pointer(id));
        if !job.is_object() {
            report.checks.push(check(
                &format!("job.{id}.structure"),
                CheckStatus::NotAssessed,
                "Job is not a supported mapping.",
                location,
                None,
            ));
            continue;
        }
        let reusable = job.get("uses").is_some();
        let steps = job.get("steps").and_then(Value::as_array);
        if steps.is_some_and(|s| s.len() > 200) {
            return Err(Error::CiAnalysisLimit);
        }
        report.jobs.push(CiJob {
            id: id.clone(),
            reusable_workflow: reusable,
            step_count: steps.map_or(0, Vec::len),
        });
        let permission = job.get("permissions").or_else(|| doc.get("permissions"));
        let (status, summary) = permission_check(permission);
        let evidence = if job.get("permissions").is_some() {
            format!("{location}/permissions")
        } else if doc.get("permissions").is_some() {
            format!("{path}#/permissions")
        } else {
            location.clone()
        };
        report.checks.push(check(&format!("job.{id}.permissions"), status, summary, evidence, Some("Keep token permissions explicit and limited to the job's requirements; repository defaults were not read.")));
        if reusable {
            report.checks.push(check(&format!("job.{id}.timeout"), CheckStatus::NotApplicable, "This job calls a reusable workflow; its internal timeouts and jobs were not inspected.", location.clone(), None));
            report.checks.push(reference_check(
                &format!("job.{id}.reference"),
                &job["uses"],
                format!("{location}/uses"),
            ));
        } else {
            let (status, summary) = match job.get("timeout-minutes") {
                None => (
                    CheckStatus::Finding,
                    "No explicit job timeout is declared; GitHub's default applies.",
                ),
                Some(v) if v.as_u64().is_some_and(|n| (1..=360).contains(&n)) => (
                    CheckStatus::Passed,
                    "An explicit positive job timeout within 360 minutes is declared.",
                ),
                _ => (
                    CheckStatus::NotAssessed,
                    "Timeout is dynamic or outside supported numeric values; it was not evaluated.",
                ),
            };
            report.checks.push(check(
                &format!("job.{id}.timeout"),
                status,
                summary,
                location.clone(),
                None,
            ));
            if let Some(steps) = steps {
                for (index, step) in steps.iter().enumerate() {
                    if let Some(reference) = step.get("uses") {
                        report.checks.push(reference_check(
                            &format!("job.{id}.step.{index}.reference"),
                            reference,
                            format!("{location}/steps/{index}/uses"),
                        ));
                    } else if !step.is_object() || step.get("run").is_none() {
                        report.checks.push(check(
                            &format!("job.{id}.step.{index}.structure"),
                            CheckStatus::NotAssessed,
                            "Step does not expose a supported run or uses field.",
                            format!("{location}/steps/{index}"),
                            None,
                        ));
                    }
                }
            } else {
                report.checks.push(check(
                    &format!("job.{id}.steps"),
                    CheckStatus::NotAssessed,
                    "No supported steps array was available; command coverage cannot be assessed.",
                    location,
                    None,
                ));
            }
        }
        if report.checks.len() > CHECK_LIMIT {
            return Err(Error::CiAnalysisLimit);
        }
    }
    report.checks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(report)
}

fn permission_check(value: Option<&Value>) -> (CheckStatus, &'static str) {
    match value {
        None => (
            CheckStatus::NotAssessed,
            "Token permissions are inherited from unknown repository/organization defaults.",
        ),
        Some(Value::String(s)) if s == "write-all" => (
            CheckStatus::Finding,
            "Effective declared token permissions use write-all; verify this broad access is necessary.",
        ),
        Some(Value::String(s)) if s == "read-all" => (
            CheckStatus::Passed,
            "Effective declared token permissions use read-all; actual access was not queried.",
        ),
        Some(Value::Object(map))
            if map
                .values()
                .all(|v| matches!(v.as_str(), Some("read" | "write" | "none"))) =>
        {
            if map.values().any(|v| v == "write") {
                (
                    CheckStatus::Finding,
                    "Specific write permissions are declared; they may be intentional and need review.",
                )
            } else {
                (
                    CheckStatus::Passed,
                    "Effective declared permission mapping contains no write grants (including an empty mapping).",
                )
            }
        }
        _ => (
            CheckStatus::NotAssessed,
            "Permission declaration has an unsupported or dynamic shape.",
        ),
    }
}
fn pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn reference_check(id: &str, value: &Value, evidence: String) -> AuditCheck {
    let (status, summary) = match value.as_str() {
        Some(s) if s.contains("${{") => (
            CheckStatus::NotAssessed,
            "Dynamic action/workflow reference was not evaluated.",
        ),
        Some(s) if s.starts_with("./") => (
            CheckStatus::NotApplicable,
            "Local action/workflow reference; its contents were not traversed.",
        ),
        Some(s) if s.starts_with("docker://") => {
            if s.rsplit_once("@sha256:").is_some_and(|(_, digest)| {
                digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())
            }) {
                (
                    CheckStatus::Passed,
                    "Container reference includes a full SHA-256 digest; image contents were not verified.",
                )
            } else {
                (
                    CheckStatus::Finding,
                    "Container reference is not pinned to a full SHA-256 digest.",
                )
            }
        }
        Some(s) if s.contains('/') => {
            if s.rsplit_once('@').is_some_and(|(_, reference)| {
                reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
            }) {
                (
                    CheckStatus::Passed,
                    "Remote action/workflow reference uses a full commit SHA; provenance was not verified.",
                )
            } else {
                (
                    CheckStatus::Finding,
                    "Remote action/workflow reference does not use a full commit SHA.",
                )
            }
        }
        _ => (CheckStatus::NotAssessed, "Reference shape is unsupported."),
    };
    check(
        id,
        status,
        summary,
        evidence,
        Some(
            "Review remote references and pin immutable revisions where appropriate; local and called contents are outside this analysis.",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn analyze(source: &str) -> CiWorkflow {
        analyze_workflow(".github/workflows/ci.yml", source.as_bytes()).expect("analysis")
    }
    fn status<'a>(report: &'a CiWorkflow, id: &str) -> &'a CheckStatus {
        &report
            .checks
            .iter()
            .find(|c| c.id == id)
            .expect("check")
            .status
    }
    #[test]
    fn yaml_on_key_and_trigger_forms_are_preserved() {
        for trigger in [
            "push",
            "[push, pull_request]",
            "{push: null, workflow_dispatch: null}",
        ] {
            let report = analyze(&format!(
                "on: {trigger}\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo data"
            ));
            assert_eq!(report.status, WorkflowStatus::Analyzed);
            assert!(report.triggers.contains(&"push".into()));
            assert_eq!(report.jobs[0].step_count, 1);
        }
    }
    #[test]
    fn job_permissions_override_workflow_and_unknown_defaults_stay_unknown() {
        let report = analyze(
            "on: push\npermissions: write-all\njobs:\n  a:\n    permissions: {}\n    steps: []\n  b:\n    steps: []",
        );
        assert_eq!(status(&report, "job.a.permissions"), &CheckStatus::Passed);
        assert_eq!(status(&report, "job.b.permissions"), &CheckStatus::Finding);
        let report = analyze("on: push\njobs:\n  a:\n    steps: []");
        assert_eq!(
            status(&report, "job.a.permissions"),
            &CheckStatus::NotAssessed
        );
        assert_eq!(
            permission_check(Some(&serde_json::json!({"contents":"read"}))).0,
            CheckStatus::Passed
        );
        assert_eq!(
            permission_check(Some(&serde_json::json!({"id-token":"write"}))).0,
            CheckStatus::Finding
        );
        assert_eq!(
            permission_check(Some(&serde_json::json!({"contents":42}))).0,
            CheckStatus::NotAssessed
        );
    }
    #[test]
    fn timeouts_and_reusable_jobs_are_distinguished() {
        let report = analyze(
            "on: workflow_call\njobs:\n  a:\n    timeout-minutes: 15\n    steps: []\n  b:\n    timeout-minutes: '${{ inputs.timeout }}'\n    steps: []\n  c:\n    steps: []\n  d:\n    uses: ./.github/workflows/build.yml",
        );
        for (id, expected) in [
            ("a", CheckStatus::Passed),
            ("b", CheckStatus::NotAssessed),
            ("c", CheckStatus::Finding),
            ("d", CheckStatus::NotApplicable),
        ] {
            assert_eq!(status(&report, &format!("job.{id}.timeout")), &expected);
        }
        assert!(
            report
                .jobs
                .iter()
                .find(|j| j.id == "d")
                .expect("job")
                .reusable_workflow
        );
    }
    #[test]
    fn pinning_checks_handle_sha_tags_local_docker_and_expressions() {
        for (reference, expected) in [
            (
                format!("actions/checkout@{}", "a".repeat(40)),
                CheckStatus::Passed,
            ),
            ("actions/checkout@v7".into(), CheckStatus::Finding),
            ("./.github/actions/setup".into(), CheckStatus::NotApplicable),
            (
                format!("docker://image@sha256:{}", "b".repeat(64)),
                CheckStatus::Passed,
            ),
            ("docker://image:latest".into(), CheckStatus::Finding),
            ("${{ matrix.action }}".into(), CheckStatus::NotAssessed),
            (
                "owner/repo/.github/workflows/build.yml@main".into(),
                CheckStatus::Finding,
            ),
        ] {
            assert_eq!(
                reference_check("pin", &Value::String(reference), "file".into()).status,
                expected
            );
        }
    }
    #[test]
    fn privileged_triggers_and_job_level_concurrency_are_observations() {
        let report = analyze(
            "on: [pull_request_target, workflow_run]\njobs:\n  a:\n    concurrency: test\n    steps: []",
        );
        assert_eq!(
            status(&report, "workflow.elevated_trigger"),
            &CheckStatus::Finding
        );
        assert_eq!(
            status(&report, "workflow.concurrency"),
            &CheckStatus::Passed
        );
    }
    #[test]
    fn invalid_duplicate_and_anchored_yaml_is_not_assessed_without_leaks() {
        for source in [
            "SECRET: [",
            "on: push\non: pull_request\njobs: {}",
            "on: push\njobs: &SECRET {}",
            "[]",
            "on: push\njobs: []",
        ] {
            let report = analyze(source);
            assert_eq!(report.status, WorkflowStatus::NotAssessed);
            assert!(
                !serde_json::to_string(&report)
                    .expect("json")
                    .contains("SECRET")
            );
        }
    }
    #[test]
    fn commands_are_never_returned_or_executed() {
        let report = analyze(
            "on: push\njobs:\n  a:\n    steps:\n      - run: SECRET_DO_NOT_EXECUTE\n      - uses: actions/checkout@v7",
        );
        assert!(
            !serde_json::to_string(&report)
                .expect("json")
                .contains("SECRET")
        );
        assert_eq!(
            status(&report, "job.a.step.1.reference"),
            &CheckStatus::Finding
        );
    }
    #[test]
    fn job_and_parser_depth_limits_are_enforced() {
        let jobs: serde_json::Map<_, _> = (0..51)
            .map(|n| (format!("j{n}"), serde_json::json!({"steps":[]})))
            .collect();
        let source = serde_json::json!({"on":"push","jobs":jobs}).to_string();
        assert!(matches!(
            analyze_workflow("ci.yml", source.as_bytes()),
            Err(Error::CiAnalysisLimit)
        ));
        let source = format!("on: push\njobs: {}0{}", "[".repeat(100), "]".repeat(100));
        assert_eq!(analyze(&source).status, WorkflowStatus::NotAssessed);
    }
}
