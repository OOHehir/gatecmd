use std::path::Path;
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
use crate::sanitizer::{validate_command, validate_path_under_root};

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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListFilesArgs {
    /// Path relative to the shared file root (e.g. "" for root, "images" for a subdirectory)
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CopyFileArgs {
    /// Source path relative to the shared file root
    pub src: String,
    /// Destination path relative to the shared file root
    pub dst: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RemoveFileArgs {
    /// Path relative to the shared file root
    pub path: String,
}

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
        output.push_str(&format!("File root: {}\n", self.config.file_root));
        output.push_str("All file path arguments must be under this directory.\n\n");
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

        let validated_args = validate_command(cmd_config, &args.args, &self.config.file_root).map_err(|e| {
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

    /// Resolve a relative path to an absolute path under file_root, validating safety.
    fn resolve_path(&self, relative: &str) -> Result<std::path::PathBuf, McpError> {
        let file_root = &self.config.file_root;
        let abs = if relative.is_empty() {
            file_root.to_string()
        } else {
            format!("{}/{}", file_root, relative)
        };

        // Block traversal and validate containment
        validate_path_under_root(&abs, file_root).map_err(|e| {
            McpError::invalid_params(format!("Path validation failed: {e}"), None)
        })?;

        Ok(Path::new(&abs).to_path_buf())
    }

    /// List files and directories in the shared file root.
    #[tool(description = "List files in the shared file root directory. Pass a relative path to list a subdirectory, or leave empty for the root.")]
    async fn list_files(
        &self,
        Parameters(args): Parameters<ListFilesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let dir = self.resolve_path(&args.path)?;

        tracing::info!(path = %dir.display(), "Listing files");

        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    format!("Failed to read directory '{}': {e}", dir.display()),
                )]));
            }
        };

        let mut output = String::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let suffix = match entry.file_type().await {
                Ok(ft) if ft.is_dir() => "/",
                _ => "",
            };
            if let Ok(meta) = entry.metadata().await {
                output.push_str(&format!("{:>10}  {}{}\n", meta.len(), name, suffix));
            } else {
                output.push_str(&format!("{:>10}  {}{}\n", "?", name, suffix));
            }
        }

        if output.is_empty() {
            output.push_str("(empty directory)\n");
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Copy a file within the shared file root.
    #[tool(description = "Copy a file within the shared file root. Both src and dst are relative paths.")]
    async fn copy_file(
        &self,
        Parameters(args): Parameters<CopyFileArgs>,
    ) -> Result<CallToolResult, McpError> {
        let src = self.resolve_path(&args.src)?;
        let dst = self.resolve_path(&args.dst)?;

        tracing::info!(src = %src.display(), dst = %dst.display(), "Copying file");

        // Create parent directories if needed
        if let Some(parent) = dst.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(CallToolResult::error(vec![Content::text(
                    format!("Failed to create directory '{}': {e}", parent.display()),
                )]));
            }
        }

        match tokio::fs::copy(&src, &dst).await {
            Ok(bytes) => Ok(CallToolResult::success(vec![Content::text(
                format!("Copied {} bytes: {} -> {}", bytes, src.display(), dst.display()),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(
                format!("Failed to copy '{}' -> '{}': {e}", src.display(), dst.display()),
            )])),
        }
    }

    /// Remove a file from the shared file root.
    #[tool(description = "Remove a file from the shared file root. Path is relative to the file root. Cannot remove directories.")]
    async fn remove_file(
        &self,
        Parameters(args): Parameters<RemoveFileArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = self.resolve_path(&args.path)?;

        // Refuse to delete the file_root itself
        if path == Path::new(&self.config.file_root) {
            return Ok(CallToolResult::error(vec![Content::text(
                "Cannot remove the file root directory itself".to_string(),
            )]));
        }

        tracing::info!(path = %path.display(), "Removing file");

        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(
                format!("Removed: {}", path.display()),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(
                format!("Failed to remove '{}': {e}", path.display()),
            )])),
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
