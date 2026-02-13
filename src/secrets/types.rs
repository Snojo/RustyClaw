//! Type definitions for the secrets module.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── Credential types ────────────────────────────────────────────────────────

/// What kind of secret a credential entry holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    /// Bearer / API token (single opaque string).
    ApiKey,
    /// HTTP passkey (WebAuthn-style credential id + secret).
    HttpPasskey,
    /// Username + password pair.
    UsernamePassword,
    /// SSH keypair (Ed25519).  Both keys are stored encrypted in the vault.
    SshKey,
    /// Generic single-value token (OAuth tokens, bot tokens, etc.).
    Token,
    /// Form autofill data — arbitrary key/value pairs for filling web
    /// forms (name, address, email, phone, etc.).
    FormAutofill,
    /// Payment method — credit/debit card details.
    PaymentMethod,
    /// Free-form encrypted note (recovery codes, license keys,
    /// security questions, PIN codes, etc.).
    SecureNote,
    /// Catch-all for anything that doesn't fit the above.
    Other,
}

impl std::fmt::Display for SecretKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey => write!(f, "API Key"),
            Self::HttpPasskey => write!(f, "HTTP Passkey"),
            Self::UsernamePassword => write!(f, "Login"),
            Self::SshKey => write!(f, "SSH Key"),
            Self::Token => write!(f, "Token"),
            Self::FormAutofill => write!(f, "Form"),
            Self::PaymentMethod => write!(f, "Payment"),
            Self::SecureNote => write!(f, "Note"),
            Self::Other => write!(f, "Other"),
        }
    }
}

impl SecretKind {
    /// A single-character icon suitable for the TUI list.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::ApiKey => "🔑",
            Self::HttpPasskey => "🌐",
            Self::UsernamePassword => "👤",
            Self::SshKey => "🔐",
            Self::Token => "🎫",
            Self::FormAutofill => "📋",
            Self::PaymentMethod => "💳",
            Self::SecureNote => "📝",
            Self::Other => "🔒",
        }
    }
}

/// Controls *when* the agent is allowed to read a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessPolicy {
    /// The agent may read this secret at any time without prompting.
    Always,
    /// The agent may read this secret only with explicit per-use user
    /// approval (e.g. a "yes/no" confirmation in the TUI).
    WithApproval,
    /// The agent must re-authenticate (vault password and/or TOTP)
    /// before each access.
    WithAuth,
    /// The secret is only available when the agent is executing one of
    /// the named skills.  An empty list means "no skill may access it"
    /// (effectively locked).
    SkillOnly(Vec<String>),
}

impl Default for AccessPolicy {
    fn default() -> Self {
        Self::WithApproval
    }
}

impl std::fmt::Display for AccessPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Always => write!(f, "always"),
            Self::WithApproval => write!(f, "approval"),
            Self::WithAuth => write!(f, "auth"),
            Self::SkillOnly(skills) => {
                if skills.is_empty() {
                    write!(f, "locked")
                } else {
                    write!(f, "skills: {}", skills.join(", "))
                }
            }
        }
    }
}

impl AccessPolicy {
    /// Short badge-style label for the TUI.
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Always => "OPEN",
            Self::WithApproval => "ASK",
            Self::WithAuth => "AUTH",
            Self::SkillOnly(_) => "SKILL",
        }
    }
}

/// Metadata envelope stored alongside the secret value(s) in the vault.
///
/// This is JSON-serialized and stored under the key `cred:<name>`.
/// The actual sensitive values live under `val:<name>` (and for
/// `UsernamePassword`, also `val:<name>:user`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    /// Human-readable label (e.g. "Anthropic API key").
    pub label: String,
    /// What kind of credential this is.
    pub kind: SecretKind,
    /// Who (or what) is allowed to read the secret.
    pub policy: AccessPolicy,
    /// Optional free-form description / notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When true, the credential is listed but the agent cannot read
    /// its value.  The user can re-enable it from the TUI.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

/// The result of reading a credential — includes the metadata envelope
/// plus the decrypted value(s).
#[derive(Debug, Clone)]
pub enum CredentialValue {
    /// A single opaque string (ApiKey, Token, HttpPasskey, Other).
    Single(String),
    /// Username + password pair.
    UserPass { username: String, password: String },
    /// SSH keypair — private key in OpenSSH PEM format, public key in
    /// `ssh-ed25519 AAAA…` format.
    SshKeyPair { private_key: String, public_key: String },
    /// Arbitrary key/value pairs (form autofill fields).
    FormFields(BTreeMap<String, String>),
    /// Payment card details.
    PaymentCard {
        cardholder: String,
        number: String,
        expiry: String,
        cvv: String,
        /// Optional billing-address / notes fields.
        extra: BTreeMap<String, String>,
    },
}

/// Context supplied by the caller when requesting access to a
/// credential.  The [`SecretsManager`] evaluates this against the
/// credential's [`AccessPolicy`].
#[derive(Debug, Clone, Default)]
pub struct AccessContext {
    /// The user explicitly approved this specific access.
    pub user_approved: bool,
    /// The caller has re-verified the vault password and/or TOTP
    /// within this request.
    pub authenticated: bool,
    /// The name of the skill currently being executed, if any.
    pub active_skill: Option<String>,
}

/// Kept for backward compatibility with older code that references this type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub key: String,
    pub description: Option<String>,
}
