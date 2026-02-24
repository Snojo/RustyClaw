//! QMD tools: qmd_search, qmd_deep_search, qmd_get.
//!
//! Shells out to the `qmd` CLI for hybrid search (BM25 + vector + re-ranking).
//! QMD must be installed: `npm install -g @tobilu/qmd`

use serde_json::Value;
use std::path::Path;
use std::process::Command;
use tracing::{debug, instrument};

/// Search QMD index using hybrid search (BM25 + vector + re-ranking).
#[instrument(skip(args, _workspace_dir), fields(query))]
pub fn exec_qmd_search(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: query".to_string())?;

    tracing::Span::current().record("query", query);

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);

    let collection = args.get("collection").and_then(|v| v.as_str());

    debug!(limit, collection, "QMD search");

    let mut cmd = Command::new("qmd");
    cmd.arg("search").arg(query);
    cmd.arg("--limit").arg(limit.to_string());
    cmd.arg("--format").arg("text");

    if let Some(coll) = collection {
        cmd.arg("--collection").arg(coll);
    }

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if stdout.trim().is_empty() {
                    Ok("No matching results found.".to_string())
                } else {
                    Ok(stdout)
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Err(format!("QMD search failed: {}", stderr.trim()))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err("QMD is not installed. Install with: npm install -g @tobilu/qmd".to_string())
            } else {
                Err(format!("Failed to run qmd: {}", e))
            }
        }
    }
}

/// Deep search using QMD with LLM re-ranking for highest relevance.
#[instrument(skip(args, _workspace_dir), fields(query))]
pub fn exec_qmd_deep_search(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: query".to_string())?;

    tracing::Span::current().record("query", query);

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);

    let collection = args.get("collection").and_then(|v| v.as_str());

    debug!(limit, collection, "QMD deep search");

    let mut cmd = Command::new("qmd");
    cmd.arg("deep_search").arg(query);
    cmd.arg("--limit").arg(limit.to_string());
    cmd.arg("--format").arg("text");

    if let Some(coll) = collection {
        cmd.arg("--collection").arg(coll);
    }

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if stdout.trim().is_empty() {
                    Ok("No matching results found.".to_string())
                } else {
                    Ok(stdout)
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Err(format!("QMD deep search failed: {}", stderr.trim()))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err("QMD is not installed. Install with: npm install -g @tobilu/qmd".to_string())
            } else {
                Err(format!("Failed to run qmd: {}", e))
            }
        }
    }
}

/// Get a specific document by path from the QMD index.
#[instrument(skip(args, _workspace_dir), fields(path))]
pub fn exec_qmd_get(args: &Value, _workspace_dir: &Path) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter: path".to_string())?;

    tracing::Span::current().record("path", path);

    debug!("QMD get document");

    let mut cmd = Command::new("qmd");
    cmd.arg("get").arg(path);

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if stdout.trim().is_empty() {
                    Err(format!("Document not found: {}", path))
                } else {
                    Ok(stdout)
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Err(format!("QMD get failed: {}", stderr.trim()))
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Err("QMD is not installed. Install with: npm install -g @tobilu/qmd".to_string())
            } else {
                Err(format!("Failed to run qmd: {}", e))
            }
        }
    }
}
