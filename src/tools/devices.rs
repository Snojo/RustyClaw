//! Device tools: nodes and canvas.
//!
//! The nodes tool provides remote device control via:
//! - SSH: For Linux/macOS/Unix remote machines
//! - ADB: For Android devices
//!
//! Canvas remains a stub (requires OpenClaw canvas service).

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

/// Discover and control paired nodes via SSH or ADB.
///
/// Supports two transport types:
/// - `ssh`: Remote machines via SSH (requires ssh CLI)
/// - `adb`: Android devices via ADB (requires adb CLI)
///
/// Node identifiers:
/// - SSH: `user@host` or `ssh:user@host:port`
/// - ADB: `adb:device_id` or just device serial
pub fn exec_nodes(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: action".to_string())?;

    match action {
        "status" => node_status(),
        "describe" => {
            let node = get_node(args)?;
            node_describe(&node)
        }
        "run" => {
            let node = get_node(args)?;
            let command = get_command_array(args)?;
            node_run(&node, &command)
        }
        "camera_snap" => {
            let node = get_node(args)?;
            let facing = args.get("facing").and_then(|v| v.as_str()).unwrap_or("back");
            adb_camera_snap(&node, facing)
        }
        "camera_list" => {
            let node = get_node(args)?;
            adb_camera_list(&node)
        }
        "screen_record" => {
            let node = get_node(args)?;
            let duration = args.get("durationMs").and_then(|v| v.as_u64()).unwrap_or(5000);
            adb_screen_record(&node, duration)
        }
        "location_get" => {
            let node = get_node(args)?;
            adb_location_get(&node)
        }
        "notify" => {
            let node = get_node(args)?;
            let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("RustyClaw");
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
            adb_notify(&node, title, body)
        }
        // Pairing actions - not applicable for SSH/ADB model
        "pending" => Ok(json!({
            "pending": [],
            "note": "SSH/ADB nodes don't require pairing. Use 'status' to list available devices."
        }).to_string()),
        "approve" | "reject" => Ok("SSH/ADB nodes don't require pairing approval.".to_string()),
        "invoke" => {
            // Map invoke to run for compatibility
            let node = get_node(args)?;
            let cmd = args.get("invokeCommand").and_then(|v| v.as_str())
                .ok_or("Missing 'invokeCommand'")?;
            node_run(&node, &[cmd])
        }
        _ => Err(format!(
            "Unknown action: {}. Valid: status, describe, run, camera_snap, camera_list, screen_record, location_get, notify, invoke",
            action
        )),
    }
}

/// Extract node identifier from args.
fn get_node(args: &Value) -> Result<String, String> {
    args.get("node")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Missing 'node' parameter".to_string())
}

/// Extract command array from args.
fn get_command_array(args: &Value) -> Result<Vec<String>, String> {
    let command = args
        .get("command")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
        .unwrap_or_default();

    if command.is_empty() {
        return Err("Missing 'command' array for run action".to_string());
    }
    Ok(command)
}

/// Determine node type from identifier.
enum NodeType {
    Ssh { user: String, host: String, port: u16 },
    Adb { device: String },
}

fn parse_node(node: &str) -> NodeType {
    // Check for explicit prefix
    if node.starts_with("adb:") {
        return NodeType::Adb { device: node[4..].to_string() };
    }
    if node.starts_with("ssh:") {
        let rest = &node[4..];
        return parse_ssh_target(rest);
    }
    
    // Heuristic: if it contains '@', treat as SSH
    if node.contains('@') {
        return parse_ssh_target(node);
    }
    
    // Default to ADB for device-serial-like strings
    NodeType::Adb { device: node.to_string() }
}

fn parse_ssh_target(target: &str) -> NodeType {
    // Parse user@host:port or user@host
    let (user_host, port) = if let Some(idx) = target.rfind(':') {
        if let Ok(p) = target[idx+1..].parse::<u16>() {
            (&target[..idx], p)
        } else {
            (target, 22)
        }
    } else {
        (target, 22)
    };

    let (user, host) = if let Some(idx) = user_host.find('@') {
        (user_host[..idx].to_string(), user_host[idx+1..].to_string())
    } else {
        ("root".to_string(), user_host.to_string())
    };

    NodeType::Ssh { user, host, port }
}

/// List available nodes (SSH hosts from config + ADB devices).
fn node_status() -> Result<String, String> {
    let mut nodes = Vec::new();

    // Check for ADB devices
    if let Ok(output) = Command::new("adb").args(["devices", "-l"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == "device" {
                    let device_id = parts[0];
                    let model = parts.iter()
                        .find(|p| p.starts_with("model:"))
                        .map(|p| &p[6..])
                        .unwrap_or("unknown");
                    nodes.push(json!({
                        "id": format!("adb:{}", device_id),
                        "type": "adb",
                        "device": device_id,
                        "model": model,
                        "status": "connected"
                    }));
                }
            }
        }
    }

    // Check SSH config for known hosts
    if let Ok(config) = std::fs::read_to_string(dirs::home_dir().unwrap_or_default().join(".ssh/config")) {
        let mut current_host: Option<String> = None;
        let mut current_user: Option<String> = None;
        let mut current_hostname: Option<String> = None;

        for line in config.lines() {
            let line = line.trim();
            if line.starts_with("Host ") && !line.contains('*') {
                // Save previous host if any
                if let Some(host) = current_host.take() {
                    let user = current_user.take().unwrap_or_else(|| "root".to_string());
                    let hostname = current_hostname.take().unwrap_or_else(|| host.clone());
                    nodes.push(json!({
                        "id": format!("ssh:{}@{}", user, hostname),
                        "type": "ssh",
                        "alias": host,
                        "user": user,
                        "host": hostname,
                        "status": "configured"
                    }));
                }
                current_host = Some(line[5..].trim().to_string());
            } else if line.to_lowercase().starts_with("user ") {
                current_user = Some(line[5..].trim().to_string());
            } else if line.to_lowercase().starts_with("hostname ") {
                current_hostname = Some(line[9..].trim().to_string());
            }
        }

        // Don't forget last host
        if let Some(host) = current_host {
            let user = current_user.unwrap_or_else(|| "root".to_string());
            let hostname = current_hostname.unwrap_or_else(|| host.clone());
            nodes.push(json!({
                "id": format!("ssh:{}@{}", user, hostname),
                "type": "ssh",
                "alias": host,
                "user": user,
                "host": hostname,
                "status": "configured"
            }));
        }
    }

    Ok(json!({
        "nodes": nodes,
        "adb_available": which::which("adb").is_ok(),
        "ssh_available": which::which("ssh").is_ok(),
        "note": "Use node='ssh:user@host' for SSH or node='adb:device_id' for Android"
    }).to_string())
}

/// Get details about a specific node.
fn node_describe(node: &str) -> Result<String, String> {
    match parse_node(node) {
        NodeType::Ssh { user, host, port } => {
            // Try to get system info via SSH
            let output = Command::new("ssh")
                .args([
                    "-o", "ConnectTimeout=5",
                    "-o", "BatchMode=yes",
                    "-p", &port.to_string(),
                    &format!("{}@{}", user, host),
                    "uname -a && hostname && uptime"
                ])
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    let info = String::from_utf8_lossy(&out.stdout);
                    Ok(json!({
                        "node": node,
                        "type": "ssh",
                        "user": user,
                        "host": host,
                        "port": port,
                        "status": "reachable",
                        "info": info.trim()
                    }).to_string())
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    Ok(json!({
                        "node": node,
                        "type": "ssh",
                        "user": user,
                        "host": host,
                        "port": port,
                        "status": "unreachable",
                        "error": err.trim()
                    }).to_string())
                }
                Err(e) => Err(format!("Failed to run ssh: {}", e))
            }
        }
        NodeType::Adb { device } => {
            // Get device properties
            let output = Command::new("adb")
                .args(["-s", &device, "shell", "getprop ro.product.model && getprop ro.build.version.release && getprop ro.serialno"])
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    let info = String::from_utf8_lossy(&out.stdout);
                    let lines: Vec<&str> = info.lines().collect();
                    Ok(json!({
                        "node": node,
                        "type": "adb",
                        "device": device,
                        "status": "connected",
                        "model": lines.get(0).unwrap_or(&""),
                        "android_version": lines.get(1).unwrap_or(&""),
                        "serial": lines.get(2).unwrap_or(&"")
                    }).to_string())
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    Ok(json!({
                        "node": node,
                        "type": "adb",
                        "device": device,
                        "status": "error",
                        "error": err.trim()
                    }).to_string())
                }
                Err(e) => Err(format!("Failed to run adb: {}", e))
            }
        }
    }
}

/// Run a command on a remote node.
fn node_run(node: &str, command: &[impl AsRef<str>]) -> Result<String, String> {
    match parse_node(node) {
        NodeType::Ssh { user, host, port } => {
            let cmd_str = command.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" ");
            
            let output = Command::new("ssh")
                .args([
                    "-o", "ConnectTimeout=10",
                    "-p", &port.to_string(),
                    &format!("{}@{}", user, host),
                    &cmd_str
                ])
                .output()
                .map_err(|e| format!("Failed to run ssh: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            Ok(json!({
                "node": node,
                "command": cmd_str,
                "exit_code": output.status.code(),
                "stdout": stdout.trim(),
                "stderr": stderr.trim()
            }).to_string())
        }
        NodeType::Adb { device } => {
            let cmd_str = command.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" ");
            
            let output = Command::new("adb")
                .args(["-s", &device, "shell", &cmd_str])
                .output()
                .map_err(|e| format!("Failed to run adb: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            Ok(json!({
                "node": node,
                "command": cmd_str,
                "exit_code": output.status.code(),
                "stdout": stdout.trim(),
                "stderr": stderr.trim()
            }).to_string())
        }
    }
}

// ── ADB-specific actions ────────────────────────────────────────────────────

/// Take a screenshot on Android device.
fn adb_camera_snap(node: &str, facing: &str) -> Result<String, String> {
    let device = match parse_node(node) {
        NodeType::Adb { device } => device,
        NodeType::Ssh { .. } => return Err("camera_snap only works with ADB nodes".to_string()),
    };

    // For now, we can take a screenshot (actual camera requires an app)
    // Real camera access would need: am start camera intent, then screencap
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let remote_path = format!("/sdcard/rustyclaw_snap_{}.png", timestamp);
    
    // Take screenshot
    let output = Command::new("adb")
        .args(["-s", &device, "shell", "screencap", "-p", &remote_path])
        .output()
        .map_err(|e| format!("Failed to run adb: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Screenshot failed: {}", err));
    }

    // Pull file
    let local_path = format!("/tmp/adb_snap_{}.png", timestamp);
    let pull = Command::new("adb")
        .args(["-s", &device, "pull", &remote_path, &local_path])
        .output()
        .map_err(|e| format!("Failed to pull file: {}", e))?;

    if !pull.status.success() {
        return Err("Failed to pull screenshot from device".to_string());
    }

    // Clean up remote file
    let _ = Command::new("adb")
        .args(["-s", &device, "shell", "rm", &remote_path])
        .output();

    Ok(json!({
        "node": node,
        "action": "camera_snap",
        "facing": facing,
        "note": "Took screenshot (camera access requires app). For actual camera, use screen_record while camera app is open.",
        "path": local_path
    }).to_string())
}

/// List cameras on Android device.
fn adb_camera_list(node: &str) -> Result<String, String> {
    let device = match parse_node(node) {
        NodeType::Adb { device } => device,
        NodeType::Ssh { .. } => return Err("camera_list only works with ADB nodes".to_string()),
    };

    // Query camera info via dumpsys
    let output = Command::new("adb")
        .args(["-s", &device, "shell", "dumpsys media.camera | grep -E 'Camera|Facing'"])
        .output()
        .map_err(|e| format!("Failed to run adb: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    Ok(json!({
        "node": node,
        "cameras": stdout.trim(),
        "note": "Use camera app + screen_record for actual camera capture"
    }).to_string())
}

/// Record screen on Android device.
fn adb_screen_record(node: &str, duration_ms: u64) -> Result<String, String> {
    let device = match parse_node(node) {
        NodeType::Adb { device } => device,
        NodeType::Ssh { .. } => return Err("screen_record only works with ADB nodes".to_string()),
    };

    let duration_secs = (duration_ms / 1000).max(1).min(180); // 1-180 seconds
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let remote_path = format!("/sdcard/rustyclaw_rec_{}.mp4", timestamp);
    let local_path = format!("/tmp/adb_rec_{}.mp4", timestamp);

    // Start recording (this blocks for duration)
    let output = Command::new("adb")
        .args([
            "-s", &device,
            "shell",
            "screenrecord",
            "--time-limit", &duration_secs.to_string(),
            &remote_path
        ])
        .output()
        .map_err(|e| format!("Failed to run adb: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Screen record failed: {}", err));
    }

    // Pull file
    let pull = Command::new("adb")
        .args(["-s", &device, "pull", &remote_path, &local_path])
        .output()
        .map_err(|e| format!("Failed to pull file: {}", e))?;

    if !pull.status.success() {
        return Err("Failed to pull recording from device".to_string());
    }

    // Clean up
    let _ = Command::new("adb")
        .args(["-s", &device, "shell", "rm", &remote_path])
        .output();

    Ok(json!({
        "node": node,
        "action": "screen_record",
        "duration_secs": duration_secs,
        "path": local_path
    }).to_string())
}

/// Get location from Android device.
fn adb_location_get(node: &str) -> Result<String, String> {
    let device = match parse_node(node) {
        NodeType::Adb { device } => device,
        NodeType::Ssh { .. } => return Err("location_get only works with ADB nodes".to_string()),
    };

    // Try to get last known location from location providers
    let output = Command::new("adb")
        .args([
            "-s", &device,
            "shell",
            "dumpsys location | grep -A2 'last location'"
        ])
        .output()
        .map_err(|e| format!("Failed to run adb: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Also try settings secure for mock location
    let mock_output = Command::new("adb")
        .args(["-s", &device, "shell", "settings get secure mock_location"])
        .output()
        .ok();

    let mock_enabled = mock_output
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
        .unwrap_or(false);

    Ok(json!({
        "node": node,
        "location_info": stdout.trim(),
        "mock_location": mock_enabled,
        "note": "For real-time location, use a location app or enable developer options"
    }).to_string())
}

/// Send notification to Android device.
fn adb_notify(node: &str, title: &str, body: &str) -> Result<String, String> {
    let device = match parse_node(node) {
        NodeType::Adb { device } => device,
        NodeType::Ssh { .. } => {
            // For SSH, we can try notify-send on Linux
            if let NodeType::Ssh { user, host, port } = parse_node(node) {
                let output = Command::new("ssh")
                    .args([
                        "-o", "ConnectTimeout=5",
                        "-p", &port.to_string(),
                        &format!("{}@{}", user, host),
                        &format!("notify-send '{}' '{}'", title, body)
                    ])
                    .output();

                return match output {
                    Ok(out) if out.status.success() => Ok(json!({
                        "node": node,
                        "action": "notify",
                        "title": title,
                        "body": body,
                        "status": "sent"
                    }).to_string()),
                    _ => Ok(json!({
                        "node": node,
                        "action": "notify",
                        "status": "failed",
                        "note": "notify-send may not be available on target"
                    }).to_string())
                };
            }
            return Err("Unexpected node type".to_string());
        }
    };

    // Use am broadcast to show a toast (simple notification)
    // For proper notifications, would need an app
    let output = Command::new("adb")
        .args([
            "-s", &device,
            "shell",
            &format!(
                "am broadcast -a android.intent.action.MAIN -e message '{}' -n com.android.settings/.notification.RedactNotificationSettings || echo '{}: {}'",
                body, title, body
            )
        ])
        .output()
        .map_err(|e| format!("Failed to run adb: {}", e))?;

    // Fallback: show toast via input/settings
    let toast = Command::new("adb")
        .args([
            "-s", &device,
            "shell",
            &format!("cmd notification post -t '{}' 'RustyClaw' '{}'", title, body)
        ])
        .output();

    let status = if toast.map(|o| o.status.success()).unwrap_or(false) {
        "sent"
    } else {
        "attempted"
    };

    Ok(json!({
        "node": node,
        "action": "notify",
        "title": title,
        "body": body,
        "status": status,
        "note": "Notification sent via cmd notification (Android 10+)"
    }).to_string())
}

// ── Canvas (stub) ───────────────────────────────────────────────────────────

/// Canvas control for UI presentation.
/// This remains a stub as it requires OpenClaw canvas service infrastructure.
pub fn exec_canvas(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: action".to_string())?;

    let node = args.get("node").and_then(|v| v.as_str());

    match action {
        "present" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'url' for present action")?;
            let width = args.get("width").and_then(|v| v.as_u64()).unwrap_or(800);
            let height = args.get("height").and_then(|v| v.as_u64()).unwrap_or(600);

            Ok(format!(
                "Would present canvas:\n- URL: {}\n- Size: {}x{}\n- Node: {}\n\nNote: Requires canvas integration.",
                url,
                width,
                height,
                node.unwrap_or("default")
            ))
        }

        "hide" => Ok(format!(
            "Would hide canvas on node: {}\n\nNote: Requires canvas integration.",
            node.unwrap_or("default")
        )),

        "navigate" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'url' for navigate action")?;
            Ok(format!(
                "Would navigate canvas to: {}\n\nNote: Requires canvas integration.",
                url
            ))
        }

        "eval" => {
            let js = args
                .get("javaScript")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'javaScript' for eval action")?;
            Ok(format!(
                "Would evaluate JavaScript ({} chars):\n{}\n\nNote: Requires canvas integration.",
                js.len(),
                if js.len() > 100 { &js[..100] } else { js }
            ))
        }

        "snapshot" => Ok(format!(
            "Would capture canvas snapshot on node: {}\n\nNote: Requires canvas integration.",
            node.unwrap_or("default")
        )),

        "a2ui_push" => Ok(
            "Would push A2UI (accessibility-to-UI) update.\n\nNote: Requires canvas integration."
                .to_string(),
        ),

        "a2ui_reset" => {
            Ok("Would reset A2UI state.\n\nNote: Requires canvas integration.".to_string())
        }

        _ => Err(format!(
            "Unknown action: {}. Valid: present, hide, navigate, eval, snapshot, a2ui_push, a2ui_reset",
            action
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_ssh_node() {
        match parse_node("user@example.com") {
            NodeType::Ssh { user, host, port } => {
                assert_eq!(user, "user");
                assert_eq!(host, "example.com");
                assert_eq!(port, 22);
            }
            _ => panic!("Expected SSH node"),
        }
    }

    #[test]
    fn test_parse_ssh_with_port() {
        match parse_node("ssh:admin@192.168.1.1:2222") {
            NodeType::Ssh { user, host, port } => {
                assert_eq!(user, "admin");
                assert_eq!(host, "192.168.1.1");
                assert_eq!(port, 2222);
            }
            _ => panic!("Expected SSH node"),
        }
    }

    #[test]
    fn test_parse_adb_node() {
        match parse_node("adb:emulator-5554") {
            NodeType::Adb { device } => {
                assert_eq!(device, "emulator-5554");
            }
            _ => panic!("Expected ADB node"),
        }
    }

    #[test]
    fn test_nodes_status() {
        let args = json!({ "action": "status" });
        let result = exec_nodes(&args, &PathBuf::from("/tmp"));
        assert!(result.is_ok());
    }
}
