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
    let mut command = tokio::process::Command::new(executable);
    command.arg(config);
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let result = async {
        let tools = client.list_all_tools().await?;
        if !tools.iter().any(|t| t.name == "list_projects") {
            return Err("list_projects not advertised".into());
        }
        let result = client
            .call_tool(CallToolRequestParams::new("list_projects"))
            .await?;
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
