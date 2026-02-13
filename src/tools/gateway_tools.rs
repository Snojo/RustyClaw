//! Gateway tools: gateway, message, tts.

use super::helpers::resolve_path;
use serde_json::Value;
use std::path::Path;

/// Gateway management.
pub fn exec_gateway(args: &Value, workspace_dir: &Path) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: action".to_string())?;

    let config_path = workspace_dir
        .parent()
        .unwrap_or(workspace_dir)
        .join("openclaw.json");

    match action {
        "restart" => {
            let reason = args
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("Restart requested via gateway tool");

            Ok(format!(
                "Gateway restart requested.\nReason: {}\nNote: Actual restart requires daemon integration.",
                reason
            ))
        }

        "config.get" => {
            if !config_path.exists() {
                return Ok(serde_json::json!({
                    "config": {},
                    "hash": "",
                    "exists": false
                })
                .to_string());
            }

            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| format!("Failed to read config: {}", e))?;

            let hash = format!(
                "{:x}",
                content.len() * 31 + content.bytes().map(|b| b as usize).sum::<usize>()
            );

            Ok(serde_json::json!({
                "config": content,
                "hash": hash,
                "exists": true,
                "path": config_path.display().to_string()
            })
            .to_string())
        }

        "config.schema" => Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "agents": { "type": "object", "description": "Agent configuration" },
                "channels": { "type": "object", "description": "Channel plugins" },
                "session": { "type": "object", "description": "Session settings" },
                "messages": { "type": "object", "description": "Message formatting" },
                "providers": { "type": "object", "description": "AI providers" }
            }
        })
        .to_string()),

        "config.apply" => {
            let raw = args
                .get("raw")
                .and_then(|v| v.as_str())
                .ok_or("Missing raw config for config.apply")?;

            let _: serde_json::Value =
                serde_json::from_str(raw).map_err(|e| format!("Invalid JSON config: {}", e))?;

            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create config directory: {}", e))?;
            }

            std::fs::write(&config_path, raw)
                .map_err(|e| format!("Failed to write config: {}", e))?;

            Ok(format!(
                "Config written to {}. Gateway restart required for changes to take effect.",
                config_path.display()
            ))
        }

        "config.patch" => {
            let raw = args
                .get("raw")
                .and_then(|v| v.as_str())
                .ok_or("Missing raw patch for config.patch")?;

            let patch: serde_json::Value =
                serde_json::from_str(raw).map_err(|e| format!("Invalid JSON patch: {}", e))?;

            let existing = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)
                    .map_err(|e| format!("Failed to read config: {}", e))?;
                serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse existing config: {}", e))?
            } else {
                serde_json::json!({})
            };

            let merged = merge_json(existing, patch);

            let output = serde_json::to_string_pretty(&merged)
                .map_err(|e| format!("Failed to serialize config: {}", e))?;

            std::fs::write(&config_path, &output)
                .map_err(|e| format!("Failed to write config: {}", e))?;

            Ok(format!(
                "Config patched at {}. Gateway restart required for changes to take effect.",
                config_path.display()
            ))
        }

        "update.run" => Ok(
            "Update check requested. Note: Self-update requires external tooling (npm/cargo)."
                .to_string(),
        ),

        _ => Err(format!(
            "Unknown action: {}. Valid: restart, config.get, config.schema, config.apply, config.patch, update.run",
            action
        )),
    }
}

/// Recursively merge two JSON values (patch semantics).
fn merge_json(base: Value, patch: Value) -> Value {
    match (base, patch) {
        (Value::Object(mut base_map), Value::Object(patch_map)) => {
            for (key, patch_val) in patch_map {
                if patch_val.is_null() {
                    base_map.remove(&key);
                } else if let Some(base_val) = base_map.remove(&key) {
                    base_map.insert(key, merge_json(base_val, patch_val));
                } else {
                    base_map.insert(key, patch_val);
                }
            }
            Value::Object(base_map)
        }
        (_, patch) => patch,
    }
}

/// Send messages via channel plugins.
pub fn exec_message(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: action".to_string())?;

    match action {
        "send" => {
            let message = args
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or("Missing message for send action")?;

            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .ok_or("Missing target for send action")?;

            let channel = args
                .get("channel")
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            Ok(format!(
                "Message queued for delivery:\n- Channel: {}\n- Target: {}\n- Message: {} chars\nNote: Actual delivery requires messenger integration.",
                channel,
                target,
                message.len()
            ))
        }

        "broadcast" => {
            let message = args
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or("Missing message for broadcast action")?;

            let targets = args
                .get("targets")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();

            if targets.is_empty() {
                return Err("No targets specified for broadcast".to_string());
            }

            Ok(format!(
                "Broadcast queued:\n- Targets: {}\n- Message: {} chars\nNote: Actual delivery requires messenger integration.",
                targets.join(", "),
                message.len()
            ))
        }

        _ => Err(format!("Unknown action: {}. Valid: send, broadcast", action)),
    }
}

/// Text-to-speech conversion.
pub fn exec_tts(args: &Value, workspace_dir: &Path) -> Result<String, String> {
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: text".to_string())?;

    let output_path = workspace_dir.join(".tts").join(format!(
        "speech_{}.mp3",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    Ok(format!(
        "TTS conversion requested:\n- Text: {} chars\n- Output would be: {}\nNote: Actual TTS requires external service (ElevenLabs, etc.).\n\nMEDIA: {}",
        text.len(),
        output_path.display(),
        output_path.display()
    ))
}

/// Analyze an image using a vision model.
pub fn exec_image(args: &Value, workspace_dir: &Path) -> Result<String, String> {
    let image_path = args
        .get("image")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: image".to_string())?;

    let prompt = args
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("Describe the image.");

    // Check if it's a URL or local path
    let is_url = image_path.starts_with("http://") || image_path.starts_with("https://");

    if !is_url {
        // Resolve local path
        let full_path = resolve_path(workspace_dir, image_path);
        if !full_path.exists() {
            return Err(format!("Image file not found: {}", image_path));
        }

        // Check it's actually an image
        let ext = full_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let valid_exts = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg"];
        if !valid_exts.contains(&ext.as_str()) {
            return Err(format!(
                "Unsupported image format: {}. Supported: {}",
                ext,
                valid_exts.join(", ")
            ));
        }
    }

    Ok(format!(
        "Image analysis requested:\n- Image: {}\n- Prompt: {}\n- Is URL: {}\n\nNote: Actual image analysis requires vision model integration (GPT-4V, Claude 3, Gemini Pro Vision, etc.).",
        image_path,
        prompt,
        is_url
    ))
}
