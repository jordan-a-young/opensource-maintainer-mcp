use base64::{Engine, engine::general_purpose::STANDARD};
use opensource_maintainer_mcp::{
    config::Config,
    discovery::Discovery,
    error::Error,
    github::GithubClient,
    mcp::MaintainerServer,
    model::{CheckStatus, WorkflowStatus},
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
async fn fixture(mock: &MockServer) {
    let content=b"on: [push, pull_request]\npermissions: {contents: read}\njobs:\n  test:\n    runs-on: ubuntu-latest\n    timeout-minutes: 10\n    steps:\n      - uses: actions/checkout@v7\n      - run: echo data";
    directories(
        mock,
        vec![
            entry("ci.yml", "blob", "blob", "100644", content.len()),
            entry("notes.txt", "ignored", "blob", "100644", 9),
        ],
    )
    .await;
    Mock::given(path("/repos/owner/demo/git/blobs/blob")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":"blob","encoding":"base64","size":content.len(),"content":STANDARD.encode(content)}))).expect(1).mount(mock).await;
}
#[tokio::test]
async fn traverses_pinned_directory_trees_and_only_reads_workflow_blobs() {
    let mock = MockServer::start().await;
    fixture(&mock).await;
    let report = service(&mock).analyze_ci("demo").await.expect("analysis");
    assert_eq!(report.tree_sha, "root");
    assert_eq!(report.workflows.len(), 1);
    assert_eq!(report.workflows[0].status, WorkflowStatus::Analyzed);
    assert_eq!(report.workflows[0].triggers, ["pull_request", "push"]);
    let requests = mock.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 5);
    for r in requests
        .iter()
        .filter(|r| r.url.path().starts_with("/repos/"))
    {
        assert!(!r.headers.contains_key("authorization"));
    }
}
#[tokio::test]
async fn missing_and_symlink_directories_are_distinct() {
    for symlink in [false, true] {
        let mock = MockServer::start().await;
        listing(&mock).await;
        tree(
            &mock,
            "main",
            "root",
            if symlink {
                vec![entry(".github", "link", "blob", "120000", 10)]
            } else {
                vec![]
            },
            false,
        )
        .await;
        let report = service(&mock).analyze_ci("demo").await.expect("report");
        assert!(report.workflows.is_empty());
        assert_eq!(
            report.checks[0].status,
            if symlink {
                CheckStatus::NotAssessed
            } else {
                CheckStatus::Finding
            }
        );
        assert_eq!(mock.received_requests().await.expect("requests").len(), 2);
    }
}
#[tokio::test]
async fn oversized_symlink_and_empty_workflow_directories_do_not_fetch_blobs() {
    let mock = MockServer::start().await;
    directories(
        &mock,
        vec![
            entry("a.yml", "a", "blob", "100644", 131073),
            entry("b.yaml", "b", "blob", "120000", 1),
        ],
    )
    .await;
    let report = service(&mock).analyze_ci("demo").await.expect("report");
    assert_eq!(report.workflows[0].status, WorkflowStatus::TooLarge);
    assert_eq!(
        report.workflows[1].status,
        WorkflowStatus::UnsupportedFileType
    );
    assert_eq!(mock.received_requests().await.expect("requests").len(), 4);
    let mock = MockServer::start().await;
    directories(&mock, vec![]).await;
    assert_eq!(
        service(&mock)
            .analyze_ci("demo")
            .await
            .expect("report")
            .checks[0]
            .status,
        CheckStatus::Finding
    );
}
#[tokio::test]
async fn truncated_tree_and_excessive_files_fail_without_partial_reports() {
    let mock = MockServer::start().await;
    listing(&mock).await;
    tree(&mock, "main", "root", vec![], true).await;
    assert!(matches!(
        service(&mock).analyze_ci("demo").await,
        Err(Error::TruncatedTree)
    ));
    let mock = MockServer::start().await;
    directories(
        &mock,
        (0..21)
            .map(|n| entry(&format!("{n}.yml"), "blob", "blob", "100644", 1))
            .collect(),
    )
    .await;
    assert!(matches!(
        service(&mock).analyze_ci("demo").await,
        Err(Error::CiAnalysisLimit)
    ));
}
#[tokio::test]
async fn scope_validation_and_upstream_failure_do_not_become_missing_ci() {
    let mock = MockServer::start().await;
    assert!(matches!(
        service(&mock).analyze_ci("../bad").await,
        Err(Error::InvalidRepositoryName)
    ));
    listing(&mock).await;
    assert!(matches!(
        service(&mock).analyze_ci("outside").await,
        Err(Error::ProjectNotFound)
    ));
    assert_eq!(mock.received_requests().await.expect("requests").len(), 1);
    let mock = MockServer::start().await;
    directories(&mock, vec![entry("ci.yml", "blob", "blob", "100644", 1)]).await;
    Mock::given(path("/repos/owner/demo/git/blobs/blob"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock)
        .await;
    assert!(matches!(
        service(&mock).analyze_ci("demo").await,
        Err(Error::RateLimited { .. })
    ));
}
#[tokio::test]
async fn mcp_ci_schema_invocation_and_invalid_parameters() {
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
    let tool = tools.iter().find(|t| t.name == "analyze_ci").expect("tool");
    assert!(tool.output_schema.is_some());
    assert_eq!(tool.input_schema["required"], json!(["repository"]));
    let result = client
        .call_tool(
            CallToolRequestParams::new("analyze_ci").with_arguments(
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
                CallToolRequestParams::new("analyze_ci")
                    .with_arguments(args.as_object().expect("args").clone()),
            )
            .await
            .expect("result");
        assert_eq!(result.is_error, Some(true));
    }
    client.cancel().await.expect("cancel");
    task.await.expect("join");
}
