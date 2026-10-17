use opensource_maintainer_mcp::{
    config::Config, discovery::Discovery, github::GithubClient, mcp::MaintainerServer,
};
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_ansi(false)
        .init();
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "server stopped");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().unwrap_or_else(|| "config.toml".into());
    if args.next().is_some() {
        return Err("usage: opensource-maintainer-mcp [config.toml]".into());
    }
    let input = std::fs::read_to_string(path)
        .map_err(|_| "cannot read configuration file; pass its path as the only argument")?;
    let config = Config::parse(&input)?;
    let token = match std::env::var("GITHUB_TOKEN") {
        Ok(token) => Some(token),
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => return Err("GITHUB_TOKEN must be valid Unicode".into()),
    };
    let discovery = Discovery::new(config, GithubClient::new(token.as_deref())?)?;
    tracing::info!("starting MCP server over stdio");
    MaintainerServer::new(discovery)
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
