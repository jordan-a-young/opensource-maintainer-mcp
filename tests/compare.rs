use base64::{Engine, engine::general_purpose::STANDARD};
use opensource_maintainer_mcp::{
    config::Config, discovery::Discovery, error::Error, github::GithubClient,
    mcp::MaintainerServer, model::ComparisonStatus,
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
    let repos: Vec<_> = ["a","b"].iter().map(|name| json!({"name":name,"full_name":format!("owner/{name}"),"description":null,"html_url":format!("https://github.com/owner/{name}"),"language":"JavaScript","archived":false,"default_branch":"main","topics":["rust"],"private":false})).collect();
    Mock::given(path("/users/owner/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(repos))
        .expect(1)
        .mount(mock)
        .await;
}
async fn contents(mock: &MockServer, name: &str, version: &str) {
    let content =
        json!({"dependencies":{"react":version},"scripts":{"test":"echo data"}}).to_string();
    Mock::given(path(format!("/repos/owner/{name}/git/trees/main"))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":format!("tree-{name}"),"truncated":false,"tree":[{"path":"package.json","sha":format!("blob-{name}"),"type":"blob","mode":"100644","size":content.len()}]}))).expect(1).mount(mock).await;
    Mock::given(path(format!("/repos/owner/{name}/git/blobs/blob-{name}"))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":format!("blob-{name}"),"encoding":"base64","size":content.len(),"content":STANDARD.encode(content)}))).expect(1).mount(mock).await;
}
#[tokio::test]
async fn compares_two_snapshots_with_one_listing_and_public_reads() {
    let mock = MockServer::start().await;
    listing(&mock).await;
    contents(&mock, "a", "^18").await;
    contents(&mock, "b", "^19").await;
    let report = service(&mock)
        .compare_standards(&["B".into(), "a".into()])
        .await
        .expect("compare");
    assert_eq!(report.projects[0].full_name, "owner/a");
    assert_eq!(report.projects[1].tree_sha, "tree-b");
    assert_eq!(
        report
            .rows
            .iter()
            .find(|r| r.id == "package.json/dependency/dependencies/react")
            .expect("row")
            .status,
        ComparisonStatus::Different
    );
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
async fn rejects_bad_selection_before_any_request() {
    let mock = MockServer::start().await;
    for names in [
        vec![],
        vec!["a"],
        vec!["a", "A"],
        vec!["a", "../b"],
        vec!["a", "b", "c", "d", "e", "f"],
    ] {
        assert!(
            service(&mock)
                .compare_standards(&names.into_iter().map(str::to_owned).collect::<Vec<_>>())
                .await
                .is_err()
        );
    }
    assert!(mock.received_requests().await.expect("requests").is_empty());
}
#[tokio::test]
async fn validates_all_projects_before_contents_and_propagates_failures() {
    let mock = MockServer::start().await;
    listing(&mock).await;
    assert!(matches!(
        service(&mock)
            .compare_standards(&["a".into(), "outside".into()])
            .await,
        Err(Error::ProjectNotFound)
    ));
    assert_eq!(mock.received_requests().await.expect("requests").len(), 1);
    let mock = MockServer::start().await;
    listing(&mock).await;
    contents(&mock, "a", "1").await;
    Mock::given(path("/repos/owner/b/git/trees/main"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock)
        .await;
    assert!(matches!(
        service(&mock)
            .compare_standards(&["a".into(), "b".into()])
            .await,
        Err(Error::RateLimited { .. })
    ));
}
#[tokio::test]
async fn mcp_schema_invocation_and_invalid_arguments() {
    let mock = MockServer::start().await;
    listing(&mock).await;
    contents(&mock, "a", "1").await;
    contents(&mock, "b", "1").await;
    let (server_io, client_io) = tokio::io::duplex(65536);
    let server = MaintainerServer::new(service(&mock));
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
        .find(|t| t.name == "compare_standards")
        .expect("tool");
    assert!(tool.output_schema.is_some());
    assert_eq!(tool.input_schema["required"], json!(["repositories"]));
    let result = client
        .call_tool(
            CallToolRequestParams::new("compare_standards").with_arguments(
                json!({"repositories":["a","b"]})
                    .as_object()
                    .expect("args")
                    .clone(),
            ),
        )
        .await
        .expect("call");
    assert_ne!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.expect("structured")["projects"][0]["tree_sha"],
        "tree-a"
    );
    for args in [
        json!({}),
        json!({"repositories":"a"}),
        json!({"repositories":["a","A"]}),
        json!({"repositories":["a","b"],"fix":true}),
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new("compare_standards")
                    .with_arguments(args.as_object().expect("args").clone()),
            )
            .await
            .expect("result");
        assert_eq!(result.is_error, Some(true));
    }
    client.cancel().await.expect("cancel");
    task.await.expect("join");
}

#[tokio::test]
async fn rejects_private_archived_and_wrong_topic_before_content_reads() {
    for change in [
        json!({"private":true}),
        json!({"archived":true}),
        json!({"topics":["other"]}),
    ] {
        let mock = MockServer::start().await;
        let mut repos: Vec<_> = ["a","b"].iter().map(|name| json!({"name":name,"full_name":format!("owner/{name}"),"description":null,"html_url":"https://github.com/owner/demo","language":null,"archived":false,"default_branch":"main","topics":["rust"],"private":false})).collect();
        for (key, value) in change.as_object().expect("object") {
            repos[1][key] = value.clone();
        }
        Mock::given(path("/users/owner/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(repos))
            .expect(1)
            .mount(&mock)
            .await;
        assert!(matches!(
            service(&mock)
                .compare_standards(&["a".into(), "b".into()])
                .await,
            Err(Error::ProjectNotFound)
        ));
        assert_eq!(mock.received_requests().await.expect("requests").len(), 1);
    }
}
