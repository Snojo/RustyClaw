//! Device tools: nodes, browser, canvas.

use serde_json::Value;
use std::path::Path;

/// Discover and control paired nodes.
pub fn exec_nodes(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: action".to_string())?;

    let node = args.get("node").and_then(|v| v.as_str());

    match action {
        "status" => {
            Ok("Node status:\n\nNo nodes currently paired.\n\nTo pair a node:\n1. Run `openclaw node run` on the target device\n2. Use `nodes` with action='pending' to see pairing requests\n3. Use `nodes` with action='approve' to approve".to_string())
        }

        "describe" => {
            let node_id = node.ok_or("Missing 'node' parameter for describe action")?;
            Ok(format!(
                "Node description requested for: {}\n\nNote: Requires gateway integration to fetch node details.",
                node_id
            ))
        }

        "pending" => {
            Ok("Pending pairing requests:\n\nNo pending requests.\n\nNote: Requires gateway integration.".to_string())
        }

        "approve" => {
            let request_id = args
                .get("requestId")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'requestId' for approve action")?;
            Ok(format!(
                "Would approve pairing request: {}\n\nNote: Requires gateway integration.",
                request_id
            ))
        }

        "reject" => {
            let request_id = args
                .get("requestId")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'requestId' for reject action")?;
            Ok(format!(
                "Would reject pairing request: {}\n\nNote: Requires gateway integration.",
                request_id
            ))
        }

        "notify" => {
            let node_id = node.ok_or("Missing 'node' parameter for notify action")?;
            let title = args
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Notification");
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");

            Ok(format!(
                "Notification queued:\n- Node: {}\n- Title: {}\n- Body: {}\n\nNote: Requires node connection.",
                node_id, title, body
            ))
        }

        "camera_snap" => {
            let node_id = node.ok_or("Missing 'node' parameter for camera_snap")?;
            let facing = args
                .get("facing")
                .and_then(|v| v.as_str())
                .unwrap_or("back");

            Ok(format!(
                "Camera snapshot requested:\n- Node: {}\n- Facing: {}\n\nNote: Requires paired node with camera access.",
                node_id, facing
            ))
        }

        "camera_list" => {
            let node_id = node.ok_or("Missing 'node' parameter for camera_list")?;
            Ok(format!(
                "Camera list requested for node: {}\n\nNote: Requires paired node.",
                node_id
            ))
        }

        "screen_record" => {
            let node_id = node.ok_or("Missing 'node' parameter for screen_record")?;
            Ok(format!(
                "Screen recording requested for node: {}\n\nNote: Requires paired node with screen recording permission.",
                node_id
            ))
        }

        "location_get" => {
            let node_id = node.ok_or("Missing 'node' parameter for location_get")?;
            Ok(format!(
                "Location requested for node: {}\n\nNote: Requires paired node with location permission.",
                node_id
            ))
        }

        "run" => {
            let node_id = node.ok_or("Missing 'node' parameter for run action")?;
            let command = args
                .get("command")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();

            if command.is_empty() {
                return Err("Missing 'command' array for run action".to_string());
            }

            Ok(format!(
                "Remote command requested:\n- Node: {}\n- Command: {}\n\nNote: Requires paired node host.",
                node_id,
                command.join(" ")
            ))
        }

        "invoke" => {
            let node_id = node.ok_or("Missing 'node' parameter for invoke action")?;
            let invoke_cmd = args
                .get("invokeCommand")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'invokeCommand' for invoke action")?;

            Ok(format!(
                "Node invoke requested:\n- Node: {}\n- Command: {}\n\nNote: Requires paired node.",
                node_id, invoke_cmd
            ))
        }

        _ => Err(format!(
            "Unknown action: {}. Valid: status, describe, pending, approve, reject, notify, camera_snap, camera_list, screen_record, location_get, run, invoke",
            action
        )),
    }
}

/// Browser automation control.
pub fn exec_browser(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: action".to_string())?;

    let profile = args
        .get("profile")
        .and_then(|v| v.as_str())
        .unwrap_or("openclaw");

    match action {
        "status" => Ok(format!(
            "Browser status:\n- Profile: {}\n- Status: Not running\n\nNote: Browser control requires Playwright/CDP integration.",
            profile
        )),

        "start" => Ok(format!(
            "Would start browser with profile: {}\n\nNote: Requires Playwright/CDP integration.",
            profile
        )),

        "stop" => Ok(format!(
            "Would stop browser profile: {}\n\nNote: Requires Playwright/CDP integration.",
            profile
        )),

        "profiles" => Ok(
            "Available browser profiles:\n- openclaw (managed, isolated)\n- chrome (extension relay)\n\nNote: Requires browser integration.".to_string(),
        ),

        "tabs" => Ok(format!(
            "Would list tabs for profile: {}\n\nNote: Requires browser integration.",
            profile
        )),

        "open" => {
            let url = args
                .get("targetUrl")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'targetUrl' for open action")?;
            Ok(format!(
                "Would open URL: {}\n- Profile: {}\n\nNote: Requires browser integration.",
                url, profile
            ))
        }

        "focus" | "close" => {
            let tab_id = args
                .get("targetId")
                .and_then(|v| v.as_str())
                .ok_or(format!("Missing 'targetId' for {} action", action))?;
            Ok(format!(
                "Would {} tab: {}\n\nNote: Requires browser integration.",
                action, tab_id
            ))
        }

        "snapshot" => Ok(format!(
            "Would capture accessibility snapshot for profile: {}\n\nReturns ARIA tree with element refs for targeting.\nNote: Requires browser integration.",
            profile
        )),

        "screenshot" => {
            let full_page = args
                .get("fullPage")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok(format!(
                "Would capture screenshot:\n- Profile: {}\n- Full page: {}\n\nNote: Requires browser integration.",
                profile, full_page
            ))
        }

        "navigate" => {
            let url = args
                .get("targetUrl")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'targetUrl' for navigate action")?;
            Ok(format!(
                "Would navigate to: {}\n\nNote: Requires browser integration.",
                url
            ))
        }

        "console" => {
            Ok("Would fetch browser console logs.\n\nNote: Requires browser integration.".to_string())
        }

        "pdf" => {
            Ok("Would generate PDF from current page.\n\nNote: Requires browser integration.".to_string())
        }

        "act" => {
            let request = args.get("request");
            if let Some(req) = request {
                let kind = req
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let element_ref = req.get("ref").and_then(|v| v.as_str()).unwrap_or("none");
                Ok(format!(
                    "Would perform action:\n- Kind: {}\n- Element ref: {}\n\nNote: Requires browser integration.",
                    kind, element_ref
                ))
            } else {
                Err("Missing 'request' object for act action".to_string())
            }
        }

        _ => Err(format!(
            "Unknown action: {}. Valid: status, start, stop, profiles, tabs, open, focus, close, snapshot, screenshot, navigate, console, pdf, act",
            action
        )),
    }
}

/// Canvas control for UI presentation.
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
