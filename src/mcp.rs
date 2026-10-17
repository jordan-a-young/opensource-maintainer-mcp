use crate::{discovery::Discovery, model::Projects};
use rmcp::{
    Json, ServerHandler,
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
};

#[derive(Clone)]
pub struct MaintainerServer {
    discovery: Discovery,
}

#[tool_router]
impl MaintainerServer {
    pub fn new(discovery: Discovery) -> Self {
        Self { discovery }
    }

    #[tool(
        description = "List public GitHub repositories matching the configured owner and topic",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn list_projects(&self) -> Result<Json<Projects>, CallToolResult> {
        self.discovery
            .list_projects()
            .await
            .map(Json)
            .map_err(|error| {
                tracing::warn!(%error, "repository discovery failed");
                CallToolResult::error(vec![ContentBlock::text(error.to_string())])
            })
    }
}

#[tool_handler]
impl ServerHandler for MaintainerServer {}
