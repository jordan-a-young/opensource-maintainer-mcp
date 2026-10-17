use base64::{Engine, engine::general_purpose::STANDARD};
use opensource_maintainer_mcp::{
    config::Config,
    discovery::Discovery,
    error::Error,
    github::GithubClient,
    mcp::MaintainerServer,
    model::{CheckStatus, ObservationStatus, WorkflowStatus},
};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};
fn service(mock: &MockServer) -> Discovery {
    Discovery::new(
        Config::parse("[github]\nowner='owner'\ntopic='rust'").expect("config"),
        GithubClient::with_endpoint(
            &format!("{}/", mock.uri()),
            Some("fake-token"),
            Duration::from_secs(2),
        )
        .expect("http"),
    )
    .expect("service")
}
fn entry(name: &str, sha: &str, kind: &str, mode: &str, size: usize) -> Value {
    json!({"path":name,"sha":sha,"type":kind,"mode":mode,"size":size})
}
async fn tree(mock: &MockServer, reference: &str, sha: &str, entries: Vec<Value>, truncated: bool) {
    Mock::given(path(format!("/repos/owner/demo/git/trees/{reference}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha":sha,"tree":entries,"truncated":truncated})),
        )
        .expect(1)
        .mount(mock)
        .await;
}
async fn listing(mock: &MockServer) {
    Mock::given(path("/users/owner/repos")).respond_with(ResponseTemplate::new(200).set_body_json(json!([{"name":"demo","full_name":"owner/demo","description":null,"html_url":"https://github.com/owner/demo","language":"Rust","archived":false,"default_branch":"main","topics":["rust"],"private":false}]))).expect(1).mount(mock).await;
}
async fn directories(mock: &MockServer, entries: Vec<Value>) {
    listing(mock).await;
    tree(
        mock,
        "main",
        "root",
        vec![entry(".github", "github", "tree", "040000", 0)],
        false,
    )
    .await;
    tree(
        mock,
        "github",
        "github",
        vec![entry("workflows", "workflows", "tree", "040000", 0)],
        false,
    )
    .await;
    tree(mock, "workflows", "workflows", entries, false).await;
}
async fn blob(mock: &MockServer, sha: &str, bytes: &[u8]) {
    Mock::given(path(format!("/repos/owner/demo/git/blobs/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":sha,"encoding":"base64","size":bytes.len(),"content":STANDARD.encode(bytes)})))
        .expect(1).mount(mock).await;
}
async fn fixture(mock: &MockServer) {
    let package = br#"{"private":true,"workspaces":["packages/*"]}"#;
    let workflow = b"on: push\npermissions: {id-token: write}\njobs:\n  publish:\n    runs-on: ubuntu-latest\n    steps:\n      - run: npm publish\n";
    listing(mock).await;
    tree(
        mock,
        "main",
        "root",
        vec![
            entry("package.json", "package", "blob", "100644", package.len()),
            entry(".github", "github", "tree", "040000", 0),
            entry(".changeset", "changeset", "tree", "040000", 0),
            entry("release.config.js", "config", "blob", "100644", 20),
        ],
        false,
    )
    .await;
    blob(mock, "package", package).await;
    tree(
        mock,
        "github",
        "github",
        vec![entry("workflows", "workflows", "tree", "040000", 0)],
        false,
    )
    .await;
    tree(
        mock,
        "workflows",
        "workflows",
        vec![entry(
            "release.yml",
            "workflow",
            "blob",
            "100644",
            workflow.len(),
        )],
        false,
    )
    .await;
    blob(mock, "workflow", workflow).await;
}
#[tokio::test]
async fn one_snapshot_anonymous_reads_and_account_state_unknown() {
    let mock = MockServer::start().await;
    fixture(&mock).await;
    let report = service(&mock)
        .review_release_config("demo")
        .await
        .expect("valid fixture result");
    assert_eq!(report.tree_sha, "root");
    assert_eq!(report.package.private, Some(true));
    assert_eq!(
        report.configuration_files,
        [".changeset", "release.config.js"]
    );
    assert_eq!(report.workflows[0].status, WorkflowStatus::Analyzed);
    assert_eq!(
        report.release_jobs[0].oidc_permission,
        ObservationStatus::Observed
    );
    assert_eq!(
        report
            .checks
            .iter()
            .find(|c| c.id == "trusted_publishing.account")
            .expect("valid fixture result")
            .status,
        CheckStatus::NotAssessed
    );
    let requests = mock
        .received_requests()
        .await
        .expect("valid fixture result");
    assert_eq!(requests.len(), 6);
    for r in requests
        .iter()
        .filter(|r| r.url.path().starts_with("/repos/"))
    {
        assert!(!r.headers.contains_key("authorization"));
    }
}
#[tokio::test]
async fn absent_symlink_and_oversized_root_packages_are_not_fetched() {
    for (entries, status) in [
        (vec![], ObservationStatus::NotApplicable),
        (
            vec![entry("package.json", "link", "blob", "120000", 5)],
            ObservationStatus::NotAssessed,
        ),
        (
            vec![entry("package.json", "large", "blob", "100644", 262145)],
            ObservationStatus::NotAssessed,
        ),
    ] {
        let mock = MockServer::start().await;
        listing(&mock).await;
        tree(&mock, "main", "root", entries, false).await;
        let report = service(&mock)
            .review_release_config("demo")
            .await
            .expect("valid fixture result");
        assert_eq!(report.package.status, status);
        assert!(report.release_jobs.is_empty());
        assert_eq!(
            mock.received_requests()
                .await
                .expect("valid fixture result")
                .len(),
            2
        );
    }
}
#[tokio::test]
async fn workspace_declarations_preserve_unknown_and_supported_states() {
    for (declaration, status, workspace) in [
        (Some(json!(true)), ObservationStatus::NotAssessed, false),
        (Some(json!(false)), ObservationStatus::NotAssessed, false),
        (
            Some(json!("packages/*")),
            ObservationStatus::NotAssessed,
            false,
        ),
        (Some(json!(42)), ObservationStatus::NotAssessed, false),
        (None, ObservationStatus::Observed, false),
        (Some(Value::Null), ObservationStatus::Observed, false),
        (Some(json!([])), ObservationStatus::Observed, true),
        (
            Some(json!(["packages/*"])),
            ObservationStatus::Observed,
            true,
        ),
        (
            Some(json!({"packages":["packages/*"]})),
            ObservationStatus::Observed,
            true,
        ),
    ] {
        let mut package = json!({"private":true});
        if let Some(declaration) = declaration {
            package["workspaces"] = declaration;
        }
        let bytes = serde_json::to_vec(&package).expect("fixture");
        let mock = MockServer::start().await;
        listing(&mock).await;
        tree(
            &mock,
            "main",
            "root",
            vec![entry(
                "package.json",
                "package",
                "blob",
                "100644",
                bytes.len(),
            )],
            false,
        )
        .await;
        blob(&mock, "package", &bytes).await;
        let report = service(&mock)
            .review_release_config("demo")
            .await
            .expect("review");
        assert_eq!(report.package.status, status, "{package}");
        assert_eq!(report.package.workspace, workspace, "{package}");
        let package_checks: Vec<_> = report
            .checks
            .iter()
            .filter(|c| c.id.starts_with("package."))
            .collect();
        if status == ObservationStatus::NotAssessed {
            assert_eq!(package_checks.len(), 1);
            assert_eq!(package_checks[0].id, "package.coverage");
            assert_eq!(package_checks[0].status, CheckStatus::NotAssessed);
        } else {
            assert!(!package_checks.iter().any(|c| c.id == "package.coverage"));
            for id in ["package.name", "package.version"] {
                assert_eq!(
                    package_checks
                        .iter()
                        .find(|c| c.id == id)
                        .expect("check")
                        .status,
                    CheckStatus::NotApplicable
                );
            }
        }
    }
}
#[tokio::test]
async fn skipped_workflow_does_not_generate_release_claims() {
    let mock = MockServer::start().await;
    directories(
        &mock,
        vec![entry("release.yml", "bad", "blob", "100644", 1)],
    )
    .await;
    blob(&mock, "bad", b"[").await;
    let report = service(&mock)
        .review_release_config("demo")
        .await
        .expect("valid fixture result");
    assert_eq!(report.workflows[0].status, WorkflowStatus::NotAssessed);
    assert!(report.release_jobs.is_empty());
}
#[tokio::test]
async fn invalid_scope_truncation_and_http_failures_remain_errors() {
    let mock = MockServer::start().await;
    assert!(matches!(
        service(&mock).review_release_config("../bad").await,
        Err(Error::InvalidRepositoryName)
    ));
    listing(&mock).await;
    assert!(matches!(
        service(&mock).review_release_config("outside").await,
        Err(Error::ProjectNotFound)
    ));
    assert_eq!(
        mock.received_requests()
            .await
            .expect("valid fixture result")
            .len(),
        1
    );
    let mock = MockServer::start().await;
    listing(&mock).await;
    tree(&mock, "main", "root", vec![], true).await;
    assert!(matches!(
        service(&mock).review_release_config("demo").await,
        Err(Error::TruncatedTree)
    ));
    let mock = MockServer::start().await;
    listing(&mock).await;
    tree(
        &mock,
        "main",
        "root",
        vec![entry("package.json", "package", "blob", "100644", 1)],
        false,
    )
    .await;
    Mock::given(path("/repos/owner/demo/git/blobs/package"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock)
        .await;
    assert!(matches!(
        service(&mock).review_release_config("demo").await,
        Err(Error::RateLimited { .. })
    ));
}

#[tokio::test]
async fn mcp_release_schema_invocation_and_invalid_parameters() {
    let mock = MockServer::start().await;
    fixture(&mock).await;
    let server = MaintainerServer::new(service(&mock));
    let (server_io, client_io) = tokio::io::duplex(65536);
    let task = tokio::spawn(async move {
        server
            .serve(server_io)
            .await
            .expect("serve")
            .waiting()
            .await
            .expect("wait");
    });
    let client = ().serve(client_io).await.expect("client");
    let tools = client.list_all_tools().await.expect("tools");
    let tool = tools
        .iter()
        .find(|t| t.name == "review_release_config")
        .expect("tool");
    assert!(tool.output_schema.is_some());
    assert_eq!(tool.input_schema["required"], json!(["repository"]));
    let result = client
        .call_tool(
            CallToolRequestParams::new("review_release_config").with_arguments(
                json!({"repository":"demo"})
                    .as_object()
                    .expect("args")
                    .clone(),
            ),
        )
        .await
        .expect("call");
    assert_ne!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.expect("json")["workflows"][0]["status"],
        "analyzed"
    );
    for args in [
        json!({}),
        json!({"repository":42}),
        json!({"repository":"../bad"}),
        json!({"repository":"demo","fix":true}),
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new("review_release_config")
                    .with_arguments(args.as_object().expect("args").clone()),
            )
            .await
            .expect("result");
        assert_eq!(result.is_error, Some(true));
    }
    client.cancel().await.expect("cancel");
    task.await.expect("join");
}
