use anyhow::Result;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use ratatui::prelude::*;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

use crate::action::Action;
use crate::commands::{handle_command, CommandAction, CommandContext};
use crate::config::Config;
use crate::gateway::{run_gateway, GatewayOptions};
use crate::pages::home::Home;
use crate::pages::Page;
use crate::panes::footer::FooterPane;
use crate::panes::header::HeaderPane;
use crate::panes::{GatewayStatus, InputMode, Pane, PaneState};
use crate::providers;
use crate::secrets::SecretsManager;
use crate::skills::SkillManager;
use crate::soul::SoulManager;
use crate::tui::{Event, EventResponse, Tui};

/// Type alias for the client-side WebSocket write half.
type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// Phase of the API-key dialog overlay.
#[derive(Debug, Clone, PartialEq)]
enum ApiKeyDialogPhase {
    /// Prompting the user to enter an API key (text is masked)
    EnterKey,
    /// Asking whether to store the entered key permanently
    ConfirmStore,
}

/// Spinner state shown while fetching models from a provider API.
struct FetchModelsLoading {
    /// Display name of the provider
    display: String,
    /// Tick counter for the spinner animation
    tick: usize,
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// State for the model-selector dialog overlay.
struct ModelSelectorState {
    /// Provider this selection is for
    provider: String,
    /// Display name
    display: String,
    /// Available model names
    models: Vec<String>,
    /// Currently highlighted index
    selected: usize,
    /// Scroll offset when the list is longer than the dialog
    scroll_offset: usize,
}

/// State for the provider-selector dialog overlay.
struct ProviderSelectorState {
    /// Provider entries: (id, display)
    providers: Vec<(String, String)>,
    /// Currently highlighted index
    selected: usize,
    /// Scroll offset
    scroll_offset: usize,
}

/// Shared state that is separate from the UI components so we can borrow both
/// independently.
struct SharedState {
    config: Config,
    messages: Vec<String>,
    input_mode: InputMode,
    secrets_manager: SecretsManager,
    skill_manager: SkillManager,
    soul_manager: SoulManager,
    gateway_status: GatewayStatus,
    /// Animated loading line shown at the bottom of the messages list.
    loading_line: Option<String>,
}

impl SharedState {
    fn pane_state(&mut self) -> PaneState<'_> {
        PaneState {
            config: &self.config,
            secrets_manager: &mut self.secrets_manager,
            skill_manager: &mut self.skill_manager,
            soul_manager: &self.soul_manager,
            messages: &mut self.messages,
            input_mode: self.input_mode,
            gateway_status: self.gateway_status,
            loading_line: self.loading_line.clone(),
        }
    }
}

pub struct App {
    state: SharedState,
    pages: Vec<Box<dyn Page>>,
    active_page: usize,
    header: HeaderPane,
    footer: FooterPane,
    should_quit: bool,
    should_suspend: bool,
    #[allow(dead_code)]
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    /// Handle for the in-process gateway server task (if running)
    gateway_task: Option<JoinHandle<()>>,
    /// Token used to cancel the gateway server task
    gateway_cancel: Option<CancellationToken>,
    /// Write half of the WebSocket client connection to the gateway
    ws_sink: Option<WsSink>,
    /// Handle for the background WebSocket reader task
    reader_task: Option<JoinHandle<()>>,
    /// Whether the skills dialog overlay is visible
    show_skills_dialog: bool,
    /// API-key dialog state
    api_key_dialog: Option<ApiKeyDialogState>,
    /// Model-selector dialog state
    model_selector: Option<ModelSelectorState>,
    /// Loading spinner shown while fetching models
    fetch_loading: Option<FetchModelsLoading>,
    /// Loading spinner shown during device flow authentication
    device_flow_loading: Option<FetchModelsLoading>,
    /// Provider-selector dialog state
    provider_selector: Option<ProviderSelectorState>,
}

/// State for the API-key input dialog overlay.
struct ApiKeyDialogState {
    /// Which provider this key is for
    provider: String,
    /// Display name for the provider
    display: String,
    /// Name of the secret key (e.g. "ANTHROPIC_API_KEY")
    #[allow(dead_code)]
    secret_key: String,
    /// Current input buffer (the API key being typed)
    input: String,
    /// Which phase the dialog is in
    phase: ApiKeyDialogPhase,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let secrets_manager = SecretsManager::new(config.credentials_dir());
        Self::build(config, secrets_manager)
    }

    /// Create the app with a password-protected secrets vault.
    pub fn with_password(config: Config, password: String) -> Result<Self> {
        let secrets_manager = SecretsManager::with_password(config.credentials_dir(), password);
        Self::build(config, secrets_manager)
    }

    fn build(config: Config, mut secrets_manager: SecretsManager) -> Result<Self> {
        let (action_tx, action_rx) = mpsc::unbounded_channel();

        // Initialise managers
        if !config.use_secrets {
            secrets_manager.set_agent_access(false);
        }

        let skills_dir = config.skills_dir();
        let mut skill_manager = SkillManager::new(skills_dir);
        let _ = skill_manager.load_skills();

        let soul_path = config.soul_path();
        let mut soul_manager = SoulManager::new(soul_path);
        let _ = soul_manager.load();

        // Build pages
        let mut home = Home::new()?;
        home.register_action_handler(action_tx.clone())?;
        let pages: Vec<Box<dyn Page>> = vec![Box::new(home)];

        let gateway_status = GatewayStatus::Disconnected;

        let state = SharedState {
            config,
            messages: vec!["Welcome to RustyClaw! Type /help for commands.".to_string()],
            input_mode: InputMode::Normal,
            secrets_manager,
            skill_manager,
            soul_manager,
            gateway_status,
            loading_line: None,
        };

        Ok(Self {
            state,
            pages,
            active_page: 0,
            header: HeaderPane::new(),
            footer: FooterPane::new(),
            should_quit: false,
            should_suspend: false,
            action_tx,
            action_rx,
            gateway_task: None,
            gateway_cancel: None,
            ws_sink: None,
            reader_task: None,
            show_skills_dialog: false,
            api_key_dialog: None,
            model_selector: None,
            fetch_loading: None,
            device_flow_loading: None,
            provider_selector: None,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut tui = Tui::new()?;
        tui.enter()?;

        // Init pages
        {
            let ps = self.state.pane_state();
            for page in &mut self.pages {
                page.init(&ps)?;
            }
        }
        self.pages[self.active_page].focus()?;

        // Auto-start gateway (uses configured URL or defaults to ws://127.0.0.1:9001)
        self.start_gateway().await;

        loop {
            // Pull the next TUI event (key, mouse, tick, render, etc.)
            if let Some(event) = tui.next().await {
                // Determine the action from the event
                let mut action = match &event {
                    Event::Render => None,
                    Event::Tick => Some(Action::Tick),
                    Event::Resize(w, h) => Some(Action::Resize(*w, *h)),
                    Event::Quit => Some(Action::Quit),
                    _ => {
                        // While loading, Esc cancels the active async operation
                        if self.fetch_loading.is_some() || self.device_flow_loading.is_some() {
                            if let Event::Key(key) = &event {
                                if key.code == crossterm::event::KeyCode::Esc {
                                    if self.device_flow_loading.is_some() {
                                        self.device_flow_loading = None;
                                        self.state.loading_line = None;
                                        self.state.messages.push(
                                            "Device flow authentication cancelled.".to_string(),
                                        );
                                    } else {
                                        self.fetch_loading = None;
                                        self.state.loading_line = None;
                                        self.state.messages.push(
                                            "Model fetch cancelled.".to_string(),
                                        );
                                    }
                                    // Consume the Esc so it doesn't propagate
                                    continue;
                                }
                            }
                        }
                        // If the API key dialog is open, intercept keys for it
                        if self.api_key_dialog.is_some() {
                            if let Event::Key(key) = &event {
                                let action = self.handle_api_key_dialog_key(key.code);
                                Some(action)
                            } else {
                                None
                            }
                        }
                        // If the provider selector is open, intercept keys for it
                        else if self.provider_selector.is_some() {
                            if let Event::Key(key) = &event {
                                let action = self.handle_provider_selector_key(key.code);
                                Some(action)
                            } else {
                                None
                            }
                        }
                        // If the model selector is open, intercept keys for it
                        else if self.model_selector.is_some() {
                            if let Event::Key(key) = &event {
                                let action = self.handle_model_selector_key(key.code);
                                Some(action)
                            } else {
                                None
                            }
                        }
                        // If the skills dialog is open, intercept keys to close it
                        else if self.show_skills_dialog {
                            if let Event::Key(key) = &event {
                                match key.code {
                                    crossterm::event::KeyCode::Esc
                                    | crossterm::event::KeyCode::Enter
                                    | crossterm::event::KeyCode::Char('q') => {
                                        self.show_skills_dialog = false;
                                        Some(Action::Noop)
                                    }
                                    _ => Some(Action::Noop), // swallow all other keys
                                }
                            } else {
                                None
                            }
                        } else {
                            let mut ps = self.state.pane_state();
                            // Footer (input bar) always gets first chance at key events.
                            // In Normal mode it returns None for keys it doesn't consume,
                            // letting the active page handle navigation.
                            match self.footer.handle_events(event.clone(), &mut ps)? {
                                Some(EventResponse::Stop(a)) => {
                                    self.state.input_mode = ps.input_mode;
                                    Some(a)
                                }
                                _ => {
                                    self.state.input_mode = ps.input_mode;
                                    // Pass to the active page for navigation keys
                                    let mut ps2 = self.state.pane_state();
                                    match self.pages[self.active_page]
                                        .handle_events(event.clone(), &mut ps2)?
                                    {
                                        Some(EventResponse::Stop(a)) => Some(a),
                                        Some(EventResponse::Continue(_)) => None,
                                        None => None,
                                    }
                                }
                            }
                        }
                    }
                };

                // Process the action (and any chained follow-up actions)
                while let Some(act) = action {
                    action = self.dispatch_action(act).await?;
                }

                // Drain the mpsc channel (pages may have sent actions via tx)
                while let Ok(act) = self.action_rx.try_recv() {
                    let mut a = Some(act);
                    while let Some(act) = a {
                        a = self.dispatch_action(act).await?;
                    }
                }

                // Render
                if matches!(event, Event::Render) {
                    self.draw(&mut tui)?;
                }

                if self.should_quit {
                    tui.stop()?;
                    break;
                }
            }
        }

        tui.exit()?;
        Ok(())
    }

    /// Dispatch a single action to header, footer, and the active page.
    /// Returns an optional follow-up action.
    async fn dispatch_action(&mut self, action: Action) -> Result<Option<Action>> {
        match &action {
            Action::Quit => {
                self.should_quit = true;
                return Ok(None);
            }
            Action::Suspend => {
                self.should_suspend = true;
                return Ok(None);
            }
            Action::Resume => {
                self.should_suspend = false;
                return Ok(None);
            }
            Action::InputSubmit(ref text) => {
                return self.handle_input_submit(text.clone());
            }
            Action::ReconnectGateway => {
                self.start_gateway().await;
                return Ok(Some(Action::Update));
            }
            Action::DisconnectGateway => {
                self.stop_gateway().await;
                return Ok(Some(Action::Update));
            }
            Action::RestartGateway => {
                self.restart_gateway().await;
                return Ok(Some(Action::Update));
            }
            Action::SendToGateway(ref text) => {
                self.send_to_gateway(text.clone()).await;
                return Ok(None);
            }
            Action::GatewayMessage(ref text) => {
                self.state.messages.push(format!("◀ {}", text));
                // Auto-scroll
                return Ok(Some(Action::Update));
            }
            Action::GatewayDisconnected(ref reason) => {
                self.state.gateway_status = GatewayStatus::Disconnected;
                self.state.messages.push(format!("Gateway disconnected: {}", reason));
                self.ws_sink = None;
                self.reader_task = None;
                return Ok(Some(Action::Update));
            }
            Action::Tick => {
                // Advance the inline loading line
                if let Some(ref mut loading) = self.fetch_loading {
                    loading.tick += 1;
                    let spinner = SPINNER_FRAMES[loading.tick % SPINNER_FRAMES.len()];
                    self.state.loading_line = Some(format!(
                        "  {} Fetching models from {}…",
                        spinner, loading.display,
                    ));
                } else if let Some(ref mut loading) = self.device_flow_loading {
                    loading.tick += 1;
                    let spinner = SPINNER_FRAMES[loading.tick % SPINNER_FRAMES.len()];
                    self.state.loading_line = Some(format!(
                        "  {} Waiting for {} authorization…",
                        spinner, loading.display,
                    ));
                }
                // Fall through so panes also get Tick
            }
            Action::ShowSkills => {
                self.show_skills_dialog = !self.show_skills_dialog;
                return Ok(None);
            }
            Action::ShowProviderSelector => {
                self.open_provider_selector();
                return Ok(None);
            }
            Action::SetProvider(ref provider) => {
                let provider = provider.clone();
                // Save provider to config
                let model_cfg = self.state.config.model.get_or_insert_with(|| {
                    crate::config::ModelProvider {
                        provider: String::new(),
                        model: None,
                        base_url: None,
                    }
                });
                model_cfg.provider = provider.clone();
                if let Some(url) = providers::base_url_for_provider(&provider) {
                    model_cfg.base_url = Some(url.to_string());
                }
                if let Err(e) = self.state.config.save(None) {
                    self.state.messages.push(format!("Failed to save config: {}", e));
                } else {
                    self.state.messages.push(format!("Provider set to {}.", provider));
                }
                // Check auth method and proceed accordingly
                let def = providers::provider_by_id(&provider);
                let auth_method = def.map(|d| d.auth_method)
                    .unwrap_or(providers::AuthMethod::ApiKey);

                match auth_method {
                    providers::AuthMethod::DeviceFlow => {
                        if let Some(secret_key) = providers::secret_key_for_provider(&provider) {
                            match self.state.secrets_manager.get_secret(secret_key, true) {
                                Ok(Some(_)) => {
                                    self.state.messages.push(format!(
                                        "✓ Access token for {} is already stored.",
                                        providers::display_name_for_provider(&provider),
                                    ));
                                    return Ok(Some(Action::FetchModels(provider)));
                                }
                                _ => {
                                    return Ok(Some(Action::StartDeviceFlow(provider)));
                                }
                            }
                        }
                    }
                    providers::AuthMethod::ApiKey => {
                        if let Some(secret_key) = providers::secret_key_for_provider(&provider) {
                            match self.state.secrets_manager.get_secret(secret_key, true) {
                                Ok(Some(_)) => {
                                    self.state.messages.push(format!(
                                        "✓ API key for {} is already stored.",
                                        providers::display_name_for_provider(&provider),
                                    ));
                                    return Ok(Some(Action::FetchModels(provider)));
                                }
                                _ => {
                                    return Ok(Some(Action::PromptApiKey(provider)));
                                }
                            }
                        }
                    }
                    providers::AuthMethod::None => {
                        self.state.messages.push(format!(
                            "{} does not require authentication.",
                            providers::display_name_for_provider(&provider),
                        ));
                        return Ok(Some(Action::FetchModels(provider)));
                    }
                }
                return Ok(None);
            }
            Action::PromptApiKey(ref provider) => {
                return Ok(self.open_api_key_dialog(provider.clone()));
            }
            Action::ConfirmStoreSecret { ref provider, ref key } => {
                return self.handle_confirm_store_secret(provider.clone(), key.clone());
            }
            Action::FetchModels(ref provider) => {
                self.spawn_fetch_models(provider.clone());
                return Ok(None);
            }
            Action::FetchModelsFailed(ref msg) => {
                self.fetch_loading = None;
                self.state.loading_line = None;
                self.state.messages.push(msg.clone());
                return Ok(Some(Action::Update));
            }
            Action::ShowModelSelector { ref provider, ref models } => {
                self.fetch_loading = None;
                self.state.loading_line = None;
                self.open_model_selector(provider.clone(), models.clone());
                return Ok(None);
            }
            Action::StartDeviceFlow(ref provider) => {
                self.spawn_device_flow(provider.clone());
                return Ok(None);
            }
            Action::DeviceFlowCodeReady { ref url, ref code } => {
                self.state.messages.push(format!(
                    "Open this URL in your browser:",
                ));
                self.state.messages.push(format!(
                    "  ➜  {}", url,
                ));
                self.state.messages.push(format!(
                    "Then enter this code:  {}", code,
                ));
                return Ok(Some(Action::Update));
            }
            Action::DeviceFlowAuthenticated { ref provider, ref token } => {
                self.device_flow_loading = None;
                self.state.loading_line = None;
                let secret_key = providers::secret_key_for_provider(provider)
                    .unwrap_or("COPILOT_TOKEN");
                let display = providers::display_name_for_provider(provider).to_string();
                match self.state.secrets_manager.store_secret(secret_key, token) {
                    Ok(()) => {
                        self.state.messages.push(format!(
                            "✓ {} authenticated successfully. Token stored.",
                            display,
                        ));
                    }
                    Err(e) => {
                        self.state.messages.push(format!(
                            "Failed to store token: {}. Token set for this session only.",
                            e,
                        ));
                    }
                }
                // Proceed to model selection
                return Ok(Some(Action::FetchModels(provider.clone())));
            }
            Action::DeviceFlowFailed(ref msg) => {
                self.device_flow_loading = None;
                self.state.loading_line = None;
                self.state.messages.push(msg.clone());
                return Ok(Some(Action::Update));
            }
            _ => {}
        }

        // Update header
        {
            let mut ps = self.state.pane_state();
            self.header.update(action.clone(), &mut ps)?;
            self.state.input_mode = ps.input_mode;
        }

        // Update footer
        let footer_follow = {
            let mut ps = self.state.pane_state();
            let r = self.footer.update(action.clone(), &mut ps)?;
            self.state.input_mode = ps.input_mode;
            r
        };

        // Update active page
        let page_follow = {
            let mut ps = self.state.pane_state();
            let r = self.pages[self.active_page].update(action, &mut ps)?;
            self.state.input_mode = ps.input_mode;
            r
        };

        Ok(footer_follow.or(page_follow))
    }

    /// Process submitted input — either a /command or a plain prompt.
    fn handle_input_submit(&mut self, text: String) -> Result<Option<Action>> {
        if text.is_empty() {
            return Ok(None);
        }

        if text.starts_with('/') {
            // It's a command
            let mut context = CommandContext {
                secrets_manager: &mut self.state.secrets_manager,
                skill_manager: &mut self.state.skill_manager,
            };

            let response = handle_command(&text, &mut context);

            match response.action {
                CommandAction::Quit => {
                    self.should_quit = true;
                    return Ok(None);
                }
                CommandAction::ClearMessages => {
                    self.state.messages.clear();
                    for msg in response.messages {
                        self.state.messages.push(msg);
                    }
                }
                CommandAction::GatewayStart => {
                    for msg in response.messages {
                        self.state.messages.push(msg);
                    }
                    return Ok(Some(Action::ReconnectGateway));
                }
                CommandAction::GatewayStop => {
                    for msg in response.messages {
                        self.state.messages.push(msg);
                    }
                    return Ok(Some(Action::DisconnectGateway));
                }
                CommandAction::GatewayRestart => {
                    for msg in response.messages {
                        self.state.messages.push(msg);
                    }
                    return Ok(Some(Action::RestartGateway));
                }
                CommandAction::GatewayInfo => {
                    let url_display = self
                        .state
                        .config
                        .gateway_url
                        .as_deref()
                        .unwrap_or("(none)");
                    self.state.messages.push(format!(
                        "Gateway: {}  Status: {}",
                        url_display,
                        self.state.gateway_status.label()
                    ));
                }
                CommandAction::SetProvider(ref provider) => {
                    for msg in &response.messages {
                        self.state.messages.push(msg.clone());
                    }
                    return Ok(Some(Action::SetProvider(provider.clone())));
                }
                CommandAction::SetModel(ref model) => {
                    for msg in &response.messages {
                        self.state.messages.push(msg.clone());
                    }
                    let model_cfg = self.state.config.model.get_or_insert_with(|| {
                        crate::config::ModelProvider {
                            provider: "anthropic".into(),
                            model: None,
                            base_url: None,
                        }
                    });
                    model_cfg.model = Some(model.clone());
                    if let Err(e) = self.state.config.save(None) {
                        self.state.messages.push(format!("Failed to save config: {}", e));
                    } else {
                        self.state.messages.push(format!("Model set to {}.", model));
                    }
                }
                CommandAction::ShowSkills => {
                    return Ok(Some(Action::ShowSkills));
                }
                CommandAction::ShowProviderSelector => {
                    return Ok(Some(Action::ShowProviderSelector));
                }
                CommandAction::None => {
                    for msg in response.messages {
                        self.state.messages.push(msg);
                    }
                }
            }

            Ok(Some(Action::TimedStatusLine(text, 3)))
        } else {
            // It's a plain prompt
            self.state.messages.push(format!("▶ {}", text));
            if self.state.gateway_status == GatewayStatus::Connected && self.ws_sink.is_some() {
                return Ok(Some(Action::SendToGateway(text)));
            }
            self.state
                .messages
                .push("(Gateway not connected — use /gateway start)".to_string());
            Ok(Some(Action::Update))
        }
    }

    /// Start the gateway server in-process, then connect to it as a client.
    async fn start_gateway(&mut self) {
        const DEFAULT_GATEWAY_URL: &str = "ws://127.0.0.1:9001";

        let url = self
            .state
            .config
            .gateway_url
            .clone()
            .unwrap_or_else(|| DEFAULT_GATEWAY_URL.to_string());

        // If already running, report and return.
        if self.gateway_task.is_some() {
            self.state
                .messages
                .push("Gateway is already running.".to_string());
            return;
        }

        self.state.gateway_status = GatewayStatus::Connecting;
        self.state
            .messages
            .push(format!("Starting gateway on {}…", url));

        // Spawn the gateway server as a background task.
        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();
        let config_clone = self.state.config.clone();
        let listen_url = url.clone();
        let handle = tokio::spawn(async move {
            let opts = GatewayOptions {
                listen: listen_url,
            };
            if let Err(err) = run_gateway(config_clone, opts, cancel_child).await {
                eprintln!("Gateway server error: {}", err);
            }
        });
        self.gateway_task = Some(handle);
        self.gateway_cancel = Some(cancel);

        // Give the server a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect as a WebSocket client.
        self.connect_to_gateway(&url).await;
    }

    /// Connect the TUI as a WebSocket client to a running gateway.
    async fn connect_to_gateway(&mut self, url: &str) {
        self.state.gateway_status = GatewayStatus::Connecting;
        match tokio_tungstenite::connect_async(url).await {
            Ok((ws_stream, _)) => {
                let (sink, stream) = ws_stream.split();
                self.ws_sink = Some(sink);

                self.state.gateway_status = GatewayStatus::Connected;
                self.state
                    .messages
                    .push(format!("Connected to gateway {}", url));

                // Spawn a background task that reads from the gateway and
                // forwards messages into the TUI event loop via action_tx.
                let tx = self.action_tx.clone();
                self.reader_task = Some(tokio::spawn(async move {
                    Self::gateway_reader_loop(stream, tx).await;
                }));
            }
            Err(err) => {
                self.state.gateway_status = GatewayStatus::Error;
                self.state
                    .messages
                    .push(format!("Gateway connection failed: {}", err));
            }
        }
    }

    /// Background loop: reads messages from the gateway WebSocket stream and
    /// sends them as actions into the TUI event loop.
    async fn gateway_reader_loop(
        mut stream: futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        tx: mpsc::UnboundedSender<Action>,
    ) {
        while let Some(result) = stream.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    let _ = tx.send(Action::GatewayMessage(text));
                }
                Ok(Message::Close(_)) => {
                    let _ = tx.send(Action::GatewayDisconnected(
                        "server sent close frame".to_string(),
                    ));
                    break;
                }
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                    // handled automatically by tungstenite
                }
                Ok(_) => {}
                Err(err) => {
                    let _ = tx.send(Action::GatewayDisconnected(format!("{}", err)));
                    break;
                }
            }
        }
    }

    /// Send a text message to the gateway over the open WebSocket connection.
    async fn send_to_gateway(&mut self, text: String) {
        if let Some(ref mut sink) = self.ws_sink {
            match sink.send(Message::Text(text)).await {
                Ok(()) => {}
                Err(err) => {
                    self.state.messages.push(format!("Send failed: {}", err));
                    self.state.gateway_status = GatewayStatus::Error;
                    self.ws_sink = None;
                }
            }
        } else {
            self.state
                .messages
                .push("Cannot send: gateway not connected.".to_string());
        }
    }

    /// Stop the gateway: close the client connection and cancel the server task.
    async fn stop_gateway(&mut self) {
        let was_running = self.gateway_task.is_some() || self.ws_sink.is_some();

        // Abort the reader task first so it doesn't fire a disconnect action.
        if let Some(handle) = self.reader_task.take() {
            handle.abort();
        }

        // Close the client-side WebSocket gracefully.
        if let Some(mut sink) = self.ws_sink.take() {
            let _ = sink.send(Message::Close(None)).await;
            let _ = sink.close().await;
        }

        // Cancel the server task.
        if let Some(cancel) = self.gateway_cancel.take() {
            cancel.cancel();
        }
        if let Some(handle) = self.gateway_task.take() {
            // Give it a moment to wind down; don't block forever.
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                handle,
            )
            .await;
        }

        if was_running {
            self.state.gateway_status = GatewayStatus::Disconnected;
            self.state
                .messages
                .push("Gateway stopped.".to_string());
        } else {
            self.state
                .messages
                .push("Gateway is not running.".to_string());
        }
    }

    /// Restart: stop, let the TUI render the disconnect, then reconnect.
    ///
    /// We stop synchronously so the status flips to Disconnected immediately,
    /// then schedule ReconnectGateway via the action channel after a short
    /// delay so the event loop renders at least one frame showing the
    /// intermediate state before the connection attempt begins.
    async fn restart_gateway(&mut self) {
        self.stop_gateway().await;

        // Schedule the reconnect after a brief pause so the render loop can
        // show the Disconnected status before we start connecting again.
        let tx = self.action_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let _ = tx.send(Action::ReconnectGateway);
        });
    }

    fn draw(&mut self, tui: &mut Tui) -> Result<()> {
        tui.draw(|frame| {
            let area = frame.size();

            // Layout: header (3 rows), body (fill), footer (2 rows: status + input)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(2),
                ])
                .split(area);

            let ps = PaneState {
                config: &self.state.config,
                secrets_manager: &mut self.state.secrets_manager,
                skill_manager: &mut self.state.skill_manager,
                soul_manager: &self.state.soul_manager,
                messages: &mut self.state.messages,
                input_mode: self.state.input_mode,
                gateway_status: self.state.gateway_status,
                loading_line: self.state.loading_line.clone(),
            };

            let _ = self.header.draw(frame, chunks[0], &ps);
            let _ = self.pages[self.active_page].draw(frame, chunks[1], &ps);
            let _ = self.footer.draw(frame, chunks[2], &ps);

            // Skills dialog overlay
            if self.show_skills_dialog {
                Self::draw_skills_dialog(frame, area, &ps);
            }

            // API key dialog overlay
            if let Some(ref dialog) = self.api_key_dialog {
                Self::draw_api_key_dialog(frame, area, dialog);
            }

            // Provider selector dialog overlay
            if let Some(ref selector) = self.provider_selector {
                Self::draw_provider_selector_dialog(frame, area, selector);
            }

            // Model selector dialog overlay
            if let Some(ref selector) = self.model_selector {
                Self::draw_model_selector_dialog(frame, area, selector);
            }
        })?;
        Ok(())
    }

    /// Draw a centered skills dialog overlay.
    fn draw_skills_dialog(frame: &mut ratatui::Frame<'_>, area: Rect, state: &PaneState<'_>) {
        use crate::theme::tui_palette as tp;
        use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

        let skills = state.skill_manager.get_skills();

        // Size the dialog: width ~60 or 80% of screen, height = skills + 4 (border + header + hint)
        let dialog_w = 60.min(area.width.saturating_sub(4));
        let dialog_h = ((skills.len() as u16) + 4).min(area.height.saturating_sub(4)).max(6);
        let x = area.x + (area.width.saturating_sub(dialog_w)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_h)) / 2;
        let dialog_area = Rect::new(x, y, dialog_w, dialog_h);

        // Clear the background behind the dialog
        frame.render_widget(Clear, dialog_area);

        let items: Vec<ListItem> = skills
            .iter()
            .map(|s| {
                let (icon, icon_style) = if s.enabled {
                    ("✓", Style::default().fg(tp::SUCCESS))
                } else {
                    ("✗", Style::default().fg(tp::MUTED))
                };
                let name_style = if s.enabled {
                    Style::default().fg(tp::ACCENT_BRIGHT)
                } else {
                    Style::default().fg(tp::TEXT_DIM)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", icon), icon_style),
                    Span::styled(&s.name, name_style),
                    Span::styled(
                        format!(" — {}", s.description.as_deref().unwrap_or("No description")),
                        Style::default().fg(tp::MUTED),
                    ),
                ]))
            })
            .collect();

        let empty_msg = if skills.is_empty() {
            vec![ListItem::new(Span::styled(
                "  No skills loaded. Place .md files in your skills/ directory.",
                Style::default().fg(tp::TEXT_DIM),
            ))]
        } else {
            vec![]
        };

        let all_items = if items.is_empty() { empty_msg } else { items };

        let list = List::new(all_items)
            .block(
                Block::default()
                    .title(Span::styled(
                        " Skills ",
                        tp::title_focused(),
                    ))
                    .title_bottom(
                        Line::from(Span::styled(
                            " Esc to close ",
                            Style::default().fg(tp::MUTED),
                        ))
                        .right_aligned(),
                    )
                    .borders(Borders::ALL)
                    .border_style(tp::focused_border())
                    .border_type(ratatui::widgets::BorderType::Rounded),
            )
            .style(Style::default().fg(tp::TEXT));

        frame.render_widget(list, dialog_area);
    }

    // ── API-key dialog ──────────────────────────────────────────────────────

    /// Open the API-key input dialog for the given provider.
    fn open_api_key_dialog(&mut self, provider: String) -> Option<Action> {
        let secret_key = match providers::secret_key_for_provider(&provider) {
            Some(k) => k.to_string(),
            None => return None, // shouldn't happen, but just in case
        };
        let display = providers::display_name_for_provider(&provider).to_string();
        self.state.messages.push(format!(
            "No API key found for {}. Please enter one below.",
            display,
        ));
        self.api_key_dialog = Some(ApiKeyDialogState {
            provider,
            display,
            secret_key,
            input: String::new(),
            phase: ApiKeyDialogPhase::EnterKey,
        });
        None
    }

    /// Handle key events when the API key dialog is open.
    fn handle_api_key_dialog_key(&mut self, code: crossterm::event::KeyCode) -> Action {
        use crossterm::event::KeyCode;

        // Take the dialog state so we can mutate it without borrowing self
        let Some(mut dialog) = self.api_key_dialog.take() else {
            return Action::Noop;
        };

        match dialog.phase {
            ApiKeyDialogPhase::EnterKey => match code {
                KeyCode::Esc => {
                    self.state
                        .messages
                        .push("API key entry cancelled.".to_string());
                    // dialog is already taken — dropped
                    return Action::Noop;
                }
                KeyCode::Enter => {
                    if dialog.input.is_empty() {
                        self.state.messages.push(
                            "No key entered — you can add one later with /provider."
                                .to_string(),
                        );
                        return Action::Noop;
                    }
                    // Move to confirmation phase
                    dialog.phase = ApiKeyDialogPhase::ConfirmStore;
                    self.api_key_dialog = Some(dialog);
                    return Action::Noop;
                }
                KeyCode::Backspace => {
                    dialog.input.pop();
                    self.api_key_dialog = Some(dialog);
                    return Action::Noop;
                }
                KeyCode::Char(c) => {
                    dialog.input.push(c);
                    self.api_key_dialog = Some(dialog);
                    return Action::Noop;
                }
                _ => {
                    self.api_key_dialog = Some(dialog);
                    return Action::Noop;
                }
            },
            ApiKeyDialogPhase::ConfirmStore => match code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    // Store it
                    let provider = dialog.provider.clone();
                    let key = dialog.input.clone();
                    return Action::ConfirmStoreSecret { provider, key };
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    // Use the key for this session but don't store
                    self.state.messages.push(format!(
                        "✓ API key for {} set for this session (not stored).",
                        dialog.display,
                    ));
                    // Proceed to model selection
                    return Action::FetchModels(dialog.provider.clone());
                }
                _ => {
                    self.api_key_dialog = Some(dialog);
                    return Action::Noop;
                }
            },
        }
    }

    /// Store the API key in the secrets vault after user confirmation.
    fn handle_confirm_store_secret(
        &mut self,
        provider: String,
        key: String,
    ) -> Result<Option<Action>> {
        let secret_key = providers::secret_key_for_provider(&provider)
            .unwrap_or("API_KEY");
        let display = providers::display_name_for_provider(&provider).to_string();

        match self.state.secrets_manager.store_secret(&secret_key, &key) {
            Ok(()) => {
                self.state.messages.push(format!(
                    "✓ API key for {} stored securely.",
                    display,
                ));
            }
            Err(e) => {
                self.state.messages.push(format!(
                    "Failed to store API key: {}. Key is set for this session only.",
                    e,
                ));
            }
        }
        // After storing the key, proceed to model selection
        Ok(Some(Action::FetchModels(provider)))
    }

    /// Draw a centered API-key dialog overlay.
    fn draw_api_key_dialog(
        frame: &mut ratatui::Frame<'_>,
        area: Rect,
        dialog: &ApiKeyDialogState,
    ) {
        use crate::theme::tui_palette as tp;
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let dialog_w = 56.min(area.width.saturating_sub(4));
        let dialog_h = 7_u16.min(area.height.saturating_sub(4)).max(5);
        let x = area.x + (area.width.saturating_sub(dialog_w)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_h)) / 2;
        let dialog_area = Rect::new(x, y, dialog_w, dialog_h);

        // Clear the background behind the dialog
        frame.render_widget(Clear, dialog_area);

        let title = format!(" {} API Key ", dialog.display);
        let block = Block::default()
            .title(Span::styled(&title, tp::title_focused()))
            .title_bottom(
                Line::from(Span::styled(
                    " Esc to cancel ",
                    Style::default().fg(tp::MUTED),
                ))
                .right_aligned(),
            )
            .borders(Borders::ALL)
            .border_style(tp::focused_border())
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);

        match dialog.phase {
            ApiKeyDialogPhase::EnterKey => {
                // Label
                let label = Line::from(Span::styled(
                    format!(" Enter your {} API key:", dialog.display),
                    Style::default().fg(tp::TEXT),
                ));
                if inner.height >= 1 {
                    frame.render_widget(
                        Paragraph::new(label),
                        Rect::new(inner.x, inner.y, inner.width, 1),
                    );
                }

                // Masked input
                if inner.height >= 3 {
                    let masked: String = "•".repeat(dialog.input.len());
                    let input_area = Rect::new(inner.x + 1, inner.y + 2, inner.width.saturating_sub(2), 1);
                    let prompt = Line::from(vec![
                        Span::styled("❯ ", Style::default().fg(tp::ACCENT)),
                        Span::styled(&masked, Style::default().fg(tp::TEXT)),
                    ]);
                    frame.render_widget(Paragraph::new(prompt), input_area);

                    // Show cursor
                    frame.set_cursor(
                        input_area.x + 2 + masked.len() as u16,
                        input_area.y,
                    );
                }
            }
            ApiKeyDialogPhase::ConfirmStore => {
                // Show key length hint
                let hint = Line::from(Span::styled(
                    format!(" Key entered ({} chars).", dialog.input.len()),
                    Style::default().fg(tp::SUCCESS),
                ));
                if inner.height >= 1 {
                    frame.render_widget(
                        Paragraph::new(hint),
                        Rect::new(inner.x, inner.y, inner.width, 1),
                    );
                }

                // Store question
                if inner.height >= 3 {
                    let question = Line::from(vec![
                        Span::styled(
                            " Store permanently in secrets vault? ",
                            Style::default().fg(tp::TEXT),
                        ),
                        Span::styled(
                            "[Y/n]",
                            Style::default()
                                .fg(tp::ACCENT_BRIGHT)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    frame.render_widget(
                        Paragraph::new(question),
                        Rect::new(inner.x, inner.y + 2, inner.width, 1),
                    );
                }
            }
        }
    }

    // ── Model selector dialog ───────────────────────────────────────────────

    /// Spawn a background task to fetch models and send the result back
    /// via the action channel.  Shows an inline loading line in the meantime.
    fn spawn_fetch_models(&mut self, provider: String) {
        let display = providers::display_name_for_provider(&provider).to_string();
        self.state.messages.push(format!(
            "Fetching available models for {}…",
            display,
        ));

        // Show the inline loading line under the chat log
        let spinner = SPINNER_FRAMES[0];
        self.state.loading_line = Some(format!(
            "  {} Fetching models from {}…",
            spinner, display,
        ));
        self.fetch_loading = Some(FetchModelsLoading {
            display: display.clone(),
            tick: 0,
        });

        // Gather what we need for the background task
        let api_key = providers::secret_key_for_provider(&provider)
            .and_then(|sk| {
                self.state
                    .secrets_manager
                    .get_secret(sk, true)
                    .ok()
                    .flatten()
            });

        let base_url = self
            .state
            .config
            .model
            .as_ref()
            .and_then(|m| m.base_url.clone());

        let tx = self.action_tx.clone();
        let provider_clone = provider.clone();

        tokio::spawn(async move {
            match providers::fetch_models(
                &provider_clone,
                api_key.as_deref(),
                base_url.as_deref(),
            )
            .await
            {
                Ok(models) => {
                    let _ = tx.send(Action::ShowModelSelector {
                        provider: provider_clone,
                        models,
                    });
                }
                Err(err) => {
                    let _ = tx.send(Action::FetchModelsFailed(err));
                }
            }
        });
    }

    /// Draw a centered loading spinner overlay.

    // ── Device flow authentication ──────────────────────────────────────────

    /// Spawn a background task to perform OAuth device flow authentication.
    /// Shows the verification URL and user code as messages, then polls for
    /// the token in the background.
    fn spawn_device_flow(&mut self, provider: String) {
        let def = match providers::provider_by_id(&provider) {
            Some(d) => d,
            None => {
                self.state.messages.push(format!("Unknown provider: {}", provider));
                return;
            }
        };

        let device_config = match def.device_flow {
            Some(cfg) => cfg,
            None => {
                self.state.messages.push(format!(
                    "{} does not support device flow authentication.",
                    def.display,
                ));
                return;
            }
        };

        let display = def.display.to_string();
        self.state.messages.push(format!(
            "Authenticating with {}…",
            display,
        ));

        // Show the inline loading line
        let spinner = SPINNER_FRAMES[0];
        self.state.loading_line = Some(format!(
            "  {} Starting {} authentication…",
            spinner, display,
        ));
        self.device_flow_loading = Some(FetchModelsLoading {
            display: display.clone(),
            tick: 0,
        });

        let tx = self.action_tx.clone();
        let provider_clone = provider.clone();
        // All fields of DeviceFlowConfig are &'static str, so we can just
        // copy the reference to the static config into the spawned task.
        let device_cfg: &'static providers::DeviceFlowConfig = device_config;

        tokio::spawn(async move {
            // Step 1: Start the device flow
            let auth = match providers::start_device_flow(device_cfg).await {
                Ok(a) => a,
                Err(e) => {
                    let _ = tx.send(Action::DeviceFlowFailed(format!(
                        "Failed to start device flow: {}", e,
                    )));
                    return;
                }
            };

            // Step 2: Show the URL and code to the user via messages
            let _ = tx.send(Action::DeviceFlowCodeReady {
                url: auth.verification_uri.clone(),
                code: auth.user_code.clone(),
            });

            // Step 3: Poll for the token
            let interval = std::time::Duration::from_secs(auth.interval.max(5));
            let max_attempts = (auth.expires_in / interval.as_secs()).max(10);

            for _ in 0..max_attempts {
                tokio::time::sleep(interval).await;

                match providers::poll_device_token(device_cfg, &auth.device_code).await {
                    Ok(Some(token)) => {
                        let _ = tx.send(Action::DeviceFlowAuthenticated {
                            provider: provider_clone,
                            token,
                        });
                        return;
                    }
                    Ok(None) => {
                        // Still pending — keep polling
                    }
                    Err(e) => {
                        let _ = tx.send(Action::DeviceFlowFailed(format!(
                            "Authentication failed: {}", e,
                        )));
                        return;
                    }
                }
            }

            let _ = tx.send(Action::DeviceFlowFailed(
                "Authentication timed out. Please try again with /provider.".to_string(),
            ));
        });
    }

    /// Open the model selector dialog with the given list.
    fn open_model_selector(&mut self, provider: String, models: Vec<String>) {
        let display = providers::display_name_for_provider(&provider).to_string();
        self.model_selector = Some(ModelSelectorState {
            provider,
            display,
            models,
            selected: 0,
            scroll_offset: 0,
        });
    }

    /// Handle key events when the model selector dialog is open.
    fn handle_model_selector_key(&mut self, code: crossterm::event::KeyCode) -> Action {
        use crossterm::event::KeyCode;

        let Some(mut sel) = self.model_selector.take() else {
            return Action::Noop;
        };

        // Maximum visible rows in the dialog body
        const MAX_VISIBLE: usize = 14;

        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.state
                    .messages
                    .push("Model selection cancelled.".to_string());
                return Action::Noop;
            }
            KeyCode::Enter => {
                if let Some(model_name) = sel.models.get(sel.selected).cloned() {
                    // Save the selected model
                    let model_cfg =
                        self.state.config.model.get_or_insert_with(|| {
                            crate::config::ModelProvider {
                                provider: sel.provider.clone(),
                                model: None,
                                base_url: None,
                            }
                        });
                    model_cfg.model = Some(model_name.clone());
                    if let Err(e) = self.state.config.save(None) {
                        self.state
                            .messages
                            .push(format!("Failed to save config: {}", e));
                    } else {
                        self.state.messages.push(format!(
                            "✓ Model set to {}.",
                            model_name,
                        ));
                    }
                }
                return Action::Update;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if sel.selected > 0 {
                    sel.selected -= 1;
                    if sel.selected < sel.scroll_offset {
                        sel.scroll_offset = sel.selected;
                    }
                }
                self.model_selector = Some(sel);
                return Action::Noop;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if sel.selected + 1 < sel.models.len() {
                    sel.selected += 1;
                    if sel.selected >= sel.scroll_offset + MAX_VISIBLE {
                        sel.scroll_offset = sel.selected - MAX_VISIBLE + 1;
                    }
                }
                self.model_selector = Some(sel);
                return Action::Noop;
            }
            KeyCode::Home => {
                sel.selected = 0;
                sel.scroll_offset = 0;
                self.model_selector = Some(sel);
                return Action::Noop;
            }
            KeyCode::End => {
                sel.selected = sel.models.len().saturating_sub(1);
                sel.scroll_offset = sel
                    .models
                    .len()
                    .saturating_sub(MAX_VISIBLE);
                self.model_selector = Some(sel);
                return Action::Noop;
            }
            _ => {
                self.model_selector = Some(sel);
                return Action::Noop;
            }
        }
    }

    /// Draw a centered model-selector dialog overlay.
    fn draw_model_selector_dialog(
        frame: &mut ratatui::Frame<'_>,
        area: Rect,
        sel: &ModelSelectorState,
    ) {
        use crate::theme::tui_palette as tp;
        use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

        const MAX_VISIBLE: usize = 14;

        let dialog_w = 60.min(area.width.saturating_sub(4));
        let visible_count = sel.models.len().min(MAX_VISIBLE);
        // +4 for border (2) + title line + hint line
        let dialog_h = ((visible_count as u16) + 4)
            .min(area.height.saturating_sub(4))
            .max(6);
        let x = area.x + (area.width.saturating_sub(dialog_w)) / 2;
        let y = area.y + (area.height.saturating_sub(dialog_h)) / 2;
        let dialog_area = Rect::new(x, y, dialog_w, dialog_h);

        // Clear the background behind the dialog
        frame.render_widget(Clear, dialog_area);

        let title = format!(" Select a {} model ", sel.display);
        let hint = if sel.models.len() > MAX_VISIBLE {
            format!(
                " {}/{} · ↑↓ navigate · Enter select · Esc cancel ",
                sel.selected + 1,
                sel.models.len(),
            )
        } else {
            " ↑↓ navigate · Enter select · Esc cancel ".to_string()
        };

        let block = Block::default()
            .title(Span::styled(&title, tp::title_focused()))
            .title_bottom(
                Line::from(Span::styled(
                    &hint,
                    Style::default().fg(tp::MUTED),
                ))
                .right_aligned(),
            )
            .borders(Borders::ALL)
            .border_style(tp::focused_border())
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);

        let end = (sel.scroll_offset + MAX_VISIBLE).min(sel.models.len());
        let visible_models = &sel.models[sel.scroll_offset..end];

        let items: Vec<ListItem> = visible_models
            .iter()
            .enumerate()
            .map(|(i, model)| {
                let abs_idx = sel.scroll_offset + i;
                let is_selected = abs_idx == sel.selected;
                let (marker, style) = if is_selected {
                    (
                        "❯ ",
                        Style::default()
                            .fg(tp::ACCENT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("  ", Style::default().fg(tp::TEXT))
                };
                ListItem::new(Line::from(vec![
                    Span::styled(marker, Style::default().fg(tp::ACCENT)),
                    Span::styled(model.as_str(), style),
                ]))
            })
            .collect();

        let list =
            List::new(items).style(Style::default().fg(tp::TEXT));

        frame.render_widget(list, inner);
    }

    // ── Provider selector dialog ──────────────────────────────

    /// Open the provider-selector dialog populated from the shared
    /// provider registry.
    fn open_provider_selector(&mut self) {
        let providers: Vec<(String, String)> = providers::PROVIDERS
            .iter()
            .map(|p| (p.id.to_string(), p.display.to_string()))
            .collect();
        self.provider_selector = Some(ProviderSelectorState {
            providers,
            selected: 0,
            scroll_offset: 0,
        });
    }

    /// Handle key events when the provider selector dialog is open.
    fn handle_provider_selector_key(
        &mut self,
        code: crossterm::event::KeyCode,
    ) -> Action {
        use crossterm::event::KeyCode;

        let Some(mut sel) = self.provider_selector.take() else {
            return Action::Noop;
        };

        const MAX_VISIBLE: usize = 14;

        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.state
                    .messages
                    .push("Provider selection cancelled.".to_string());
                return Action::Noop;
            }
            KeyCode::Enter => {
                if let Some((id, display)) =
                    sel.providers.get(sel.selected).cloned()
                {
                    self.state.messages.push(format!(
                        "Switching provider to {}\u{2026}",
                        display,
                    ));
                    return Action::SetProvider(id);
                }
                return Action::Noop;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if sel.selected > 0 {
                    sel.selected -= 1;
                    if sel.selected < sel.scroll_offset {
                        sel.scroll_offset = sel.selected;
                    }
                }
                self.provider_selector = Some(sel);
                return Action::Noop;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if sel.selected + 1 < sel.providers.len() {
                    sel.selected += 1;
                    if sel.selected >= sel.scroll_offset + MAX_VISIBLE {
                        sel.scroll_offset =
                            sel.selected - MAX_VISIBLE + 1;
                    }
                }
                self.provider_selector = Some(sel);
                return Action::Noop;
            }
            KeyCode::Home => {
                sel.selected = 0;
                sel.scroll_offset = 0;
                self.provider_selector = Some(sel);
                return Action::Noop;
            }
            KeyCode::End => {
                sel.selected =
                    sel.providers.len().saturating_sub(1);
                sel.scroll_offset =
                    sel.providers.len().saturating_sub(MAX_VISIBLE);
                self.provider_selector = Some(sel);
                return Action::Noop;
            }
            _ => {
                self.provider_selector = Some(sel);
                return Action::Noop;
            }
        }
    }

    /// Draw a centered provider-selector dialog overlay.
    fn draw_provider_selector_dialog(
        frame: &mut ratatui::Frame<'_>,
        area: Rect,
        sel: &ProviderSelectorState,
    ) {
        use crate::theme::tui_palette as tp;
        use ratatui::widgets::{
            Block, Borders, Clear, List, ListItem,
        };

        const MAX_VISIBLE: usize = 14;

        let dialog_w = 50.min(area.width.saturating_sub(4));
        let visible_count = sel.providers.len().min(MAX_VISIBLE);
        let dialog_h = ((visible_count as u16) + 4)
            .min(area.height.saturating_sub(4))
            .max(6);
        let x =
            area.x + (area.width.saturating_sub(dialog_w)) / 2;
        let y =
            area.y + (area.height.saturating_sub(dialog_h)) / 2;
        let dialog_area = Rect::new(x, y, dialog_w, dialog_h);

        frame.render_widget(Clear, dialog_area);

        let title = " Select a provider ";
        let hint = if sel.providers.len() > MAX_VISIBLE {
            format!(
                " {}/{} · ↑↓ navigate · Enter select · Esc cancel ",
                sel.selected + 1,
                sel.providers.len(),
            )
        } else {
            " ↑↓ navigate · Enter select · Esc cancel ".to_string()
        };

        let block = Block::default()
            .title(Span::styled(title, tp::title_focused()))
            .title_bottom(
                Line::from(Span::styled(
                    &hint,
                    Style::default().fg(tp::MUTED),
                ))
                .right_aligned(),
            )
            .borders(Borders::ALL)
            .border_style(tp::focused_border())
            .border_type(ratatui::widgets::BorderType::Rounded);

        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);

        let end = (sel.scroll_offset + MAX_VISIBLE)
            .min(sel.providers.len());
        let visible = &sel.providers[sel.scroll_offset..end];

        let items: Vec<ListItem> = visible
            .iter()
            .enumerate()
            .map(|(i, (_id, display))| {
                let abs_idx = sel.scroll_offset + i;
                let is_selected = abs_idx == sel.selected;
                let (marker, style) = if is_selected {
                    (
                        "❯ ",
                        Style::default()
                            .fg(tp::ACCENT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("  ", Style::default().fg(tp::TEXT))
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        marker,
                        Style::default().fg(tp::ACCENT),
                    ),
                    Span::styled(display.as_str(), style),
                ]))
            })
            .collect();

        let list =
            List::new(items).style(Style::default().fg(tp::TEXT));

        frame.render_widget(list, inner);
    }
}
