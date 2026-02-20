# RustyClaw Learning & Evolution: Analysis and Recommendations

Based on examining both OpenClaw's implementation and RustyClaw's current state.

---

## Executive Summary

RustyClaw has solid foundations but is missing several key mechanisms that make OpenClaw effective at learning, task management, and personality evolution. The biggest gaps are:

1. **No pre-compaction memory flush** — memories can be lost during context compaction
2. **No heartbeat system** — no periodic self-check capability
3. **No vector/semantic search** — only BM25 keyword matching
4. **No workspace file injection** — SOUL.md, MEMORY.md not automatically included in prompts
5. **No session transcript indexing** — can't search past conversations

---

## Detailed Gap Analysis

### 1. Memory Persistence & Compaction

#### OpenClaw Has:
- **Pre-compaction memory flush**: Silent agent turn that runs before compaction, prompting the agent to write durable memories to disk
- **Configurable thresholds**: `softThresholdTokens`, `reserveTokensFloor`
- **Automatic prompt injection**: System message + user message remind agent to flush

```json5
compaction: {
  memoryFlush: {
    enabled: true,
    softThresholdTokens: 4000,
    systemPrompt: "Session nearing compaction. Store durable memories now.",
    prompt: "Write any lasting notes to memory/YYYY-MM-DD.md; reply with NO_REPLY if nothing to store."
  }
}
```

#### RustyClaw Has:
- Context compaction at 75% of window ✓
- No pre-flush mechanism ✗

#### Recommendation:
**Add `MemoryFlush` module** that triggers an agent turn when approaching compaction threshold. This is critical — without it, important context gets lost.

```rust
// src/compaction.rs (new)
pub struct MemoryFlush {
    enabled: bool,
    soft_threshold_tokens: usize,
    system_prompt: String,
    user_prompt: String,
}

impl MemoryFlush {
    pub fn should_flush(&self, current_tokens: usize, max_tokens: usize, reserve: usize) -> bool {
        let threshold = max_tokens - reserve - self.soft_threshold_tokens;
        current_tokens >= threshold
    }
    
    pub async fn trigger_flush(&self, session: &mut Session) -> Result<()> {
        // Inject system + user prompts, run agent turn, expect NO_REPLY
    }
}
```

---

### 2. Heartbeat System

#### OpenClaw Has:
- Periodic agent turns at configurable intervals (default 30min)
- `HEARTBEAT.md` file for agent-editable task checklist
- Active hours configuration
- Delivery target configuration
- Smart suppression (`HEARTBEAT_OK` = no delivery)

```json5
heartbeat: {
  every: "30m",
  target: "last",
  activeHours: { start: "08:00", end: "22:00" }
}
```

#### RustyClaw Has:
- Cron jobs ✓ (can achieve similar results)
- No native heartbeat ✗
- No `HEARTBEAT.md` convention ✗

#### Recommendation:
**Add native `Heartbeat` system** as sugar over cron. Benefits:
- Simpler configuration
- Built-in `HEARTBEAT.md` injection
- Active hours enforcement
- Main session context preservation

```rust
// src/heartbeat.rs (new)
pub struct Heartbeat {
    interval: Duration,
    active_hours: Option<(NaiveTime, NaiveTime)>,
    target: HeartbeatTarget,
}

impl Heartbeat {
    pub async fn tick(&self, session: &mut Session) -> Result<HeartbeatResult> {
        // 1. Check active hours
        // 2. Read HEARTBEAT.md if exists
        // 3. Inject prompt
        // 4. Run agent turn
        // 5. Return HEARTBEAT_OK or alert
    }
}
```

Alternatively, document how to achieve this with cron:
```bash
rustyclaw cron add --name "Heartbeat" --every "30m" --session main \
  --system-event "Read HEARTBEAT.md and check on things. Reply HEARTBEAT_OK if nothing needs attention."
```

---

### 3. Vector/Semantic Memory Search

#### OpenClaw Has:
- Local embeddings (`node-llama-cpp` + GGUF models)
- Remote embeddings (OpenAI, Gemini, Voyage)
- Hybrid search (BM25 + vector, weighted merge)
- MMR diversity re-ranking
- Temporal decay (recency boost)
- Embedding cache (SQLite)
- Session transcript indexing
- sqlite-vec acceleration

#### RustyClaw Has:
- BM25 keyword search only ✓
- Basic chunking ✓
- No embeddings ✗
- No hybrid search ✗
- No recency weighting ✗

#### Recommendation:
**Phase 1: Add recency weighting to BM25** (quick win)

```rust
fn score_with_recency(&self, base_score: f64, file_path: &str) -> f64 {
    let age_days = extract_date_from_path(file_path)
        .map(|d| (Utc::now().date_naive() - d).num_days())
        .unwrap_or(0);
    
    let half_life = 30.0;
    let decay = (-0.693 * age_days as f64 / half_life).exp();
    
    base_score * decay
}
```

**Phase 2: Add local embeddings via `fastembed-rs`**

```rust
// Cargo.toml
fastembed = "0.4"

// src/memory.rs
use fastembed::TextEmbedding;

pub struct VectorIndex {
    embedder: TextEmbedding,
    chunks: Vec<(MemoryChunk, Vec<f32>)>,
}

impl VectorIndex {
    pub fn new() -> Result<Self> {
        // fastembed auto-downloads models
        let embedder = TextEmbedding::try_new(Default::default())?;
        Ok(Self { embedder, chunks: Vec::new() })
    }
    
    pub fn embed_chunk(&self, text: &str) -> Vec<f32> {
        self.embedder.embed(vec![text], None)
            .unwrap()[0].clone()
    }
    
    pub fn search(&self, query: &str, k: usize) -> Vec<SearchResult> {
        let query_vec = self.embed_chunk(query);
        // Cosine similarity search
    }
}
```

**Phase 3: Hybrid BM25 + Vector**

Combine both signals with configurable weights (default 0.7 vector, 0.3 keyword).

---

### 4. Workspace File Injection

#### OpenClaw Has:
- Automatic injection of workspace files into system prompt:
  - `SOUL.md` — personality
  - `MEMORY.md` — long-term memory (main session only)
  - `AGENTS.md` — behavior guidelines
  - `TOOLS.md` — tool usage notes
  - `IDENTITY.md` — agent identity
  - `USER.md` — user profile
  - `HEARTBEAT.md` — periodic task checklist
- Configurable injection order and scope
- Security: `MEMORY.md` only in direct/main session (not group chats)

#### RustyClaw Has:
- `SoulManager` loads `SOUL.md` ✓
- No automatic prompt injection ✗
- No support for other workspace files ✗

#### Recommendation:
**Add `WorkspaceContext` that builds system prompt from workspace files**

```rust
// src/workspace_context.rs (new)
pub struct WorkspaceContext {
    workspace_dir: PathBuf,
    session_type: SessionType, // Main, Group, etc.
}

impl WorkspaceContext {
    pub fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();
        
        // Always include
        if let Ok(soul) = self.read_file("SOUL.md") {
            parts.push(format!("## SOUL.md\n{}", soul));
        }
        if let Ok(agents) = self.read_file("AGENTS.md") {
            parts.push(format!("## AGENTS.md\n{}", agents));
        }
        if let Ok(tools) = self.read_file("TOOLS.md") {
            parts.push(format!("## TOOLS.md\n{}", tools));
        }
        
        // Main session only (privacy)
        if self.session_type == SessionType::Main {
            if let Ok(memory) = self.read_file("MEMORY.md") {
                parts.push(format!("## MEMORY.md\n{}", memory));
            }
            if let Ok(user) = self.read_file("USER.md") {
                parts.push(format!("## USER.md\n{}", user));
            }
        }
        
        parts.join("\n\n---\n\n")
    }
}
```

---

### 5. Session Transcript Memory

#### OpenClaw Has:
- Optional session transcript indexing
- Delta-based sync (not every message)
- `memory_search` can query past conversations
- Privacy scoping (per-session control)

#### RustyClaw Has:
- Conversation persistence ✓
- No transcript indexing for search ✗

#### Recommendation:
**Add session transcript export to searchable format**

```rust
// src/sessions.rs additions
impl Session {
    pub fn export_for_indexing(&self) -> String {
        self.messages
            .iter()
            .filter(|m| matches!(m.role, Role::User | Role::Assistant))
            .map(|m| format!("[{}] {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
```

Then index these alongside memory files for `memory_search`.

---

### 6. Learning from Corrections

#### OpenClaw Approach:
- Agent is instructed to update workspace files when learning
- `AGENTS.md` tells agent: "When you make a mistake → document it so future-you doesn't repeat it"
- Memory files are the learning substrate

#### RustyClaw Has:
- No explicit guidance on self-improvement

#### Recommendation:
**Update default `SOUL.md` content** to include learning guidance:

```markdown
## Learning

When you make mistakes:
1. Acknowledge the error
2. Update relevant workspace files (TOOLS.md, memory/) to prevent repetition
3. Improve your future behavior

"Text > Brain" — if you want to remember something, write it to a file.
```

---

### 7. Startup File Loading

#### OpenClaw Has:
- Required file audit on session start
- Automatic loading of `memory/YYYY-MM-DD.md` (today + yesterday)
- Post-compaction audit to reload files
- `WORKFLOW_AUTO.md` for automated behaviors

#### RustyClaw Has:
- No automatic memory file loading at startup ✗

#### Recommendation:
**Add startup hook in session initialization**

```rust
impl Session {
    pub async fn initialize(&mut self) -> Result<()> {
        // Load today's memory file
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let today_path = format!("memory/{}.md", today);
        if self.workspace.join(&today_path).exists() {
            let content = fs::read_to_string(self.workspace.join(&today_path))?;
            self.inject_context(&format!("## Today's Notes ({})\n{}", today, content));
        }
        
        // Load MEMORY.md for main session
        if self.session_type == SessionType::Main {
            if let Ok(memory) = fs::read_to_string(self.workspace.join("MEMORY.md")) {
                self.inject_context(&format!("## MEMORY.md\n{}", memory));
            }
        }
        
        Ok(())
    }
}
```

---

## Priority Implementation Order

### Phase 1: Critical (Memory Safety)
1. **Pre-compaction memory flush** — prevents memory loss
2. **Workspace file injection** — enables personality/learning

### Phase 2: Important (Learning)
3. **Recency-weighted BM25** — quick improvement to search relevance
4. **Startup memory loading** — ensures continuity across sessions
5. **Update default SOUL.md** — guide agent self-improvement

### Phase 3: Enhancement (Quality)
6. **Local vector embeddings** — semantic search capability
7. **Hybrid search** — combine keyword + semantic
8. **Session transcript indexing** — search past conversations

### Phase 4: Polish (UX)
9. **Native heartbeat system** — simpler than cron for periodic checks
10. **MMR diversity + temporal decay** — better search ranking

---

## Implementation Effort Estimates

| Feature | Effort | Impact | Priority |
|---------|--------|--------|----------|
| Pre-compaction flush | 2-3 days | Critical | P0 |
| Workspace file injection | 1-2 days | High | P0 |
| Recency-weighted BM25 | 0.5 days | Medium | P1 |
| Startup memory loading | 1 day | High | P1 |
| Update SOUL.md defaults | 0.5 days | Medium | P1 |
| Local vector embeddings | 3-5 days | High | P2 |
| Hybrid search | 2-3 days | Medium | P2 |
| Session transcript indexing | 2-3 days | Medium | P2 |
| Native heartbeat | 2-3 days | Medium | P3 |
| MMR + temporal decay | 1-2 days | Low | P3 |

---

## Crate Recommendations

| Capability | Crate | Notes |
|------------|-------|-------|
| Local embeddings | `fastembed` | Pure Rust, auto-downloads models |
| Vector DB | `qdrant-client` or inline cosine | Start simple |
| Cron parsing | `cron` | Already have this |
| Date handling | `chrono` | Already have this |

---

## Summary

RustyClaw has the foundations but needs **memory flush before compaction** and **workspace file injection** to truly learn and maintain personality. These should be P0.

Adding **semantic search** (Phase 2-3) will significantly improve memory retrieval quality, especially as memory files grow.

The **heartbeat system** is nice-to-have since cron can achieve similar results, but native support would be more user-friendly.
