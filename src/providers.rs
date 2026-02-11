//! Shared provider catalogue.
//!
//! Single source of truth for supported providers, their secret key names,
//! base URLs, and available models.  Used by both the onboarding wizard and
//! the TUI `/provider` + `/model` commands.

/// A provider definition with its secret key name and available models.
pub struct ProviderDef {
    pub id: &'static str,
    pub display: &'static str,
    /// Name of the secret that holds the API key (e.g. `"ANTHROPIC_API_KEY"`).
    /// `None` means the provider does not require a key (e.g. Ollama).
    pub secret_key: Option<&'static str>,
    pub base_url: Option<&'static str>,
    pub models: &'static [&'static str],
}

pub const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "anthropic",
        display: "Anthropic (Claude)",
        secret_key: Some("ANTHROPIC_API_KEY"),
        base_url: Some("https://api.anthropic.com"),
        models: &[
            "claude-opus-4-20250514",
            "claude-sonnet-4-20250514",
            "claude-haiku-4-20250514",
        ],
    },
    ProviderDef {
        id: "openai",
        display: "OpenAI (GPT / o-series)",
        secret_key: Some("OPENAI_API_KEY"),
        base_url: Some("https://api.openai.com/v1"),
        models: &[
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "o3",
            "o4-mini",
        ],
    },
    ProviderDef {
        id: "google",
        display: "Google (Gemini)",
        secret_key: Some("GEMINI_API_KEY"),
        base_url: Some("https://generativelanguage.googleapis.com/v1beta"),
        models: &[
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.0-flash",
        ],
    },
    ProviderDef {
        id: "xai",
        display: "xAI (Grok)",
        secret_key: Some("XAI_API_KEY"),
        base_url: Some("https://api.x.ai/v1"),
        models: &["grok-3", "grok-3-mini"],
    },
    ProviderDef {
        id: "openrouter",
        display: "OpenRouter",
        secret_key: Some("OPENROUTER_API_KEY"),
        base_url: Some("https://openrouter.ai/api/v1"),
        models: &[
            "anthropic/claude-opus-4-20250514",
            "anthropic/claude-sonnet-4-20250514",
            "openai/gpt-4.1",
            "google/gemini-2.5-pro",
        ],
    },
    ProviderDef {
        id: "ollama",
        display: "Ollama (local)",
        secret_key: None,
        base_url: Some("http://localhost:11434/v1"),
        models: &["llama3.1", "mistral", "codellama", "deepseek-coder"],
    },
    ProviderDef {
        id: "custom",
        display: "Custom / OpenAI-compatible endpoint",
        secret_key: Some("CUSTOM_API_KEY"),
        base_url: None, // will prompt
        models: &[],
    },
];

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Look up a provider by ID.
pub fn provider_by_id(id: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Return the secret-key name for the given provider ID, or `None` if the
/// provider doesn't require one (e.g. Ollama).
pub fn secret_key_for_provider(id: &str) -> Option<&'static str> {
    provider_by_id(id).and_then(|p| p.secret_key)
}

/// Return the display name for the given provider ID.
pub fn display_name_for_provider(id: &str) -> &str {
    provider_by_id(id).map(|p| p.display).unwrap_or(id)
}

/// Return all provider IDs.
pub fn provider_ids() -> Vec<&'static str> {
    PROVIDERS.iter().map(|p| p.id).collect()
}

/// Return all model names across all providers (for tab-completion).
pub fn all_model_names() -> Vec<&'static str> {
    PROVIDERS.iter().flat_map(|p| p.models.iter().copied()).collect()
}

/// Return the models for the given provider ID.
pub fn models_for_provider(id: &str) -> &'static [&'static str] {
    provider_by_id(id).map(|p| p.models).unwrap_or(&[])
}

/// Return the base URL for the given provider ID.
pub fn base_url_for_provider(id: &str) -> Option<&'static str> {
    provider_by_id(id).and_then(|p| p.base_url)
}

// ── Dynamic model fetching ──────────────────────────────────────────────────

/// Fetch the list of available models from a provider's API.
///
/// Returns `Err` with a human-readable message on any failure — no silent
/// fallbacks.  Callers should display the error to the user.
pub async fn fetch_models(
    provider_id: &str,
    api_key: Option<&str>,
    base_url_override: Option<&str>,
) -> Result<Vec<String>, String> {
    let def = match provider_by_id(provider_id) {
        Some(d) => d,
        None => return Err(format!("Unknown provider: {}", provider_id)),
    };

    let base = base_url_override
        .or(def.base_url)
        .unwrap_or("");

    if base.is_empty() {
        return Err(format!(
            "No base URL configured for {}. Set one in config.toml or use /provider.",
            def.display,
        ));
    }

    // Anthropic has no public models endpoint
    if provider_id == "anthropic" {
        return Err(format!(
            "Anthropic does not provide a models API. Set a model manually with /model <name>.",
        ));
    }

    let result = match provider_id {
        // Google Gemini uses a different response shape
        "google" => fetch_google_models(base, api_key).await,
        // Ollama — no auth needed, OpenAI-compatible /v1/models
        "ollama" => fetch_openai_compatible_models(base, None).await,
        // Everything else is OpenAI-compatible
        _ => fetch_openai_compatible_models(base, api_key).await,
    };

    match result {
        Ok(models) if models.is_empty() => Err(format!(
            "The {} API returned an empty model list.",
            def.display,
        )),
        Ok(models) => Ok(models),
        Err(e) => Err(format!("Failed to fetch models from {}: {}", def.display, e)),
    }
}

/// Fetch from an OpenAI-compatible `/models` endpoint.
///
/// Works for OpenAI, xAI, OpenRouter, Ollama, and custom providers.
async fn fetch_openai_compatible_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, reqwest::Error> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let mut req = client.get(&url);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }

    let resp = req.send().await?.error_for_status()?;
    let body: serde_json::Value = resp.json().await?;

    let models = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(models)
}

/// Fetch from the Google Gemini `/models` endpoint.
async fn fetch_google_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, reqwest::Error> {
    let key = match api_key {
        Some(k) => k,
        // No key — return empty so the outer match produces a clear error
        None => return Ok(Vec::new()),
    };

    let url = format!(
        "{}/models?key={}",
        base_url.trim_end_matches('/'),
        key,
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client.get(&url).send().await?.error_for_status()?;
    let body: serde_json::Value = resp.json().await?;

    let models = body
        .get("models")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m.get("name")
                        .and_then(|v| v.as_str())
                        // API returns "models/gemini-2.5-pro" — strip the prefix
                        .map(|s| s.strip_prefix("models/").unwrap_or(s).to_string())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(models)
}
