use base64::{Engine, engine::general_purpose::STANDARD};
use opensource_maintainer_mcp::{
    config::Config, discovery::Discovery, error::Error, github::GithubClient,
    mcp::MaintainerServer, model::CheckStatus,
};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::json;
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
async fn listing(mock: &MockServer) {
    Mock::given(path("/users/owner/repos")).respond_with(ResponseTemplate::new(200).set_body_json(json!([{
        "name":"demo", "full_name":"owner/demo", "description":null, "html_url":"https://github.com/owner/demo", "language":"Rust",
        "archived":false,"default_branch":"main","topics":["rust"],"private":false
    }]))).expect(1).mount(mock).await;
}
async fn fixture(mock: &MockServer) {
    listing(mock).await;
    let content =
        br#"{"scripts":{"test":"echo never-execute","lint":"eslint"},"packageManager":"pnpm@10"}"#;
    Mock::given(path("/repos/owner/demo/git/trees/main")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "sha":"snapshot", "truncated":false,"tree":[
            {"path":"package.json","sha":"manifest","type":"blob","mode":"100644","size":content.len()},
            {"path":"README.md","sha":"readme","type":"blob","mode":"100644","size":10},
            {"path":"LICENSE","sha":"symlink","type":"blob","mode":"120000","size":10},
            {"path":"yarn.lock","sha":"lock","type":"blob","mode":"100644","size":10}
        ]
    }))).expect(1).mount(mock).await;
    Mock::given(path("/repos/owner/demo/git/blobs/manifest")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "sha":"manifest", "encoding":"base64", "size":content.len(),"content":STANDARD.encode(content)
    }))).expect(1).mount(mock).await;
}

#[tokio::test]
async fn audit_reuses_one_snapshot_without_fetching_document_contents() {
    let mock = MockServer::start().await;
    fixture(&mock).await;
    let report = service(&mock)
        .audit_repository("demo")
        .await
        .expect("audit");
    assert_eq!(report.project.full_name, "owner/demo");
    assert_eq!(report.tree_sha, "snapshot");
    let status = |id| {
        &report
            .checks
            .iter()
            .find(|c| c.id == id)
            .expect("check")
            .status
    };
    assert_eq!(status("documentation.readme"), &CheckStatus::Passed);
    assert_eq!(status("documentation.license"), &CheckStatus::Finding);
    assert_eq!(
        status("configuration.package_manager"),
        &CheckStatus::Finding
    );
    assert_eq!(status("npm.test_script"), &CheckStatus::Passed);
    let requests = mock.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 3);
    for request in requests
        .iter()
        .filter(|r| r.url.path().starts_with("/repos/"))
    {
        assert!(!request.headers.contains_key("authorization"));
    }
}

#[tokio::test]
async fn failed_inspection_never_turns_into_missing_file_findings() {
    let mock = MockServer::start().await;
    assert!(matches!(
        service(&mock).audit_repository("../private").await,
        Err(Error::InvalidRepositoryName)
    ));
    listing(&mock).await;
    Mock::given(path("/repos/owner/demo/git/trees/main"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock)
        .await;
    assert!(matches!(
        service(&mock).audit_repository("demo").await,
        Err(Error::RateLimited { .. })
    ));
}

#[tokio::test]
async fn mcp_audit_schema_invocation_and_parameter_errors() {
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
        .find(|t| t.name == "audit_repository")
        .expect("audit tool");
    assert!(tool.output_schema.is_some());
    assert_eq!(tool.input_schema["required"], json!(["repository"]));
    let result = client
        .call_tool(
            CallToolRequestParams::new("audit_repository").with_arguments(
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
        result.structured_content.expect("structured")["tree_sha"],
        "snapshot"
    );
    for args in [
        json!({}),
        json!({"repository":"../bad"}),
        json!({"repository":42}),
        json!({"repository":"demo","fix":true}),
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new("audit_repository")
                    .with_arguments(args.as_object().expect("args").clone()),
            )
            .await
            .expect("tool result");
        assert_eq!(result.is_error, Some(true));
    }
    client.cancel().await.expect("cancel");
    task.await.expect("join");
}
