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
