use opensource_maintainer_mcp::{
    config::Config, discovery::Discovery, github::GithubClient, mcp::MaintainerServer,
};
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use std::time::Duration;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

#[tokio::test]
async fn discovers_and_invokes_tool_with_structured_output_and_errors() {
    for status in [200, 401] {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(serde_json::json!([{
                    "name":"demo", "full_name":"owner/demo", "description":null,
                    "html_url":"https://github.com/owner/demo", "language":"Rust",
                    "archived":false, "default_branch":"main", "topics":["rust"], "private":false
                }])),
            )
            .expect(1)
            .mount(&mock)
            .await;
        let config = Config::parse("[github]\nowner='owner'\ntopic='rust'").expect("config");
        let github =
            GithubClient::with_endpoint(&format!("{}/", mock.uri()), None, Duration::from_secs(2))
                .expect("client");
        let server = MaintainerServer::new(Discovery::new(config, github).expect("discovery"));
        let (server_io, client_io) = tokio::io::duplex(65536);
        let server_task = tokio::spawn(async move {
            server
                .serve(server_io)
                .await
                .expect("serve")
                .waiting()
                .await
                .expect("server exit");
        });
        let client = ().serve(client_io).await.expect("initialize");
        let tools = client.list_all_tools().await.expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "list_projects");
        assert!(tools[0].output_schema.is_some());
        let result = client
            .call_tool(CallToolRequestParams::new("list_projects"))
            .await
            .expect("call");
        if status == 200 {
            assert_ne!(result.is_error, Some(true));
            assert_eq!(
                result.structured_content.expect("structured output")["projects"][0]["full_name"],
                "owner/demo"
            );
        } else {
            assert_eq!(result.is_error, Some(true));
            assert!(
                result.content[0]
                    .as_text()
                    .expect("error text")
                    .text
                    .contains("authentication")
            );
        }
        client.cancel().await.expect("cancel");
        server_task.await.expect("join");
    }
}

#[tokio::test]
async fn executable_speaks_stdio_and_discovers_tool_without_network() {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_opensource-maintainer-mcp"));
    command
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/config.toml"))
        .env_remove("GITHUB_TOKEN");
    let transport = TokioChildProcess::new(command).expect("spawn");
    let client = ().serve(transport).await.expect("stdio initialization");
    let tools = client.list_all_tools().await.expect("tools/list");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "list_projects");
    client.cancel().await.expect("shutdown");
}

#[test]
fn invalid_configuration_fails_on_stderr_without_protocol_noise() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_opensource-maintainer-mcp"))
        .arg("/nonexistent/config.toml")
        .output()
        .expect("spawn");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read configuration"));
}
