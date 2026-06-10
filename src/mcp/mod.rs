//! MCP server (stdio transport). Stub for phase 1; full tool/resource
//! implementation lands in the MCP task.

/// Run the MCP server on stdio. Unimplemented in phase 1.
pub async fn run_stdio() -> anyhow::Result<()> {
    anyhow::bail!("mcp::run_stdio is not yet implemented (phase 2)")
}
