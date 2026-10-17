//! Explicit live smoke test; never run by the automated test suite.
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::args()
        .nth(1)
        .ok_or("pass the server executable path")?;
    let config = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "examples/config.toml".into());
    let repository = std::env::args().nth(3);
    let mode = std::env::args().nth(4);
    let tool = match mode.as_deref() {
        None => "get_project_overview",
        Some("audit") if repository.is_some() => "audit_repository",
        Some("compare") if repository.is_some() => "compare_standards",
        _ => {
            return Err(
                "optional fourth argument must be 'audit' or 'compare' with repository names"
                    .into(),
            );
        }
    };
    let mut command = tokio::process::Command::new(executable);
    command.arg(config);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let result = async {
        let tools = client.list_all_tools().await?;
        if !tools.iter().any(|t| t.name == "list_projects") {
            return Err("list_projects not advertised".into());
        }
        let request = if let Some(repository) = repository {
            if !tools.iter().any(|t| t.name == tool) {
                return Err(format!("{tool} not advertised").into());
            }
            let (key, value) = if tool == "compare_standards" {
                (
                    "repositories",
                    serde_json::Value::Array(
                        repository
                            .split(',')
                            .map(|name| serde_json::Value::String(name.into()))
                            .collect(),
                    ),
                )
            } else {
                ("repository", serde_json::Value::String(repository))
            };
            CallToolRequestParams::new(tool)
                .with_arguments(serde_json::Map::from_iter([(key.into(), value)]))
        } else {
            CallToolRequestParams::new("list_projects")
        };
        let result = client.call_tool(request).await?;
        if result.is_error == Some(true) {
            return Err(format!("tool failed: {:?}", result.content).into());
        }
        let content = result
            .structured_content
            .ok_or("missing structured output")?;
        println!("{}", serde_json::to_string_pretty(&content)?);
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    client.cancel().await?;
    result
}
