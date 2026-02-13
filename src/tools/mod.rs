//! Agent tool system for RustyClaw.
//!
//! Provides a registry of tools that the language model can invoke, and
//! formatters that serialise the tool definitions into each provider's
//! native schema (OpenAI function-calling, Anthropic tool-use, Google
//! function declarations).

mod helpers;
mod file;
mod runtime;
mod web;
mod memory_tools;
mod cron_tool;
mod sessions_tools;
mod patch;
mod gateway_tools;
mod devices;
mod skills_tools;
mod secrets_tools;

// Re-export helpers for external use
pub use helpers::{
    process_manager, set_credentials_dir, is_protected_path,
    expand_tilde, VAULT_ACCESS_DENIED, command_references_credentials,
};

// Internal use of helpers (import with different names to avoid conflict with re-exports)
use helpers::{resolve_path, display_path, should_visit};

// File operations
use file::{exec_read_file, exec_write_file, exec_edit_file, exec_list_directory, exec_search_files, exec_find_files};

// Runtime operations
use runtime::{exec_execute_command, exec_process};

// Web operations
use web::{exec_web_fetch, exec_web_search};

// Memory operations
use memory_tools::{exec_memory_search, exec_memory_get};

// Cron operations
use cron_tool::exec_cron;

// Session operations
use sessions_tools::{exec_sessions_list, exec_sessions_spawn, exec_sessions_send, exec_sessions_history, exec_session_status, exec_agents_list};

// Patch operations
use patch::exec_apply_patch;

// Gateway operations
use gateway_tools::{exec_gateway, exec_message, exec_tts, exec_image};

// Device operations
use devices::{exec_nodes, exec_browser, exec_canvas};

// Skill operations
use skills_tools::{exec_skill_list, exec_skill_search, exec_skill_install, exec_skill_info, exec_skill_enable, exec_skill_link_secret};

// Secrets operations
use secrets_tools::exec_secrets_stub;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

// ── Tool definitions ────────────────────────────────────────────────────────

/// JSON-Schema-like parameter definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    /// JSON Schema type: "string", "integer", "boolean", "array", "object".
    #[serde(rename = "type")]
    pub param_type: String,
    pub required: bool,
}

/// A tool that the agent can invoke.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Vec<ToolParam>,
    /// The function that executes the tool, returning a string result or error.
    pub execute: fn(args: &Value, workspace_dir: &Path) -> Result<String, String>,
}

// ── Tool registry ───────────────────────────────────────────────────────────

/// Return all available tools.
pub fn all_tools() -> Vec<&'static ToolDef> {
    vec![
        &READ_FILE,
        &WRITE_FILE,
        &EDIT_FILE,
        &LIST_DIRECTORY,
        &SEARCH_FILES,
        &FIND_FILES,
        &EXECUTE_COMMAND,
        &WEB_FETCH,
        &WEB_SEARCH,
        &PROCESS,
        &MEMORY_SEARCH,
        &MEMORY_GET,
        &CRON,
        &SESSIONS_LIST,
        &SESSIONS_SPAWN,
        &SESSIONS_SEND,
        &SESSIONS_HISTORY,
        &SESSION_STATUS,
        &AGENTS_LIST,
        &APPLY_PATCH,
        &SECRETS_LIST,
        &SECRETS_GET,
        &SECRETS_STORE,
        &GATEWAY,
        &MESSAGE,
        &TTS,
        &IMAGE,
        &NODES,
        &BROWSER,
        &CANVAS,
        &SKILL_LIST,
        &SKILL_SEARCH,
        &SKILL_INSTALL,
        &SKILL_INFO,
        &SKILL_ENABLE,
        &SKILL_LINK_SECRET,
    ]
}

// ── Built-in tools ──────────────────────────────────────────────────────────

/// `read_file` — read the contents of a file on disk.
pub static READ_FILE: ToolDef = ToolDef {
    name: "read_file",
    description: "Read the contents of a file. Returns the file text. \
                  Handles plain text files directly and can also extract \
                  text from .docx, .doc, .rtf, .odt, .pdf, and .html files. \
                  If you have an absolute path from find_files or search_files, \
                  pass it exactly as-is. Use the optional start_line / end_line \
                  parameters to read a specific range (1-based, inclusive).",
    parameters: vec![],  // filled by init; see `read_file_params()`.
    execute: exec_read_file,
};

pub static WRITE_FILE: ToolDef = ToolDef {
    name: "write_file",
    description: "Create or overwrite a file with the given content. \
                  Parent directories are created automatically.",
    parameters: vec![],
    execute: exec_write_file,
};

pub static EDIT_FILE: ToolDef = ToolDef {
    name: "edit_file",
    description: "Make a targeted edit to an existing file using search-and-replace. \
                  The old_string must match exactly one location in the file. \
                  Include enough context lines to make the match unique.",
    parameters: vec![],
    execute: exec_edit_file,
};

pub static LIST_DIRECTORY: ToolDef = ToolDef {
    name: "list_directory",
    description: "List the contents of a directory. Returns file and \
                  directory names, with directories suffixed by '/'.",
    parameters: vec![],
    execute: exec_list_directory,
};

pub static SEARCH_FILES: ToolDef = ToolDef {
    name: "search_files",
    description: "Search file CONTENTS for a text pattern (like grep -i). \
                  The search is case-insensitive. Returns matching lines \
                  with paths and line numbers. Use `find_files` instead \
                  when searching by file name. Set `path` to an absolute \
                  directory (e.g. '/Users/alice') to search outside the \
                  workspace.",
    parameters: vec![],
    execute: exec_search_files,
};

pub static FIND_FILES: ToolDef = ToolDef {
    name: "find_files",
    description: "Find files by name. Returns paths that can be passed directly to read_file. Accepts plain keywords (case-insensitive \
                  substring match) OR glob patterns (e.g. '*.pdf'). Multiple \
                  keywords can be separated with spaces — a file matches if its \
                  name contains ANY keyword. Examples: 'resume', 'resume cv', \
                  '*.pdf'. Set `path` to an absolute directory to search outside \
                  the workspace (e.g. '/Users/alice'). Use `search_files` to \
                  search file CONTENTS instead.",
    parameters: vec![],
    execute: exec_find_files,
};

pub static EXECUTE_COMMAND: ToolDef = ToolDef {
    name: "execute_command",
    description: "Execute a shell command and return its output (stdout + stderr). \
                  Runs via `sh -c` in the workspace directory by default. \
                  Use for builds, tests, git operations, system lookups \
                  (e.g. `find ~ -name '*.pdf'`, `mdfind`, `which`), or \
                  any other CLI task. Set `working_dir` to an absolute \
                  path to run in a different directory.",
    parameters: vec![],
    execute: exec_execute_command,
};

pub static WEB_FETCH: ToolDef = ToolDef {
    name: "web_fetch",
    description: "Fetch and extract readable content from a URL (HTML → markdown or plain text). \
                  Use for reading web pages, documentation, articles, or any HTTP-accessible content. \
                  For JavaScript-heavy sites that require rendering, use a browser tool instead.",
    parameters: vec![],
    execute: exec_web_fetch,
};

pub static WEB_SEARCH: ToolDef = ToolDef {
    name: "web_search",
    description: "Search the web using Brave Search API. Returns titles, URLs, and snippets. \
                  Requires BRAVE_API_KEY environment variable to be set. \
                  Use for finding current information, research, and fact-checking.",
    parameters: vec![],
    execute: exec_web_search,
};

pub static PROCESS: ToolDef = ToolDef {
    name: "process",
    description: "Manage background exec sessions. Actions: list (show all sessions), \
                  poll (get new output + status for a session), log (get output with offset/limit), \
                  write (send data to stdin), kill (terminate a session), clear (remove completed sessions), \
                  remove (remove a specific session).",
    parameters: vec![],
    execute: exec_process,
};

pub static MEMORY_SEARCH: ToolDef = ToolDef {
    name: "memory_search",
    description: "Semantically search MEMORY.md and memory/*.md files for relevant information. \
                  Use before answering questions about prior work, decisions, dates, people, \
                  preferences, or todos. Returns matching snippets with file path and line numbers.",
    parameters: vec![],
    execute: exec_memory_search,
};

pub static MEMORY_GET: ToolDef = ToolDef {
    name: "memory_get",
    description: "Read content from a memory file (MEMORY.md or memory/*.md). \
                  Use after memory_search to get full context around a snippet. \
                  Supports optional line range for large files.",
    parameters: vec![],
    execute: exec_memory_get,
};

pub static CRON: ToolDef = ToolDef {
    name: "cron",
    description: "Manage scheduled jobs. Actions: status (scheduler status), list (show jobs), \
                  add (create job), update (modify job), remove (delete job), run (trigger immediately), \
                  runs (get run history). Use for reminders and recurring tasks.",
    parameters: vec![],
    execute: exec_cron,
};

pub static SESSIONS_LIST: ToolDef = ToolDef {
    name: "sessions_list",
    description: "List active sessions with optional filters. Shows main sessions and sub-agents. \
                  Use to check on running background tasks.",
    parameters: vec![],
    execute: exec_sessions_list,
};

pub static SESSIONS_SPAWN: ToolDef = ToolDef {
    name: "sessions_spawn",
    description: "Spawn a sub-agent to run a task in the background. The sub-agent runs in its own \
                  isolated session and announces results back when finished. Non-blocking.",
    parameters: vec![],
    execute: exec_sessions_spawn,
};

pub static SESSIONS_SEND: ToolDef = ToolDef {
    name: "sessions_send",
    description: "Send a message to another session. Use sessionKey or label to identify the target. \
                  Returns immediately after sending.",
    parameters: vec![],
    execute: exec_sessions_send,
};

pub static SESSIONS_HISTORY: ToolDef = ToolDef {
    name: "sessions_history",
    description: "Fetch message history for a session. Returns recent messages from the specified session.",
    parameters: vec![],
    execute: exec_sessions_history,
};

pub static SESSION_STATUS: ToolDef = ToolDef {
    name: "session_status",
    description: "Show session status including usage, time, and cost. Use for model-use questions. \
                  Can also set per-session model override.",
    parameters: vec![],
    execute: exec_session_status,
};

pub static AGENTS_LIST: ToolDef = ToolDef {
    name: "agents_list",
    description: "List available agent IDs that can be targeted with sessions_spawn. \
                  Returns the configured agents based on allowlists.",
    parameters: vec![],
    execute: exec_agents_list,
};

pub static APPLY_PATCH: ToolDef = ToolDef {
    name: "apply_patch",
    description: "Apply a unified diff patch to one or more files. Supports multi-hunk patches. \
                  Use for complex multi-line edits where edit_file would be cumbersome.",
    parameters: vec![],
    execute: exec_apply_patch,
};

pub static SECRETS_LIST: ToolDef = ToolDef {
    name: "secrets_list",
    description: "List the names (keys) stored in the encrypted secrets vault. \
                  Returns only key names, never values. Use secrets_get to \
                  retrieve a specific value.",
    parameters: vec![],
    execute: exec_secrets_stub,
};

pub static SECRETS_GET: ToolDef = ToolDef {
    name: "secrets_get",
    description: "Retrieve a secret value from the encrypted vault by key name. \
                  The value is returned as a string. Prefer injecting it directly \
                  into environment variables or config rather than echoing it.",
    parameters: vec![],
    execute: exec_secrets_stub,
};

pub static SECRETS_STORE: ToolDef = ToolDef {
    name: "secrets_store",
    description: "Store or update a key/value pair in the encrypted secrets vault. \
                  The value is encrypted at rest. Use for API keys, tokens, and \
                  other sensitive material.",
    parameters: vec![],
    execute: exec_secrets_stub,
};

pub static GATEWAY: ToolDef = ToolDef {
    name: "gateway",
    description: "Manage the gateway daemon. Actions: restart (restart gateway), \
                  config.get (get current config), config.schema (get config schema), \
                  config.apply (replace entire config), config.patch (partial config update), \
                  update.run (update gateway).",
    parameters: vec![],
    execute: exec_gateway,
};

pub static MESSAGE: ToolDef = ToolDef {
    name: "message",
    description: "Send messages via channel plugins. Actions: send (send a message), \
                  broadcast (send to multiple targets). Supports various channels \
                  like telegram, discord, whatsapp, signal, etc.",
    parameters: vec![],
    execute: exec_message,
};

pub static TTS: ToolDef = ToolDef {
    name: "tts",
    description: "Convert text to speech and return a media path. Use when the user \
                  requests audio or TTS is enabled.",
    parameters: vec![],
    execute: exec_tts,
};

pub static IMAGE: ToolDef = ToolDef {
    name: "image",
    description: "Analyze an image using the configured image/vision model. \
                  Pass a local file path or URL. Returns a text description or \
                  answers the prompt about the image.",
    parameters: vec![],
    execute: exec_image,
};

pub static NODES: ToolDef = ToolDef {
    name: "nodes",
    description: "Discover and control paired nodes (companion devices). Actions: \
                  status (list nodes), describe (node details), pending/approve/reject (pairing), \
                  notify (send notification), camera_snap/camera_list (camera), \
                  screen_record (screen capture), location_get (GPS), run/invoke (remote commands).",
    parameters: vec![],
    execute: exec_nodes,
};

pub static BROWSER: ToolDef = ToolDef {
    name: "browser",
    description: "Control web browser for automation. Actions: status, start, stop, \
                  profiles, tabs, open, focus, close, snapshot, screenshot, navigate, \
                  console, pdf, act (click/type/press/hover/drag). Use snapshot to get \
                  page accessibility tree for element targeting.",
    parameters: vec![],
    execute: exec_browser,
};

pub static CANVAS: ToolDef = ToolDef {
    name: "canvas",
    description: "Control node canvases for UI presentation. Actions: present (show content), \
                  hide, navigate, eval (run JavaScript), snapshot (capture rendered UI), \
                  a2ui_push/a2ui_reset (accessibility-to-UI).",
    parameters: vec![],
    execute: exec_canvas,
};

pub static SKILL_LIST: ToolDef = ToolDef {
    name: "skill_list",
    description: "List all loaded skills with their status (enabled, gates, source, linked secrets). \
                  Use to discover what capabilities are available.",
    parameters: vec![],
    execute: exec_skill_list,
};

pub static SKILL_SEARCH: ToolDef = ToolDef {
    name: "skill_search",
    description: "Search the ClawHub registry for installable skills. Returns skill names, \
                  descriptions, versions, and required secrets.",
    parameters: vec![],
    execute: exec_skill_search,
};

pub static SKILL_INSTALL: ToolDef = ToolDef {
    name: "skill_install",
    description: "Install a skill from the ClawHub registry by name. Optionally specify a version. \
                  After installation the skill is immediately available. Use skill_link_secret to \
                  bind required credentials.",
    parameters: vec![],
    execute: exec_skill_install,
};

pub static SKILL_INFO: ToolDef = ToolDef {
    name: "skill_info",
    description: "Show detailed information about a loaded skill: description, source, linked \
                  secrets, gating status, and instructions summary.",
    parameters: vec![],
    execute: exec_skill_info,
};

pub static SKILL_ENABLE: ToolDef = ToolDef {
    name: "skill_enable",
    description: "Enable or disable a loaded skill. Disabled skills are not injected into the \
                  agent prompt and cannot be activated.",
    parameters: vec![],
    execute: exec_skill_enable,
};

pub static SKILL_LINK_SECRET: ToolDef = ToolDef {
    name: "skill_link_secret",
    description: "Link or unlink a vault credential to a skill. When linked, the secret is \
                  accessible under the SkillOnly policy while the skill is active. Use action \
                  'link' to bind or 'unlink' to remove the binding.",
    parameters: vec![],
    execute: exec_skill_link_secret,
};

// ── Parameter definitions ───────────────────────────────────────────────────

/// We need a runtime-constructed param list because `Vec` isn't const.
/// This function is what the registry / formatters actually call.
pub fn read_file_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "path".into(),
            description: "Path to the file to read. IMPORTANT: if you received \
                          an absolute path from find_files or search_files \
                          (starting with /), pass it exactly as-is. Only \
                          relative paths are resolved against the workspace root."
                .into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "start_line".into(),
            description: "First line to read (1-based, inclusive). Omit to start from the beginning.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "end_line".into(),
            description: "Last line to read (1-based, inclusive). Omit to read to the end.".into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn write_file_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "path".into(),
            description: "Path to the file to create or overwrite.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "content".into(),
            description: "The full content to write to the file.".into(),
            param_type: "string".into(),
            required: true,
        },
    ]
}

fn edit_file_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "path".into(),
            description: "Path to the file to edit.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "old_string".into(),
            description: "The exact text to find (must match exactly once). \
                          Include surrounding context lines for uniqueness."
                .into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "new_string".into(),
            description: "The replacement text.".into(),
            param_type: "string".into(),
            required: true,
        },
    ]
}

fn list_directory_params() -> Vec<ToolParam> {
    vec![ToolParam {
        name: "path".into(),
        description: "Path to the directory to list.".into(),
        param_type: "string".into(),
        required: true,
    }]
}

fn search_files_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "pattern".into(),
            description: "The text pattern to search for inside files.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "path".into(),
            description: "Directory to search in. Defaults to the workspace root. \
                          Use an absolute path (e.g. '/Users/alice/Documents') to \
                          search outside the workspace."
                .into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "include".into(),
            description: "Glob pattern to filter filenames (e.g. '*.rs').".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn find_files_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "pattern".into(),
            description: "Search term(s) or glob pattern. Plain words are matched \
                          case-insensitively against file names (e.g. 'resume' \
                          matches Resume.pdf). Separate multiple keywords with \
                          spaces to match ANY (e.g. 'resume cv'). Use glob \
                          syntax ('*', '?') for extension filters (e.g. '*.pdf')."
                .into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "path".into(),
            description: "Base directory for the search. Defaults to the workspace root. \
                          Use an absolute path (e.g. '/Users/alice' or '~') to \
                          search outside the workspace."
                .into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn execute_command_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "command".into(),
            description: "The shell command to execute (passed to sh -c).".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "working_dir".into(),
            description: "Working directory for the command. Defaults to the workspace root. \
                          Use an absolute path to run elsewhere."
                .into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "timeout_secs".into(),
            description: "Maximum seconds before killing the command (default: 30).".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "background".into(),
            description: "Run in background immediately. Returns a sessionId for use with process tool.".into(),
            param_type: "boolean".into(),
            required: false,
        },
        ToolParam {
            name: "yieldMs".into(),
            description: "Milliseconds to wait before auto-backgrounding (default: 10000). \
                          Set to 0 to disable auto-background."
                .into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn web_fetch_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "url".into(),
            description: "HTTP or HTTPS URL to fetch.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "extract_mode".into(),
            description: "Extraction mode: 'markdown' (default) or 'text'. \
                          Markdown preserves links and structure; text is plain."
                .into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "max_chars".into(),
            description: "Maximum characters to return (truncates if exceeded). \
                          Default: 50000."
                .into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn web_search_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "query".into(),
            description: "Search query string.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "count".into(),
            description: "Number of results to return (1-10). Default: 5.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "country".into(),
            description: "2-letter country code for region-specific results (e.g., 'DE', 'US'). \
                          Default: 'US'."
                .into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "search_lang".into(),
            description: "ISO language code for search results (e.g., 'de', 'en', 'fr').".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "freshness".into(),
            description: "Filter results by discovery time. Values: 'pd' (past 24h), \
                          'pw' (past week), 'pm' (past month), 'py' (past year), \
                          or date range 'YYYY-MM-DDtoYYYY-MM-DD'."
                .into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn process_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action to perform: 'list', 'poll', 'log', 'write', 'kill', 'clear', 'remove'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "sessionId".into(),
            description: "Session ID for poll/log/write/kill/remove actions.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "data".into(),
            description: "Data to write to stdin (for 'write' action).".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "offset".into(),
            description: "Line offset for 'log' action (0-indexed). Omit to get last N lines.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "limit".into(),
            description: "Maximum lines to return for 'log' action. Default: 50.".into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn memory_search_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "query".into(),
            description: "Search query for finding relevant memory content.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "maxResults".into(),
            description: "Maximum number of results to return. Default: 5.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "minScore".into(),
            description: "Minimum relevance score threshold (0.0-1.0). Default: 0.1.".into(),
            param_type: "number".into(),
            required: false,
        },
    ]
}

fn memory_get_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "path".into(),
            description: "Path to the memory file (MEMORY.md or memory/*.md).".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "from".into(),
            description: "Starting line number (1-indexed). Default: 1.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "lines".into(),
            description: "Number of lines to read. Default: entire file.".into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn secrets_list_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "prefix".into(),
            description: "Optional prefix to filter key names.".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn secrets_get_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "key".into(),
            description: "The name of the secret to retrieve.".into(),
            param_type: "string".into(),
            required: true,
        },
    ]
}

fn secrets_store_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "key".into(),
            description: "The name under which to store the secret.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "value".into(),
            description: "The secret value to encrypt and store.".into(),
            param_type: "string".into(),
            required: true,
        },
    ]
}

fn gateway_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action: 'restart', 'config.get', 'config.schema', 'config.apply', 'config.patch', 'update.run'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "raw".into(),
            description: "JSON config content for config.apply or config.patch.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "baseHash".into(),
            description: "Config hash from config.get (required for apply/patch when config exists).".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "reason".into(),
            description: "Reason for restart or config change.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "delayMs".into(),
            description: "Delay before restart in milliseconds. Default: 2000.".into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn message_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action: 'send' or 'broadcast'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "message".into(),
            description: "Message content to send.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "target".into(),
            description: "Target channel/user ID or name.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "channel".into(),
            description: "Channel type: telegram, discord, whatsapp, signal, slack, etc.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "targets".into(),
            description: "Multiple targets for broadcast action.".into(),
            param_type: "array".into(),
            required: false,
        },
        ToolParam {
            name: "replyTo".into(),
            description: "Message ID to reply to.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "silent".into(),
            description: "Send without notification. Default: false.".into(),
            param_type: "boolean".into(),
            required: false,
        },
    ]
}

fn tts_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "text".into(),
            description: "Text to convert to speech.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "channel".into(),
            description: "Optional channel ID to pick output format.".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn image_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "image".into(),
            description: "Path to local image file or URL.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "prompt".into(),
            description: "Question or instruction about the image. Default: 'Describe the image.'".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn nodes_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action: 'status', 'describe', 'pending', 'approve', 'reject', 'notify', 'camera_snap', 'camera_list', 'screen_record', 'location_get', 'run', 'invoke'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "node".into(),
            description: "Node ID or name to target.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "requestId".into(),
            description: "Pairing request ID for approve/reject.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "title".into(),
            description: "Notification title.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "body".into(),
            description: "Notification body text.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "command".into(),
            description: "Command array for 'run' action.".into(),
            param_type: "array".into(),
            required: false,
        },
        ToolParam {
            name: "invokeCommand".into(),
            description: "Command name for 'invoke' action.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "facing".into(),
            description: "Camera facing: 'front', 'back', or 'both'.".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn browser_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action: 'status', 'start', 'stop', 'profiles', 'tabs', 'open', 'focus', 'close', 'snapshot', 'screenshot', 'navigate', 'console', 'pdf', 'act'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "profile".into(),
            description: "Browser profile: 'openclaw' (managed) or 'chrome' (extension relay).".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "targetUrl".into(),
            description: "URL for 'open' or 'navigate' actions.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "targetId".into(),
            description: "Tab ID for targeting specific tab.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "ref".into(),
            description: "Element reference from snapshot for actions.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "request".into(),
            description: "Action request object with kind (click/type/press/hover/drag), ref, text, etc.".into(),
            param_type: "object".into(),
            required: false,
        },
        ToolParam {
            name: "fullPage".into(),
            description: "Capture full page for screenshot. Default: false.".into(),
            param_type: "boolean".into(),
            required: false,
        },
    ]
}

fn canvas_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action: 'present', 'hide', 'navigate', 'eval', 'snapshot', 'a2ui_push', 'a2ui_reset'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "node".into(),
            description: "Target node for canvas operations.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "url".into(),
            description: "URL to present or navigate to.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "javaScript".into(),
            description: "JavaScript code for 'eval' action.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "width".into(),
            description: "Canvas width in pixels.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "height".into(),
            description: "Canvas height in pixels.".into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn cron_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action: 'status', 'list', 'add', 'update', 'remove', 'run', 'runs'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "jobId".into(),
            description: "Job ID for update/remove/run/runs actions.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "job".into(),
            description: "Job definition object for 'add' action.".into(),
            param_type: "object".into(),
            required: false,
        },
        ToolParam {
            name: "patch".into(),
            description: "Patch object for 'update' action.".into(),
            param_type: "object".into(),
            required: false,
        },
        ToolParam {
            name: "includeDisabled".into(),
            description: "Include disabled jobs in list. Default: false.".into(),
            param_type: "boolean".into(),
            required: false,
        },
    ]
}

fn sessions_list_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "kinds".into(),
            description: "Filter by session kinds: 'main', 'subagent', 'cron'.".into(),
            param_type: "array".into(),
            required: false,
        },
        ToolParam {
            name: "activeMinutes".into(),
            description: "Only show sessions active within N minutes.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "limit".into(),
            description: "Maximum sessions to return. Default: 20.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "messageLimit".into(),
            description: "Include last N messages per session.".into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn sessions_spawn_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "task".into(),
            description: "What the sub-agent should do (required).".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "label".into(),
            description: "Short label for identification.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "agentId".into(),
            description: "Spawn under a different agent ID.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "model".into(),
            description: "Override the model for this sub-agent.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "thinking".into(),
            description: "Override thinking level (off/low/medium/high).".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "runTimeoutSeconds".into(),
            description: "Abort sub-agent after N seconds.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "cleanup".into(),
            description: "'delete' or 'keep' session after completion.".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn sessions_send_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "message".into(),
            description: "Message to send to the target session.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "sessionKey".into(),
            description: "Session key to send to.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "label".into(),
            description: "Session label to send to (alternative to sessionKey).".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "timeoutSeconds".into(),
            description: "Timeout for waiting on response.".into(),
            param_type: "integer".into(),
            required: false,
        },
    ]
}

fn sessions_history_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "sessionKey".into(),
            description: "Session key to get history for.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "limit".into(),
            description: "Maximum messages to return. Default: 20.".into(),
            param_type: "integer".into(),
            required: false,
        },
        ToolParam {
            name: "includeTools".into(),
            description: "Include tool call messages. Default: false.".into(),
            param_type: "boolean".into(),
            required: false,
        },
    ]
}

fn session_status_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "sessionKey".into(),
            description: "Session key to get status for. Default: current session.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "model".into(),
            description: "Set per-session model override. Use 'default' to reset.".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn agents_list_params() -> Vec<ToolParam> {
    // No parameters needed
    vec![]
}

fn apply_patch_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "patch".into(),
            description: "Unified diff patch content to apply.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "path".into(),
            description: "Target file path. If not specified, parsed from patch header.".into(),
            param_type: "string".into(),
            required: false,
        },
        ToolParam {
            name: "dry_run".into(),
            description: "If true, validate patch without applying. Default: false.".into(),
            param_type: "boolean".into(),
            required: false,
        },
    ]
}

// ── Skill tool parameters ───────────────────────────────────────────────────

fn skill_list_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "filter".into(),
            description: "Optional filter: 'all' (default), 'enabled', 'disabled', 'registry'.".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn skill_search_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "query".into(),
            description: "Search query for the ClawHub registry.".into(),
            param_type: "string".into(),
            required: true,
        },
    ]
}

fn skill_install_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "name".into(),
            description: "Name of the skill to install from ClawHub.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "version".into(),
            description: "Specific version to install (default: latest).".into(),
            param_type: "string".into(),
            required: false,
        },
    ]
}

fn skill_info_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "name".into(),
            description: "Name of the skill to get info about.".into(),
            param_type: "string".into(),
            required: true,
        },
    ]
}

fn skill_enable_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "name".into(),
            description: "Name of the skill to enable or disable.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "enabled".into(),
            description: "Whether to enable (true) or disable (false) the skill.".into(),
            param_type: "boolean".into(),
            required: true,
        },
    ]
}

fn skill_link_secret_params() -> Vec<ToolParam> {
    vec![
        ToolParam {
            name: "action".into(),
            description: "Action: 'link' or 'unlink'.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "skill".into(),
            description: "Name of the skill.".into(),
            param_type: "string".into(),
            required: true,
        },
        ToolParam {
            name: "secret".into(),
            description: "Name of the vault credential to link/unlink.".into(),
            param_type: "string".into(),
            required: true,
        },
    ]
}

// ── Provider-specific formatters ────────────────────────────────────────────

/// Parameters for a tool, building a JSON Schema `properties` / `required`.
fn params_to_json_schema(params: &[ToolParam]) -> (Value, Value) {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for p in params {
        let mut prop = serde_json::Map::new();
        prop.insert("type".into(), json!(p.param_type));
        prop.insert("description".into(), json!(p.description));

        // Arrays need an items schema
        if p.param_type == "array" {
            prop.insert("items".into(), json!({"type": "string"}));
        }

        properties.insert(p.name.clone(), Value::Object(prop));
        if p.required {
            required.push(json!(p.name));
        }
    }

    (Value::Object(properties), Value::Array(required))
}

/// Resolve the parameter list for a tool (static defs use empty vecs
/// because Vec isn't const; we resolve at call time).
fn resolve_params(tool: &ToolDef) -> Vec<ToolParam> {
    if !tool.parameters.is_empty() {
        return tool.parameters.clone();
    }
    match tool.name {
        "read_file" => read_file_params(),
        "write_file" => write_file_params(),
        "edit_file" => edit_file_params(),
        "list_directory" => list_directory_params(),
        "search_files" => search_files_params(),
        "find_files" => find_files_params(),
        "execute_command" => execute_command_params(),
        "web_fetch" => web_fetch_params(),
        "web_search" => web_search_params(),
        "process" => process_params(),
        "memory_search" => memory_search_params(),
        "memory_get" => memory_get_params(),
        "cron" => cron_params(),
        "sessions_list" => sessions_list_params(),
        "sessions_spawn" => sessions_spawn_params(),
        "sessions_send" => sessions_send_params(),
        "sessions_history" => sessions_history_params(),
        "session_status" => session_status_params(),
        "agents_list" => agents_list_params(),
        "apply_patch" => apply_patch_params(),
        "secrets_list" => secrets_list_params(),
        "secrets_get" => secrets_get_params(),
        "secrets_store" => secrets_store_params(),
        "gateway" => gateway_params(),
        "message" => message_params(),
        "tts" => tts_params(),
        "image" => image_params(),
        "nodes" => nodes_params(),
        "browser" => browser_params(),
        "canvas" => canvas_params(),
        "skill_list" => skill_list_params(),
        "skill_search" => skill_search_params(),
        "skill_install" => skill_install_params(),
        "skill_info" => skill_info_params(),
        "skill_enable" => skill_enable_params(),
        "skill_link_secret" => skill_link_secret_params(),
        _ => vec![],
    }
}

/// OpenAI / OpenAI-compatible function-calling format.
///
/// ```json
/// { "type": "function", "function": { "name", "description", "parameters": { … } } }
/// ```
pub fn tools_openai() -> Vec<Value> {
    all_tools()
        .into_iter()
        .map(|t| {
            let params = resolve_params(t);
            let (properties, required) = params_to_json_schema(&params);
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    }
                }
            })
        })
        .collect()
}

/// Anthropic tool-use format.
///
/// ```json
/// { "name", "description", "input_schema": { … } }
/// ```
pub fn tools_anthropic() -> Vec<Value> {
    all_tools()
        .into_iter()
        .map(|t| {
            let params = resolve_params(t);
            let (properties, required) = params_to_json_schema(&params);
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                }
            })
        })
        .collect()
}

/// Google Gemini function-declaration format.
///
/// ```json
/// { "name", "description", "parameters": { … } }
/// ```
pub fn tools_google() -> Vec<Value> {
    all_tools()
        .into_iter()
        .map(|t| {
            let params = resolve_params(t);
            let (properties, required) = params_to_json_schema(&params);
            json!({
                "name": t.name,
                "description": t.description,
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                }
            })
        })
        .collect()
}

// ── Tool execution ──────────────────────────────────────────────────────────

/// Returns `true` for tools that must be routed through the gateway
/// (i.e. handled by `execute_secrets_tool`) rather than `execute_tool`.
pub fn is_secrets_tool(name: &str) -> bool {
    matches!(name, "secrets_list" | "secrets_get" | "secrets_store")
}

/// Returns `true` for skill-management tools that are routed through the
/// gateway (i.e. handled by `execute_skill_tool`) because they need access
/// to the process-global `SkillManager`.
pub fn is_skill_tool(name: &str) -> bool {
    matches!(
        name,
        "skill_list"
            | "skill_search"
            | "skill_install"
            | "skill_info"
            | "skill_enable"
            | "skill_link_secret"
    )
}

/// Find a tool by name and execute it with the given arguments.
pub fn execute_tool(name: &str, args: &Value, workspace_dir: &Path) -> Result<String, String> {
    for tool in all_tools() {
        if tool.name == name {
            return (tool.execute)(args, workspace_dir);
        }
    }
    Err(format!("Unknown tool: {}", name))
}

// ── Wire types for WebSocket protocol ───────────────────────────────────────

/// A tool call requested by the model (sent gateway → client for display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// The result of executing a tool (sent gateway → client for display,
/// and also injected back into the conversation for the model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub result: String,
    pub is_error: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Helper: return the project root as workspace dir for tests.
    fn ws() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    // ── read_file ───────────────────────────────────────────────────

    #[test]
    fn test_read_file_this_file() {
        let args = json!({ "path": file!(), "start_line": 1, "end_line": 5 });
        let result = exec_read_file(&args, ws());
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Agent tool system"));
    }

    #[test]
    fn test_read_file_missing() {
        let args = json!({ "path": "/nonexistent/file.txt" });
        let result = exec_read_file(&args, ws());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_no_path() {
        let args = json!({});
        let result = exec_read_file(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_read_file_relative() {
        // Relative path should resolve against workspace_dir.
        let args = json!({ "path": "Cargo.toml", "start_line": 1, "end_line": 3 });
        let result = exec_read_file(&args, ws());
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("package"));
    }

    // ── write_file ──────────────────────────────────────────────────

    #[test]
    fn test_write_file_and_read_back() {
        let dir = std::env::temp_dir().join("rustyclaw_test_write");
        let _ = std::fs::remove_dir_all(&dir);
        let args = json!({
            "path": "sub/test.txt",
            "content": "hello world"
        });
        let result = exec_write_file(&args, &dir);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("11 bytes"));

        let content = std::fs::read_to_string(dir.join("sub/test.txt")).unwrap();
        assert_eq!(content, "hello world");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── edit_file ───────────────────────────────────────────────────

    #[test]
    fn test_edit_file_single_match() {
        let dir = std::env::temp_dir().join("rustyclaw_test_edit");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), "aaa\nbbb\nccc\n").unwrap();

        let args = json!({ "path": "f.txt", "old_string": "bbb", "new_string": "BBB" });
        let result = exec_edit_file(&args, &dir);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(dir.join("f.txt")).unwrap();
        assert_eq!(content, "aaa\nBBB\nccc\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_edit_file_no_match() {
        let dir = std::env::temp_dir().join("rustyclaw_test_edit_no");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), "aaa\nbbb\n").unwrap();

        let args = json!({ "path": "f.txt", "old_string": "zzz", "new_string": "ZZZ" });
        let result = exec_edit_file(&args, &dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_edit_file_multiple_matches() {
        let dir = std::env::temp_dir().join("rustyclaw_test_edit_multi");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), "aaa\naaa\n").unwrap();

        let args = json!({ "path": "f.txt", "old_string": "aaa", "new_string": "bbb" });
        let result = exec_edit_file(&args, &dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("2 times"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── list_directory ──────────────────────────────────────────────

    #[test]
    fn test_list_directory() {
        let args = json!({ "path": "src" });
        let result = exec_list_directory(&args, ws());
        assert!(result.is_ok());
        let text = result.unwrap();
        // tools is now a directory
        assert!(text.contains("tools/"));
        assert!(text.contains("main.rs"));
    }

    // ── search_files ────────────────────────────────────────────────

    #[test]
    fn test_search_files_finds_pattern() {
        let args = json!({ "pattern": "exec_read_file", "path": "src", "include": "*.rs" });
        let result = exec_search_files(&args, ws());
        assert!(result.is_ok());
        let text = result.unwrap();
        // The function is now in tools/file.rs
        assert!(text.contains("tools/file.rs") || text.contains("tools\\file.rs"));
    }

    #[test]
    fn test_search_files_no_match() {
        let dir = std::env::temp_dir().join("rustyclaw_test_search_none");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hello world\n").unwrap();

        let args = json!({ "pattern": "XYZZY_NEVER_42" });
        let result = exec_search_files(&args, &dir);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No matches"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── find_files ──────────────────────────────────────────────────

    #[test]
    fn test_find_files_glob() {
        let args = json!({ "pattern": "*.toml" });
        let result = exec_find_files(&args, ws());
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Cargo.toml"));
    }

    #[test]
    fn test_find_files_keyword_case_insensitive() {
        // "cargo" should match "Cargo.toml" (case-insensitive).
        let args = json!({ "pattern": "cargo" });
        let result = exec_find_files(&args, ws());
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Cargo.toml"));
    }

    #[test]
    fn test_find_files_multiple_keywords() {
        // Space-separated keywords: match ANY.
        let args = json!({ "pattern": "cargo license" });
        let result = exec_find_files(&args, ws());
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Cargo.toml"));
        assert!(text.contains("LICENSE"));
    }

    #[test]
    fn test_find_files_keyword_no_match() {
        let dir = std::env::temp_dir().join("rustyclaw_test_find_kw");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hello.txt"), "content").unwrap();

        let args = json!({ "pattern": "resume" });
        let result = exec_find_files(&args, &dir);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No files found"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── execute_command ─────────────────────────────────────────────

    #[test]
    fn test_execute_command_echo() {
        let args = json!({ "command": "echo hello" });
        let result = exec_execute_command(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }

    #[test]
    fn test_execute_command_failure() {
        let args = json!({ "command": "false" });
        let result = exec_execute_command(&args, ws());
        assert!(result.is_ok()); // still returns Ok with exit code
        assert!(result.unwrap().contains("exit code"));
    }

    // ── execute_tool dispatch ───────────────────────────────────────

    #[test]
    fn test_execute_tool_dispatch() {
        let args = json!({ "path": file!() });
        let result = execute_tool("read_file", &args, ws());
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_tool_unknown() {
        let result = execute_tool("no_such_tool", &json!({}), ws());
        assert!(result.is_err());
    }

    // ── Provider format tests ───────────────────────────────────────

    #[test]
    fn test_openai_format() {
        let tools = tools_openai();
        assert_eq!(tools.len(), 36);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "read_file");
        assert!(tools[0]["function"]["parameters"]["properties"]["path"].is_object());
    }

    #[test]
    fn test_anthropic_format() {
        let tools = tools_anthropic();
        assert_eq!(tools.len(), 36);
        assert_eq!(tools[0]["name"], "read_file");
        assert!(tools[0]["input_schema"]["properties"]["path"].is_object());
    }

    #[test]
    fn test_google_format() {
        let tools = tools_google();
        assert_eq!(tools.len(), 36);
        assert_eq!(tools[0]["name"], "read_file");
    }

    // ── resolve_path helper ─────────────────────────────────────────

    #[test]
    fn test_resolve_path_absolute() {
        let result = helpers::resolve_path(Path::new("/workspace"), "/absolute/path.txt");
        assert_eq!(result, std::path::PathBuf::from("/absolute/path.txt"));
    }

    #[test]
    fn test_resolve_path_relative() {
        let result = helpers::resolve_path(Path::new("/workspace"), "relative/path.txt");
        assert_eq!(result, std::path::PathBuf::from("/workspace/relative/path.txt"));
    }

    // ── web_fetch ───────────────────────────────────────────────────

    #[test]
    fn test_web_fetch_missing_url() {
        let args = json!({});
        let result = exec_web_fetch(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_web_fetch_invalid_url() {
        let args = json!({ "url": "not-a-url" });
        let result = exec_web_fetch(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("http"));
    }

    #[test]
    fn test_web_fetch_params_defined() {
        let params = web_fetch_params();
        assert_eq!(params.len(), 3);
        assert!(params.iter().any(|p| p.name == "url" && p.required));
        assert!(params.iter().any(|p| p.name == "extract_mode" && !p.required));
        assert!(params.iter().any(|p| p.name == "max_chars" && !p.required));
    }

    // ── web_search ──────────────────────────────────────────────────

    #[test]
    fn test_web_search_missing_query() {
        let args = json!({});
        let result = exec_web_search(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_web_search_no_api_key() {
        // Clear any existing key for the test
        // SAFETY: This test is single-threaded and no other thread reads BRAVE_API_KEY.
        unsafe { std::env::remove_var("BRAVE_API_KEY") };
        let args = json!({ "query": "test" });
        let result = exec_web_search(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("BRAVE_API_KEY"));
    }

    #[test]
    fn test_web_search_params_defined() {
        let params = web_search_params();
        assert_eq!(params.len(), 5);
        assert!(params.iter().any(|p| p.name == "query" && p.required));
        assert!(params.iter().any(|p| p.name == "count" && !p.required));
        assert!(params.iter().any(|p| p.name == "country" && !p.required));
        assert!(params.iter().any(|p| p.name == "search_lang" && !p.required));
        assert!(params.iter().any(|p| p.name == "freshness" && !p.required));
    }

    // ── process ─────────────────────────────────────────────────────

    #[test]
    fn test_process_missing_action() {
        let args = json!({});
        let result = exec_process(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_process_invalid_action() {
        let args = json!({ "action": "invalid" });
        let result = exec_process(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown action"));
    }

    #[test]
    fn test_process_list_empty() {
        let args = json!({ "action": "list" });
        let result = exec_process(&args, ws());
        assert!(result.is_ok());
        // May have sessions from other tests, so just check it doesn't error
    }

    #[test]
    fn test_process_params_defined() {
        let params = process_params();
        assert_eq!(params.len(), 5);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
        assert!(params.iter().any(|p| p.name == "sessionId" && !p.required));
        assert!(params.iter().any(|p| p.name == "data" && !p.required));
        assert!(params.iter().any(|p| p.name == "offset" && !p.required));
        assert!(params.iter().any(|p| p.name == "limit" && !p.required));
    }

    #[test]
    fn test_execute_command_params_with_background() {
        let params = execute_command_params();
        assert_eq!(params.len(), 5);
        assert!(params.iter().any(|p| p.name == "command" && p.required));
        assert!(params.iter().any(|p| p.name == "background" && !p.required));
        assert!(params.iter().any(|p| p.name == "yieldMs" && !p.required));
    }

    // ── memory_search ───────────────────────────────────────────────

    #[test]
    fn test_memory_search_params_defined() {
        let params = memory_search_params();
        assert_eq!(params.len(), 3);
        assert!(params.iter().any(|p| p.name == "query" && p.required));
        assert!(params.iter().any(|p| p.name == "maxResults" && !p.required));
        assert!(params.iter().any(|p| p.name == "minScore" && !p.required));
    }

    #[test]
    fn test_memory_search_missing_query() {
        let args = json!({});
        let result = exec_memory_search(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    // ── memory_get ──────────────────────────────────────────────────

    #[test]
    fn test_memory_get_params_defined() {
        let params = memory_get_params();
        assert_eq!(params.len(), 3);
        assert!(params.iter().any(|p| p.name == "path" && p.required));
        assert!(params.iter().any(|p| p.name == "from" && !p.required));
        assert!(params.iter().any(|p| p.name == "lines" && !p.required));
    }

    #[test]
    fn test_memory_get_missing_path() {
        let args = json!({});
        let result = exec_memory_get(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_memory_get_invalid_path() {
        let args = json!({ "path": "../etc/passwd" });
        let result = exec_memory_get(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a valid memory file"));
    }

    // ── cron ────────────────────────────────────────────────────────

    #[test]
    fn test_cron_params_defined() {
        let params = cron_params();
        assert_eq!(params.len(), 5);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
        assert!(params.iter().any(|p| p.name == "jobId" && !p.required));
    }

    #[test]
    fn test_cron_missing_action() {
        let args = json!({});
        let result = exec_cron(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_cron_invalid_action() {
        let args = json!({ "action": "invalid" });
        let result = exec_cron(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown action"));
    }

    // ── sessions_list ───────────────────────────────────────────────

    #[test]
    fn test_sessions_list_params_defined() {
        let params = sessions_list_params();
        assert_eq!(params.len(), 4);
        assert!(params.iter().all(|p| !p.required));
    }

    // ── sessions_spawn ──────────────────────────────────────────────

    #[test]
    fn test_sessions_spawn_params_defined() {
        let params = sessions_spawn_params();
        assert_eq!(params.len(), 7);
        assert!(params.iter().any(|p| p.name == "task" && p.required));
    }

    #[test]
    fn test_sessions_spawn_missing_task() {
        let args = json!({});
        let result = exec_sessions_spawn(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    // ── sessions_send ───────────────────────────────────────────────

    #[test]
    fn test_sessions_send_params_defined() {
        let params = sessions_send_params();
        assert_eq!(params.len(), 4);
        assert!(params.iter().any(|p| p.name == "message" && p.required));
    }

    #[test]
    fn test_sessions_send_missing_message() {
        let args = json!({});
        let result = exec_sessions_send(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    // ── sessions_history ────────────────────────────────────────────

    #[test]
    fn test_sessions_history_params_defined() {
        let params = sessions_history_params();
        assert_eq!(params.len(), 3);
        assert!(params.iter().any(|p| p.name == "sessionKey" && p.required));
    }

    // ── session_status ──────────────────────────────────────────────

    #[test]
    fn test_session_status_params_defined() {
        let params = session_status_params();
        assert_eq!(params.len(), 2);
        assert!(params.iter().all(|p| !p.required));
    }

    #[test]
    fn test_session_status_general() {
        let args = json!({});
        let result = exec_session_status(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Session Status"));
    }

    // ── agents_list ─────────────────────────────────────────────────

    #[test]
    fn test_agents_list_params_defined() {
        let params = agents_list_params();
        assert_eq!(params.len(), 0);
    }

    #[test]
    fn test_agents_list_returns_main() {
        let args = json!({});
        let result = exec_agents_list(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("main"));
    }

    // ── apply_patch ─────────────────────────────────────────────────

    #[test]
    fn test_apply_patch_params_defined() {
        let params = apply_patch_params();
        assert_eq!(params.len(), 3);
        assert!(params.iter().any(|p| p.name == "patch" && p.required));
        assert!(params.iter().any(|p| p.name == "dry_run" && !p.required));
    }

    #[test]
    fn test_apply_patch_missing_patch() {
        let args = json!({});
        let result = exec_apply_patch(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_parse_unified_diff() {
        let patch_str = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,4 @@
 line1
+new line
 line2
 line3
"#;
        let hunks = patch::parse_unified_diff(patch_str).unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file_path, "test.txt");
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[0].old_count, 3);
    }

    // ── secrets tools ───────────────────────────────────────────────

    #[test]
    fn test_secrets_stub_rejects() {
        let args = json!({});
        let result = exec_secrets_stub(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("gateway"));
    }

    #[test]
    fn test_is_secrets_tool() {
        assert!(is_secrets_tool("secrets_list"));
        assert!(is_secrets_tool("secrets_get"));
        assert!(is_secrets_tool("secrets_store"));
        assert!(!is_secrets_tool("read_file"));
        assert!(!is_secrets_tool("memory_get"));
    }

    #[test]
    fn test_secrets_list_params_defined() {
        let params = secrets_list_params();
        assert_eq!(params.len(), 1);
        assert!(params.iter().any(|p| p.name == "prefix" && !p.required));
    }

    #[test]
    fn test_secrets_get_params_defined() {
        let params = secrets_get_params();
        assert_eq!(params.len(), 1);
        assert!(params.iter().any(|p| p.name == "key" && p.required));
    }

    #[test]
    fn test_secrets_store_params_defined() {
        let params = secrets_store_params();
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "key" && p.required));
        assert!(params.iter().any(|p| p.name == "value" && p.required));
    }

    #[test]
    fn test_protected_path_without_init() {
        // Before set_credentials_dir is called, nothing is protected.
        assert!(!is_protected_path(Path::new("/some/random/path")));
    }

    // ── gateway ─────────────────────────────────────────────────────

    #[test]
    fn test_gateway_params_defined() {
        let params = gateway_params();
        assert_eq!(params.len(), 5);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
    }

    #[test]
    fn test_gateway_missing_action() {
        let args = json!({});
        let result = exec_gateway(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_gateway_config_schema() {
        let args = json!({ "action": "config.schema" });
        let result = exec_gateway(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("properties"));
    }

    // ── message ─────────────────────────────────────────────────────

    #[test]
    fn test_message_params_defined() {
        let params = message_params();
        assert_eq!(params.len(), 7);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
    }

    #[test]
    fn test_message_missing_action() {
        let args = json!({});
        let result = exec_message(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    // ── tts ─────────────────────────────────────────────────────────

    #[test]
    fn test_tts_params_defined() {
        let params = tts_params();
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "text" && p.required));
    }

    #[test]
    fn test_tts_missing_text() {
        let args = json!({});
        let result = exec_tts(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_tts_returns_media_path() {
        let args = json!({ "text": "Hello world" });
        let result = exec_tts(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("MEDIA:"));
    }

    // ── image ───────────────────────────────────────────────────────

    #[test]
    fn test_image_params_defined() {
        let params = image_params();
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "image" && p.required));
        assert!(params.iter().any(|p| p.name == "prompt" && !p.required));
    }

    #[test]
    fn test_image_missing_image() {
        let args = json!({});
        let result = exec_image(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_image_url_detection() {
        let args = json!({ "image": "https://example.com/photo.jpg" });
        let result = exec_image(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Is URL: true"));
    }

    // ── nodes ───────────────────────────────────────────────────────

    #[test]
    fn test_nodes_params_defined() {
        let params = nodes_params();
        assert_eq!(params.len(), 8);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
        assert!(params.iter().any(|p| p.name == "node" && !p.required));
    }

    #[test]
    fn test_nodes_missing_action() {
        let args = json!({});
        let result = exec_nodes(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_nodes_status() {
        let args = json!({ "action": "status" });
        let result = exec_nodes(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Node status"));
    }

    // ── browser ─────────────────────────────────────────────────────

    #[test]
    fn test_browser_params_defined() {
        let params = browser_params();
        assert_eq!(params.len(), 7);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
    }

    #[test]
    fn test_browser_missing_action() {
        let args = json!({});
        let result = exec_browser(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_browser_status() {
        let args = json!({ "action": "status" });
        let result = exec_browser(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Browser status"));
    }

    // ── canvas ──────────────────────────────────────────────────────

    #[test]
    fn test_canvas_params_defined() {
        let params = canvas_params();
        assert_eq!(params.len(), 6);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
    }

    #[test]
    fn test_canvas_missing_action() {
        let args = json!({});
        let result = exec_canvas(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_canvas_snapshot() {
        let args = json!({ "action": "snapshot" });
        let result = exec_canvas(&args, ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("canvas snapshot"));
    }

    // ── skill tools ─────────────────────────────────────────────────

    #[test]
    fn test_skill_list_params_defined() {
        let params = skill_list_params();
        assert_eq!(params.len(), 1);
        assert!(params.iter().any(|p| p.name == "filter" && !p.required));
    }

    #[test]
    fn test_skill_search_params_defined() {
        let params = skill_search_params();
        assert_eq!(params.len(), 1);
        assert!(params.iter().any(|p| p.name == "query" && p.required));
    }

    #[test]
    fn test_skill_install_params_defined() {
        let params = skill_install_params();
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "name" && p.required));
        assert!(params.iter().any(|p| p.name == "version" && !p.required));
    }

    #[test]
    fn test_skill_info_params_defined() {
        let params = skill_info_params();
        assert_eq!(params.len(), 1);
        assert!(params.iter().any(|p| p.name == "name" && p.required));
    }

    #[test]
    fn test_skill_enable_params_defined() {
        let params = skill_enable_params();
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "name" && p.required));
        assert!(params.iter().any(|p| p.name == "enabled" && p.required));
    }

    #[test]
    fn test_skill_link_secret_params_defined() {
        let params = skill_link_secret_params();
        assert_eq!(params.len(), 3);
        assert!(params.iter().any(|p| p.name == "action" && p.required));
        assert!(params.iter().any(|p| p.name == "skill" && p.required));
        assert!(params.iter().any(|p| p.name == "secret" && p.required));
    }

    #[test]
    fn test_skill_list_standalone_stub() {
        let result = exec_skill_list(&json!({}), ws());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("standalone mode"));
    }

    #[test]
    fn test_skill_search_missing_query() {
        let result = exec_skill_search(&json!({}), ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_skill_install_missing_name() {
        let result = exec_skill_install(&json!({}), ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_skill_info_missing_name() {
        let result = exec_skill_info(&json!({}), ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required parameter"));
    }

    #[test]
    fn test_skill_enable_missing_params() {
        let result = exec_skill_enable(&json!({}), ws());
        assert!(result.is_err());
    }

    #[test]
    fn test_skill_link_secret_bad_action() {
        let args = json!({ "action": "nope", "skill": "x", "secret": "y" });
        let result = exec_skill_link_secret(&args, ws());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown action"));
    }

    #[test]
    fn test_is_skill_tool() {
        assert!(is_skill_tool("skill_list"));
        assert!(is_skill_tool("skill_search"));
        assert!(is_skill_tool("skill_install"));
        assert!(is_skill_tool("skill_info"));
        assert!(is_skill_tool("skill_enable"));
        assert!(is_skill_tool("skill_link_secret"));
        assert!(!is_skill_tool("read_file"));
        assert!(!is_skill_tool("secrets_list"));
    }
}
