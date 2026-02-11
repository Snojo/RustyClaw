//! Interactive onboarding wizard.
//!
//! Mirrors the openclaw `onboard` command: walks the user through selecting a
//! model provider, storing an API key, picking a default model, and
//! initialising the SOUL.

use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use crossterm::terminal;

use crate::config::{Config, ModelProvider};
use crate::secrets::SecretsManager;
use crate::soul::{SoulManager, DEFAULT_SOUL_CONTENT};
use crate::theme as t;

// ── Provider catalogue ──────────────────────────────────────────────────────

/// A provider definition with its secret key name and available models.
struct ProviderDef {
    id: &'static str,
    display: &'static str,
    secret_key: Option<&'static str>,
    base_url: Option<&'static str>,
    models: &'static [&'static str],
}

const PROVIDERS: &[ProviderDef] = &[
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
        models: &[
            "llama3.1",
            "mistral",
            "codellama",
            "deepseek-coder",
        ],
    },
    ProviderDef {
        id: "custom",
        display: "Custom / OpenAI-compatible endpoint",
        secret_key: Some("CUSTOM_API_KEY"),
        base_url: None, // will prompt
        models: &[],
    },
];

// ── Public entry point ──────────────────────────────────────────────────────

/// Run the interactive onboarding wizard, mutating `config` in place and
/// storing secrets.  Returns `true` if the user completed onboarding.
pub fn run_onboard_wizard(
    config: &mut Config,
    secrets: &mut SecretsManager,
    reset: bool,
) -> Result<bool> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    println!();
    t::print_header("🦀  RustyClaw Onboarding  🦀");
    println!();

    // ── Optional reset ─────────────────────────────────────────────
    if reset {
        println!("{}\n", t::warn("Resetting configuration…"));
        *config = Config::default();
    }

    // ── 1. Select model provider ───────────────────────────────────
    println!("{}", t::heading("Select a model provider:"));
    println!();
    for (i, p) in PROVIDERS.iter().enumerate() {
        println!("  {}. {}", t::accent_bright(&format!("{}", i + 1)), p.display);
    }
    println!();

    let provider = loop {
        let choice = prompt_line(&mut reader, &format!("{} ", t::accent(&format!("Provider [1-{}]:", PROVIDERS.len()))))?;
        if let Ok(n) = choice.trim().parse::<usize>() {
            if n >= 1 && n <= PROVIDERS.len() {
                break &PROVIDERS[n - 1];
            }
        }
        println!("  {}", t::warn(&format!("Please enter a number between 1 and {}.", PROVIDERS.len())));
    };

    println!();
    println!("  {}", t::icon_ok(&format!("Selected: {}", t::accent_bright(provider.display))));
    println!();

    // ── 1b. Optional password for secrets vault ────────────────────
    let vault_exists = config.credentials_dir().join("secrets.json").exists();
    if !vault_exists {
        println!("{}", t::bold("You can protect your secrets vault with a password."));
        println!("{}", t::muted("If you skip this, a key file will be generated instead."));
        println!();
        println!("  {}  If you set a password you will need to enter it every", t::warn("⚠"));
        println!("     time RustyClaw starts, including when the gateway is");
        println!("     launched.  Automated / unattended starts will not be");
        println!("     possible without the password.");
        println!();

        let pw = prompt_secret(&mut reader, &format!("{} ", t::accent("Vault password (leave blank to skip):")))?;
        let pw = pw.trim().to_string();

        if pw.is_empty() {
            println!("  {}", t::icon_ok("Using auto-generated key file (no password)."));
            config.secrets_password_protected = false;
        } else {
            loop {
                let confirm = prompt_secret(&mut reader, &format!("{} ", t::accent("Confirm password:")))?;
                if confirm.trim() == pw {
                    secrets.set_password(pw);
                    config.secrets_password_protected = true;
                    println!("  {}", t::icon_ok("Secrets vault will be password-protected."));
                    break;
                }
                println!("  {}", t::icon_warn("Passwords do not match — please try again."));
            }
        }
        println!();
    } else if config.secrets_password_protected {
        // Vault already exists with a password — make sure SecretsManager has it.
        let pw = prompt_secret(&mut reader, &format!("{} ", t::accent("Enter vault password:")))?;
        secrets.set_password(pw.trim().to_string());
        println!();
    }

    // ── 2. API key ─────────────────────────────────────────────────
    if let Some(secret_key) = provider.secret_key {
        // Check if we already have one stored.
        let existing = secrets.get_secret(secret_key, true)?;
        if let Some(_) = &existing {
            let reuse = prompt_line(
                &mut reader,
                &format!("{} ", t::accent(&format!("An API key for {} is already stored. Keep it? [Y/n]:", provider.display))),
            )?;
            if reuse.trim().eq_ignore_ascii_case("n") {
                let key = prompt_secret(&mut reader, &format!("{} ", t::accent("Enter API key:")))?;
                if key.trim().is_empty() {
                    println!("  {}", t::icon_warn("No key entered — keeping existing key."));
                } else {
                    secrets.store_secret(secret_key, key.trim())?;
                    println!("  {}", t::icon_ok("API key updated."));
                }
            } else {
                println!("  {}", t::icon_ok("Keeping existing API key."));
            }
        } else {
            let key = prompt_secret(&mut reader, &format!("{} ", t::accent("Enter API key:")))?;
            if key.trim().is_empty() {
                println!("  {}", t::icon_warn("No key entered — you can add one later with:"));
                println!("      {}", t::accent_bright("rustyclaw onboard"));
            } else {
                secrets.store_secret(secret_key, key.trim())?;
                println!("  {}", t::icon_ok("API key stored securely."));
            }
        }
        println!();
    }

    // ── 3. Base URL (only for custom) ──────────────────────────────
    let base_url: String = if provider.id == "custom" {
        let url = prompt_line(&mut reader, &format!("{} ", t::accent("Base URL (OpenAI-compatible):")))?;
        let url = url.trim().to_string();
        if url.is_empty() {
            println!("  {}", t::icon_warn("No URL entered. You can set model.base_url in config.toml later."));
            String::new()
        } else {
            println!("  {}", t::icon_ok(&format!("Base URL: {}", t::info(&url))));
            url
        }
    } else {
        provider.base_url.unwrap_or("").to_string()
    };

    // ── 4. Select a model ──────────────────────────────────────────
    let model: String = if provider.models.is_empty() {
        // Custom provider — ask for a model name.
        let m = prompt_line(&mut reader, &format!("{} ", t::accent("Model name:")))?;
        m.trim().to_string()
    } else {
        println!("{}", t::heading("Select a default model:"));
        println!();
        for (i, m) in provider.models.iter().enumerate() {
            println!("  {}. {}", t::accent_bright(&format!("{}", i + 1)), m);
        }
        println!();

        loop {
            let choice = prompt_line(
                &mut reader,
                &format!("{} ", t::accent(&format!("Model [1-{}]:", provider.models.len()))),
            )?;
            if let Ok(n) = choice.trim().parse::<usize>() {
                if n >= 1 && n <= provider.models.len() {
                    break provider.models[n - 1].to_string();
                }
            }
            println!(
                "  {}",
                t::warn(&format!("Please enter a number between 1 and {}.", provider.models.len()))
            );
        }
    };

    if !model.is_empty() {
        println!();
        println!("  {}", t::icon_ok(&format!("Default model: {}", t::accent_bright(&model))));
    }

    // ── 5. Initialize / update SOUL.md ─────────────────────────────
    println!();
    let soul_path = config.soul_path();

    // Only prompt if the file exists AND has been customised (differs from the
    // default template).  A previous `rustyclaw tui` run may have already
    // created the default SOUL.md — that shouldn't count as "already exists".
    let soul_customised = soul_path.exists()
        && std::fs::read_to_string(&soul_path)
            .map(|c| c != DEFAULT_SOUL_CONTENT)
            .unwrap_or(false);

    let init_soul = if soul_customised {
        let answer = prompt_line(
            &mut reader,
            &format!("{} ", t::accent("SOUL.md has been customised. Reset to default? [y/N]:")),
        )?;
        answer.trim().eq_ignore_ascii_case("y")
    } else {
        true
    };

    if init_soul {
        let mut soul = SoulManager::new(soul_path.clone());
        soul.load()?;
        println!("  {}", t::icon_ok(&format!("SOUL.md initialised at {}", t::info(&soul_path.display().to_string()))));
    } else {
        println!("  {}", t::icon_ok("Keeping existing SOUL.md"));
    }

    // ── 6. Write config ────────────────────────────────────────────
    config.model = Some(ModelProvider {
        provider: provider.id.to_string(),
        model: if model.is_empty() {
            None
        } else {
            Some(model)
        },
        base_url: if base_url.is_empty() {
            None
        } else {
            Some(base_url)
        },
    });

    // Ensure the full directory skeleton exists and save.
    config.ensure_dirs()
        .context("Failed to create directory structure")?;
    config.save(None)?;

    t::print_header("Onboarding complete! 🎉");
    println!(
        "  {}",
        t::icon_ok(&format!("Config saved to {}",
            t::info(&config.settings_dir.join("config.toml").display().to_string())
        ))
    );
    println!("  Run {} to start the TUI.", t::accent_bright("`rustyclaw tui`"));
    println!();

    Ok(true)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn prompt_line(reader: &mut impl BufRead, prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut buf = String::new();
    reader.read_line(&mut buf)?;
    Ok(buf.trim_end_matches('\n').trim_end_matches('\r').to_string())
}

fn prompt_secret(_reader: &mut impl BufRead, prompt: &str) -> Result<String> {
    use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

    print!("{}", prompt);
    io::stdout().flush()?;

    // Enable raw mode to suppress echo and line buffering.
    terminal::enable_raw_mode()?;

    let result = (|| -> Result<String> {
        let mut buf = String::new();
        loop {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                match code {
                    KeyCode::Enter => break,
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        anyhow::bail!("Interrupted");
                    }
                    KeyCode::Backspace => {
                        buf.pop();
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                    }
                    _ => {}
                }
            }
        }
        Ok(buf)
    })();

    // Always restore cooked mode, even on error.
    let _ = terminal::disable_raw_mode();
    // Print newline since Enter was consumed without echo.
    println!();

    result
}
