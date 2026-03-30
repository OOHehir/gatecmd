use std::sync::Arc;
use std::time::Duration;

use rmcp::{
    ErrorData as McpError,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    schemars, tool, tool_handler, tool_router,
    ServerHandler,
};
use tokio::process::Command;

use crate::config::Config;
use crate::sanitizer::validate_command;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExecuteCommandArgs {
    /// Name of the command to execute (must be in allowlist)
    pub command: String,
    /// Arguments string (must match an allowed pattern)
    #[serde(default)]
    pub args: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListCommandsArgs {}

#[derive(Clone)]
pub struct HostCmdServer {
    config: Arc<Config>,
    tool_router: ToolRouter<HostCmdServer>,
}

#[tool_router]
impl HostCmdServer {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            tool_router: Self::tool_router(),
        }
    }

    /// List all available commands and their allowed argument patterns.
    #[tool(description = "List all available host commands and their allowed argument patterns")]
    async fn list_commands(
        &self,
        Parameters(_args): Parameters<ListCommandsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mut output = String::new();
        for cmd in &self.config.commands {
            output.push_str(&format!("## {}\n", cmd.name));
            output.push_str(&format!("Binary: {}\n", cmd.binary));
            output.push_str(&format!("Description: {}\n", cmd.description));
            output.push_str(&format!("Timeout: {}s\n", cmd.timeout_secs));
            output.push_str("Allowed argument patterns:\n");
            for pat in &cmd.allowed_args {
                if pat.pattern.is_empty() {
                    output.push_str("  - (no arguments)\n");
                } else {
                    output.push_str(&format!("  - {}\n", pat.pattern));
                }
            }
            output.push('\n');
        }
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Execute an allowed command on the host with validated arguments.
    #[tool(description = "Execute a whitelisted command on the host machine. Use list_commands first to see available commands and allowed argument patterns.")]
    async fn execute_command(
        &self,
        Parameters(args): Parameters<ExecuteCommandArgs>,
    ) -> Result<CallToolResult, McpError> {
        let cmd_config = self.config.find_command(&args.command).ok_or_else(|| {
            McpError::invalid_params(
                format!("Unknown command '{}'. Use list_commands to see available commands.", args.command),
                None,
            )
        })?;

        let validated_args = validate_command(cmd_config, &args.args).map_err(|e| {
            McpError::invalid_params(format!("Argument validation failed: {e}"), None)
        })?;

        tracing::info!(
            command = %cmd_config.name,
            binary = %cmd_config.binary,
            args = ?validated_args,
            "Executing command"
        );

        let timeout = Duration::from_secs(cmd_config.timeout_secs);

        let result = tokio::time::timeout(timeout, async {
            Command::new(&cmd_config.binary)
                .args(&validated_args)
                .output()
                .await
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let mut response = format!("Exit code: {exit_code}\n");
                if !stdout.is_empty() {
                    response.push_str(&format!("\n--- stdout ---\n{stdout}"));
                }
                if !stderr.is_empty() {
                    response.push_str(&format!("\n--- stderr ---\n{stderr}"));
                }

                if output.status.success() {
                    Ok(CallToolResult::success(vec![Content::text(response)]))
                } else {
                    Ok(CallToolResult::error(vec![Content::text(response)]))
                }
            }
            Ok(Err(e)) => {
                tracing::error!(error = %e, "Failed to execute command");
                Ok(CallToolResult::error(vec![Content::text(
                    format!("Failed to execute: {e}"),
                )]))
            }
            Err(_) => {
                tracing::warn!("Command timed out after {}s", cmd_config.timeout_secs);
                Ok(CallToolResult::error(vec![Content::text(
                    format!("Command timed out after {}s", cmd_config.timeout_secs),
                )]))
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for HostCmdServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("gatecmd", "0.1.0"))
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "MCP server for executing whitelisted commands on the host machine. \
                 Use list_commands to see available commands, then execute_command to run them."
                    .to_string(),
            )
    }
}
