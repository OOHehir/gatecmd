use crate::config::{ArgPattern, CommandConfig};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::Path;

/// Characters that could be used for shell injection.
const FORBIDDEN_CHARS: &[char] = &[';', '|', '&', '$', '`', '(', ')', '{', '}', '>', '<', '\n', '\r', '\\'];

/// Validate and resolve a command invocation against the allowlist.
/// Returns the validated argument list.
/// `file_root` is the canonicalized directory that all file args must reside under.
pub fn validate_command(
    cmd_config: &CommandConfig,
    args_str: &str,
    file_root: &str,
) -> Result<Vec<String>> {
    let args_str = args_str.trim();

    // Check for forbidden characters in the raw input
    for ch in FORBIDDEN_CHARS {
        if args_str.contains(*ch) {
            bail!("Forbidden character '{}' in arguments", ch);
        }
    }

    // Split args by whitespace
    let provided_args: Vec<&str> = if args_str.is_empty() {
        vec![]
    } else {
        args_str.split_whitespace().collect()
    };

    // Try to match against each allowed pattern
    for pattern in &cmd_config.allowed_args {
        if let Some(bindings) = match_pattern(pattern, &provided_args) {
            // Validate file args for path traversal and file_root containment
            for file_param in &pattern.file_args {
                if let Some(value) = bindings.get(file_param) {
                    validate_file_arg(value, file_root)?;
                }
            }
            return Ok(provided_args.iter().map(|s| s.to_string()).collect());
        }
    }

    let allowed: Vec<&str> = cmd_config
        .allowed_args
        .iter()
        .map(|p| p.pattern.as_str())
        .collect();
    bail!(
        "Arguments '{}' do not match any allowed pattern for '{}'. Allowed: {:?}",
        args_str,
        cmd_config.name,
        allowed
    );
}

/// Try to match provided args against a pattern like "wl {offset} {file}".
/// Returns parameter bindings (param_name -> value) if matched, None otherwise.
fn match_pattern(
    pattern: &ArgPattern,
    provided: &[&str],
) -> Option<HashMap<String, String>> {
    let pattern_parts: Vec<&str> = if pattern.pattern.is_empty() {
        vec![]
    } else {
        pattern.pattern.split_whitespace().collect()
    };

    if pattern_parts.len() != provided.len() {
        return None;
    }

    let mut bindings = HashMap::new();

    for (pat, val) in pattern_parts.iter().zip(provided.iter()) {
        if pat.starts_with('{') && pat.ends_with('}') {
            let param_name = &pat[1..pat.len() - 1];
            bindings.insert(param_name.to_string(), val.to_string());
        } else if pat != val {
            return None;
        }
    }

    Some(bindings)
}

/// Validate that a path is safe and under file_root.
/// Public so the file management tools can reuse it.
pub fn validate_path_under_root(path_str: &str, file_root: &str) -> Result<()> {
    validate_file_arg(path_str, file_root)
}

fn validate_file_arg(path_str: &str, file_root: &str) -> Result<()> {
    // Reject path traversal
    if path_str.contains("..") {
        bail!("Path traversal ('..') not allowed in file argument: {}", path_str);
    }

    // Must be an absolute path
    let path = Path::new(path_str);
    if !path.is_absolute() {
        bail!("File argument must be an absolute path: {}", path_str);
    }

    // Must be under file_root — use lexical check since the file may not exist yet
    // (the agent may be about to write it). file_root is already canonicalized at config load.
    if !path_str.starts_with(file_root) ||
       (path_str.len() > file_root.len() && !path_str[file_root.len()..].starts_with('/')) {
        bail!(
            "File argument must be under file_root '{}', got: {}",
            file_root,
            path_str
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ArgPattern, CommandConfig};

    fn test_cmd() -> CommandConfig {
        CommandConfig {
            name: "test".into(),
            binary: "/usr/bin/test".into(),
            description: "test command".into(),
            timeout_secs: 10,
            allowed_args: vec![
                ArgPattern {
                    pattern: "ld".into(),
                    file_args: vec![],
                },
                ArgPattern {
                    pattern: "wl {offset} {file}".into(),
                    file_args: vec!["file".into()],
                },
                ArgPattern {
                    pattern: "".into(),
                    file_args: vec![],
                },
            ],
        }
    }

    const TEST_FILE_ROOT: &str = "/tmp/shared";

    #[test]
    fn test_literal_match() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "ld", TEST_FILE_ROOT);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec!["ld"]);
    }

    #[test]
    fn test_empty_args() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "", TEST_FILE_ROOT);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_param_match() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "wl 0x0000 /tmp/shared/firmware.img", TEST_FILE_ROOT);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec!["wl", "0x0000", "/tmp/shared/firmware.img"]);
    }

    #[test]
    fn test_reject_shell_injection() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "ld; rm -rf /", TEST_FILE_ROOT);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_pipe() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "ld | cat /etc/passwd", TEST_FILE_ROOT);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_path_traversal() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "wl 0x0000 /tmp/shared/../etc/shadow", TEST_FILE_ROOT);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_relative_file_path() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "wl 0x0000 firmware.img", TEST_FILE_ROOT);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_unmatched_pattern() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "delete everything", TEST_FILE_ROOT);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_file_outside_root() {
        let cmd = test_cmd();
        let result = validate_command(&cmd, "wl 0x0000 /etc/passwd", TEST_FILE_ROOT);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file_root"));
    }

    #[test]
    fn test_reject_file_root_prefix_trick() {
        // /tmp/shared_evil should not match /tmp/shared
        let cmd = test_cmd();
        let result = validate_command(&cmd, "wl 0x0000 /tmp/shared_evil/fw.img", TEST_FILE_ROOT);
        assert!(result.is_err());
    }
}
