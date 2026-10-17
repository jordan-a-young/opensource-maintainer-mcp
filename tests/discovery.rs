use opensource_maintainer_mcp::{
    config::Config, discovery::Discovery, error::Error, github::GithubClient,
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn repository(index: usize) -> Value {
    json!({"name":format!("repo-{index}"), "full_name":format!("owner/repo-{index}"),
        "description":"Example", "html_url":format!("https://github.com/owner/repo-{index}"),
        "language":"Rust", "archived":false, "default_branch":"main", "topics":["rust"], "private":false})
}
fn discovery(server: &MockServer, archived: bool) -> Discovery {
    let config = Config::parse(&format!(
        "[github]\nowner='owner'\ntopic='rust'\ninclude_archived={archived}"
    ))
    .expect("config");
    Discovery::new(
        config,
        GithubClient::with_endpoint(
            &format!("{}/", server.uri()),
            Some("test-token"),
            Duration::from_secs(2),
        )
        .expect("client"),
    )
    .expect("service")
}
#[tokio::test]
async fn paginates_filters_and_deduplicates() {
    let server = MockServer::start().await;
    let mut first: Vec<_> = (0..100).map(repository).collect();
    first[0]["archived"] = json!(true);
    first[1]["topics"] = json!(["other"]);
    first[2]["private"] = json!(true);
    Mock::given(method("GET"))
        .and(path("/users/owner/repos"))
        .and(query_param("page", "1"))
        .and(query_param("per_page", "100"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(vec![repository(100), repository(99)]),
        )
        .expect(1)
        .mount(&server)
        .await;
    let projects = discovery(&server, false)
        .list_projects()
        .await
        .expect("discovery")
        .projects;
    assert_eq!(projects.len(), 98);
    assert!(projects.iter().any(|p| p.name == "repo-100"));
    assert!(
        !projects
            .iter()
            .any(|p| ["repo-0", "repo-1", "repo-2"].contains(&p.name.as_str()))
    );
}
#[tokio::test]
async fn archived_opt_in_and_empty_results() {
    let server = MockServer::start().await;
    let mut archived = repository(0);
    archived["archived"] = json!(true);
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![archived]))
        .expect(2)
        .mount(&server)
        .await;
    assert_eq!(
        discovery(&server, true)
            .list_projects()
            .await
            .expect("included")
            .projects
            .len(),
        1
    );
    assert!(
        discovery(&server, false)
            .list_projects()
            .await
            .expect("excluded")
            .projects
            .is_empty()
    );
}
#[tokio::test]
async fn maps_http_errors_without_leaking_response_bodies() {
    for status in [401, 403, 404, 429, 500] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status).set_body_string("secret"))
            .expect(1)
            .mount(&server)
            .await;
        let error = discovery(&server, false)
            .list_projects()
            .await
            .expect_err("http error");
        match status {
            401 => assert!(matches!(error, Error::Authentication)),
            429 => assert!(matches!(error, Error::RateLimited { .. })),
            _ => assert!(matches!(error, Error::Http(code) if code == status)),
        }
        assert!(!error.to_string().contains("secret"));
    }
}
#[tokio::test]
async fn handles_primary_and_secondary_rate_limits() {
    for response in [
        ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", "12345"),
        ResponseTemplate::new(403).insert_header("retry-after", "60"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        assert!(matches!(
            discovery(&server, false).list_projects().await,
            Err(Error::RateLimited { .. })
        ));
    }
}
#[tokio::test]
async fn rejects_malformed_responses_and_partial_results() {
    let server = MockServer::start().await;
    Mock::given(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json((0..100).map(repository).collect::<Vec<_>>()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        discovery(&server, false).list_projects().await,
        Err(Error::InvalidResponse)
    ));
}
#[tokio::test]
async fn handles_timeout_and_connection_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(1)))
        .mount(&server)
        .await;
    let client = GithubClient::with_endpoint(
        &format!("{}/", server.uri()),
        None,
        Duration::from_millis(20),
    )
    .expect("client");
    assert!(matches!(
        client.repositories("owner").await,
        Err(Error::Timeout)
    ));
    // A closed port can hit the request timeout before Windows reports refusal.
    // Accept and read the request, then close without an HTTP response instead.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port");
    let addr = listener.local_addr().expect("address");
    let connection = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("connection");
        let mut request = [0; 1024];
        assert!(socket.read(&mut request).await.expect("request") > 0);
    });
    let client =
        GithubClient::with_endpoint(&format!("http://{addr}/"), None, Duration::from_secs(5))
            .expect("client");
    let result = client.repositories("owner").await;
    connection.await.expect("server task");
    assert!(matches!(result, Err(Error::Network)));
}

#[tokio::test]
async fn exact_page_boundary_fetches_empty_final_page() {
    let server = MockServer::start().await;
    Mock::given(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json((0..100).map(repository).collect::<Vec<_>>()),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        discovery(&server, false)
            .list_projects()
            .await
            .expect("complete listing")
            .projects
            .len(),
        100
    );
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let server = MockServer::start().await;
    let destination = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&destination)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", destination.uri()))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        discovery(&server, false).list_projects().await,
        Err(Error::Http(302))
    ));
}
