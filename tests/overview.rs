use base64::{Engine, engine::general_purpose::STANDARD};
use opensource_maintainer_mcp::{
    config::Config, discovery::Discovery, error::Error, github::GithubClient,
    mcp::MaintainerServer, model::ManifestStatus,
};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn repository() -> Value {
    json!({"name":"demo", "full_name":"owner/demo", "description":null,
        "html_url":"https://github.com/owner/demo", "language":"Rust", "archived":false,
        "default_branch":"main", "topics":["rust"], "private":false})
}
fn entry(name: &str, sha: &str, size: usize) -> Value {
    json!({"path":name, "sha":sha, "type":"blob", "mode":"100644", "size":size})
}
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
async fn listing(mock: &MockServer, repos: Value) {
    Mock::given(method("GET"))
        .and(path("/users/owner/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(repos))
        .expect(1)
        .mount(mock)
        .await;
}
async fn tree(mock: &MockServer, entries: Vec<Value>, truncated: bool) {
    Mock::given(method("GET"))
        .and(path("/repos/owner/demo/git/trees/main"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha":"snapshot", "tree":entries, "truncated":truncated})),
        )
        .expect(1)
        .mount(mock)
        .await;
}
async fn blob(mock: &MockServer, sha: &str, content: &[u8]) {
    Mock::given(method("GET")).and(path(format!("/repos/owner/demo/git/blobs/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":sha,"encoding":"base64","size":content.len(),"content":format!("{}\n", STANDARD.encode(content))})))
        .expect(1).mount(mock).await;
}
async fn fixture(mock: &MockServer) {
    listing(mock, json!([repository()])).await;
    let npm = br#"{"name":"demo","version":"1.0.0","scripts":{"test":"vitest"}}"#;
    let cargo = b"[package]\nname='demo'\nversion='0.1.0'";
    tree(
        mock,
        vec![
            entry("package.json", "npm-sha", npm.len()),
            entry("Cargo.toml", "cargo-sha", cargo.len()),
            entry("eslint.config.mjs", "config-sha", 50),
            entry(".npmrc", "npmrc-sha", 20),
            entry(".env", "secret-sha", 20),
        ],
        false,
    )
    .await;
    blob(mock, "npm-sha", npm).await;
    blob(mock, "cargo-sha", cargo).await;
}

#[tokio::test]
async fn inventories_package_manager_files_without_reading_configuration() {
    let mock = MockServer::start().await;
    listing(&mock, json!([repository()])).await;
    let names = [
        ".npmrc",
        ".yarnrc",
        ".yarnrc.yml",
        "npm-shrinkwrap.json",
        "pnpm-workspace.yaml",
    ];
    let mut entries: Vec<_> = names.iter().map(|name| entry(name, "config", 10)).collect();
    let mut symlink = entry("yarn.lock", "symlink", 10);
    symlink["mode"] = json!("120000");
    entries.push(symlink);
    tree(&mock, entries, false).await;
    let overview = service(&mock)
        .get_project_overview("demo")
        .await
        .expect("overview");
    assert_eq!(overview.configuration_files, names);
    assert_eq!(mock.received_requests().await.expect("requests").len(), 2);
}

#[tokio::test]
async fn pnpm_workspace_audit_uses_only_regular_root_filename_evidence() {
    use opensource_maintainer_mcp::model::CheckStatus;
    for (mode, expected) in [
        ("100644", CheckStatus::NotAssessed),
        ("120000", CheckStatus::Finding),
    ] {
        let mock = MockServer::start().await;
        let content = b"{}";
        listing(&mock, json!([repository()])).await;
        let mut workspace = entry("pnpm-workspace.yaml", "config", 10);
        workspace["mode"] = json!(mode);
        tree(
            &mock,
            vec![entry("package.json", "npm-sha", content.len()), workspace],
            false,
        )
        .await;
        blob(&mock, "npm-sha", content).await;
        let report = service(&mock)
            .audit_repository("demo")
            .await
            .expect("audit");
        for id in ["npm.test_script", "npm.lint_script"] {
            let check = report
                .checks
                .iter()
                .find(|check| check.id == id)
                .expect("check");
            assert_eq!(check.status, expected);
            assert_eq!(
                check.evidence.contains(&"pnpm-workspace.yaml".into()),
                mode == "100644"
            );
        }
        assert_eq!(mock.received_requests().await.expect("requests").len(), 3);
    }
}

#[tokio::test]
async fn malformed_workspace_declarations_do_not_infer_member_tooling() {
    use opensource_maintainer_mcp::model::CheckStatus;

    for value in [json!(false), json!(42), json!("packages/*")] {
        let mock = MockServer::start().await;
        let content = serde_json::to_vec(&json!({"workspaces":value})).expect("fixture");
        listing(&mock, json!([repository()])).await;
        tree(
            &mock,
            vec![entry("package.json", "npm-sha", content.len())],
            false,
        )
        .await;
        blob(&mock, "npm-sha", &content).await;

        let report = service(&mock)
            .audit_repository("demo")
            .await
            .expect("audit");
        for (id, expected) in [
            ("manifest.package.json", CheckStatus::Finding),
            ("npm.test_script", CheckStatus::NotAssessed),
            ("npm.lint_script", CheckStatus::NotAssessed),
        ] {
            let check = report
                .checks
                .iter()
                .find(|check| check.id == id)
                .expect("check");
            assert_eq!(check.status, expected);
            assert!(!check.summary.contains("workspace members"));
        }
    }
}

#[tokio::test]
async fn reads_snapshot_manifests_and_lists_configs_without_reading_them() {
    let mock = MockServer::start().await;
    fixture(&mock).await;
    let result = service(&mock)
        .get_project_overview("DEMO")
        .await
        .expect("overview");
    assert_eq!(result.project.full_name, "owner/demo");
    assert_eq!(result.topics, ["rust"]);
    assert_eq!(result.tree_sha, "snapshot");
    assert_eq!(result.configuration_files, [".npmrc", "eslint.config.mjs"]);
    assert_eq!(result.manifests.len(), 2);
    assert_eq!(result.manifests[0].path, "Cargo.toml");
    assert_eq!(result.manifests[0].version.as_deref(), Some("0.1.0"));
    assert_eq!(result.manifests[1].scripts["test"], "vitest");
    let requests = mock.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 4);
    for request in requests
        .iter()
        .filter(|r| r.url.path().starts_with("/repos/"))
    {
        assert!(
            !request.headers.contains_key("authorization"),
            "content requests must be anonymous"
        );
    }
}

#[tokio::test]
async fn invalid_names_make_no_http_requests() {
    let mock = MockServer::start().await;
    for name in [
        "",
        ".",
        "..",
        "../demo",
        "owner/demo",
        "https://github.com/owner/demo",
        "demo?ref=main",
    ] {
        assert!(matches!(
            service(&mock).get_project_overview(name).await,
            Err(Error::InvalidRepositoryName)
        ));
    }
    assert!(mock.received_requests().await.expect("requests").is_empty());
}

#[tokio::test]
async fn rejects_projects_outside_configured_scope_before_content_reads() {
    for mutation in [
        json!({"private":true}),
        json!({"archived":true}),
        json!({"topics":["other"]}),
        json!({"name":"different"}),
    ] {
        let mock = MockServer::start().await;
        let mut repo = repository();
        for (key, value) in mutation.as_object().expect("object") {
            repo[key] = value.clone();
        }
        listing(&mock, json!([repo])).await;
        assert!(matches!(
            service(&mock).get_project_overview("demo").await,
            Err(Error::ProjectNotFound)
        ));
        assert_eq!(mock.received_requests().await.expect("requests").len(), 1);
    }
}

#[tokio::test]
async fn reports_missing_invalid_oversized_and_symlink_manifests() {
    for mode in ["absent", "invalid", "oversized", "symlink"] {
        let mock = MockServer::start().await;
        listing(&mock, json!([repository()])).await;
        let mut item = entry("package.json", "npm-sha", 3);
        if mode == "oversized" {
            item["size"] = json!(262145);
        }
        if mode == "symlink" {
            item["mode"] = json!("120000");
        }
        tree(
            &mock,
            if mode == "absent" { vec![] } else { vec![item] },
            false,
        )
        .await;
        if mode == "invalid" {
            blob(&mock, "npm-sha", b"bad").await;
        }
        let result = service(&mock)
            .get_project_overview("demo")
            .await
            .expect("overview");
        match mode {
            "absent" => assert!(result.manifests.is_empty()),
            "invalid" => assert!(matches!(
                result.manifests[0].status,
                ManifestStatus::Invalid
            )),
            "oversized" => assert!(matches!(
                result.manifests[0].status,
                ManifestStatus::TooLarge
            )),
            _ => assert!(matches!(
                result.manifests[0].status,
                ManifestStatus::UnsupportedFileType
            )),
        }
        assert_eq!(
            mock.received_requests().await.expect("requests").len(),
            if mode == "invalid" { 3 } else { 2 }
        );
    }
}

#[tokio::test]
async fn truncated_tree_is_an_error_not_a_partial_overview() {
    let mock = MockServer::start().await;
    listing(&mock, json!([repository()])).await;
    tree(&mock, vec![], true).await;
    assert!(matches!(
        service(&mock).get_project_overview("demo").await,
        Err(Error::TruncatedTree)
    ));
}

#[tokio::test]
async fn propagates_tree_and_blob_http_failures() {
    for stage in ["tree", "blob"] {
        for status in [401, 403, 404, 409, 429, 500] {
            let mock = MockServer::start().await;
            listing(&mock, json!([repository()])).await;
            let endpoint = if stage == "blob" {
                tree(&mock, vec![entry("package.json", "npm-sha", 3)], false).await;
                "/repos/owner/demo/git/blobs/npm-sha"
            } else {
                "/repos/owner/demo/git/trees/main"
            };
            Mock::given(path(endpoint))
                .respond_with(ResponseTemplate::new(status).set_body_string("SECRET"))
                .expect(1)
                .mount(&mock)
                .await;
            let error = service(&mock)
                .get_project_overview("demo")
                .await
                .expect_err("upstream error");
            assert!(!error.to_string().contains("SECRET"));
            match status {
                401 => assert!(matches!(error, Error::Authentication)),
                429 => assert!(matches!(error, Error::RateLimited { .. })),
                _ => assert!(matches!(error, Error::Http(code) if code == status)),
            }
        }
    }
}

#[tokio::test]
async fn rejects_corrupt_blob_envelopes() {
    for body in [
        json!({"sha":"wrong","encoding":"base64","size":2,"content":"e30="}),
        json!({"sha":"npm-sha","encoding":"utf-8","size":2,"content":"{}"}),
        json!({"sha":"npm-sha","encoding":"base64","size":2,"content":"!invalid!"}),
        json!({"sha":"npm-sha","encoding":"base64","size":99,"content":"e30="}),
    ] {
        let mock = MockServer::start().await;
        listing(&mock, json!([repository()])).await;
        tree(&mock, vec![entry("package.json", "npm-sha", 2)], false).await;
        Mock::given(path("/repos/owner/demo/git/blobs/npm-sha"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&mock)
            .await;
        assert!(matches!(
            service(&mock).get_project_overview("demo").await,
            Err(Error::InvalidResponse)
        ));
    }
}

#[tokio::test]
async fn oversized_api_response_is_bounded() {
    let mock = MockServer::start().await;
    listing(&mock, json!([repository()])).await;
    Mock::given(path("/repos/owner/demo/git/trees/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(2 * 1024 * 1024 + 1)))
        .mount(&mock)
        .await;
    assert!(matches!(
        service(&mock).get_project_overview("demo").await,
        Err(Error::ResponseTooLarge)
    ));
}

#[tokio::test]
async fn mcp_overview_schema_invocation_and_errors() {
    let mock = MockServer::start().await;
    fixture(&mock).await;
    let server = MaintainerServer::new(service(&mock));
    let (server_io, client_io) = tokio::io::duplex(65536);
    let server_task = tokio::spawn(async move {
        server
            .serve(server_io)
            .await
            .expect("server")
            .waiting()
            .await
            .expect("wait");
    });
    let client = ().serve(client_io).await.expect("client");
    let tools = client.list_all_tools().await.expect("tools");
    let tool = tools
        .iter()
        .find(|t| t.name == "get_project_overview")
        .expect("overview tool");
    assert!(tool.output_schema.is_some());
    assert_eq!(tool.input_schema["required"], json!(["repository"]));
    let result = client
        .call_tool(
            CallToolRequestParams::new("get_project_overview").with_arguments(
                json!({"repository":"demo"})
                    .as_object()
                    .expect("args")
                    .clone(),
            ),
        )
        .await
        .expect("invoke");
    assert_ne!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.expect("structured")["manifests"][1]["scripts"]["test"],
        "vitest"
    );
    let error = client
        .call_tool(
            CallToolRequestParams::new("get_project_overview").with_arguments(
                json!({"repository":"../bad"})
                    .as_object()
                    .expect("args")
                    .clone(),
            ),
        )
        .await
        .expect("tool error");
    assert_eq!(error.is_error, Some(true));
    let missing = client
        .call_tool(CallToolRequestParams::new("get_project_overview"))
        .await
        .expect("parameter error result");
    assert_eq!(missing.is_error, Some(true));
    for args in [
        json!({"repository":42}),
        json!({"repository":"demo", "ref":"other"}),
    ] {
        let invalid = client
            .call_tool(
                CallToolRequestParams::new("get_project_overview")
                    .with_arguments(args.as_object().expect("args").clone()),
            )
            .await
            .expect("parameter error result");
        assert_eq!(invalid.is_error, Some(true));
    }
    client.cancel().await.expect("cancel");
    server_task.await.expect("join");
}
