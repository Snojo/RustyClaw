use anyhow::Result;
use ratatui::{
    layout::{Constraint, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
};

use crate::action::Action;
use crate::panes::{MessageRole, Pane, PaneState};
use crate::theme::tui_palette as tp;
use crate::tui::Frame;

pub struct MessagesPane {
    focused: bool,
    focused_border_style: Style,
    scroll_offset: usize,
}

impl MessagesPane {
    pub fn new(focused: bool, focused_border_style: Style) -> Self {
        Self {
            focused,
            focused_border_style,
            scroll_offset: 0,
        }
    }

    fn border_style(&self) -> Style {
        if self.focused {
            self.focused_border_style
        } else {
            tp::unfocused_border()
        }
    }

    fn border_type(&self) -> BorderType {
        if self.focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        }
    }

    /// Map a [`MessageRole`] to its display colour.
    fn role_color(role: &MessageRole) -> Color {
        match role {
            MessageRole::User => tp::ACCENT_BRIGHT,
            MessageRole::Assistant => tp::TEXT,
            MessageRole::Info => tp::INFO,
            MessageRole::Success => tp::SUCCESS,
            MessageRole::Warning => tp::WARN,
            MessageRole::Error => tp::ERROR,
            MessageRole::System => tp::MUTED,
        }
    }

    /// Copy text to the system clipboard using platform-native tools.
    fn copy_to_clipboard(text: &str) -> Result<()> {
        use anyhow::Context;
        use std::io::Write;
        use std::process::{Command, Stdio};

        #[cfg(target_os = "macos")]
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .context("Failed to launch pbcopy")?;

        #[cfg(target_os = "linux")]
        let mut child = {
            Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()
                .or_else(|_| {
                    Command::new("xsel")
                        .arg("--clipboard")
                        .stdin(Stdio::piped())
                        .spawn()
                })
                .context("Failed to launch xclip or xsel")?
        };

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        anyhow::bail!("Clipboard not supported on this platform");

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
        Ok(())
    }

    /// Parse inline markdown into styled [`Span`]s.
    ///
    /// Supports: **bold**, *italic*, `code`, and ### headings (prefix only).
    fn parse_inline_markdown(text: &str, base_color: Color) -> Vec<Span<'static>> {
        let mut spans = Vec::new();

        // Handle heading prefixes
        let text = if text.starts_with("### ") {
            spans.push(Span::styled(
                "▎ ",
                Style::default().fg(tp::ACCENT_DIM),
            ));
            &text[4..]
        } else if text.starts_with("## ") {
            spans.push(Span::styled(
                "▎ ",
                Style::default().fg(tp::ACCENT_BRIGHT).add_modifier(Modifier::BOLD),
            ));
            &text[3..]
        } else if text.starts_with("# ") {
            spans.push(Span::styled(
                "▎ ",
                Style::default().fg(tp::ACCENT).add_modifier(Modifier::BOLD),
            ));
            &text[2..]
        } else {
            text
        };

        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;
        let mut buf = String::new();

        let base = Style::default().fg(base_color);
        let bold = base.add_modifier(Modifier::BOLD);
        let italic = base.add_modifier(Modifier::ITALIC);
        let code = Style::default().fg(tp::ACCENT_BRIGHT).bg(tp::SURFACE_BRIGHT);

        while i < len {
            // Backtick code
            if chars[i] == '`' {
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), base));
                    buf.clear();
                }
                i += 1;
                let start = i;
                while i < len && chars[i] != '`' {
                    i += 1;
                }
                let code_text: String = chars[start..i].iter().collect();
                spans.push(Span::styled(format!(" {} ", code_text), code));
                if i < len {
                    i += 1; // skip closing `
                }
                continue;
            }

            // **bold**
            if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), base));
                    buf.clear();
                }
                i += 2;
                let start = i;
                while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '*') {
                    i += 1;
                }
                let bold_text: String = chars[start..i].iter().collect();
                spans.push(Span::styled(bold_text, bold));
                if i + 1 < len {
                    i += 2; // skip closing **
                }
                continue;
            }

            // *italic*
            if chars[i] == '*' {
                if !buf.is_empty() {
                    spans.push(Span::styled(buf.clone(), base));
                    buf.clear();
                }
                i += 1;
                let start = i;
                while i < len && chars[i] != '*' {
                    i += 1;
                }
                let italic_text: String = chars[start..i].iter().collect();
                spans.push(Span::styled(italic_text, italic));
                if i < len {
                    i += 1; // skip closing *
                }
                continue;
            }

            buf.push(chars[i]);
            i += 1;
        }

        if !buf.is_empty() {
            spans.push(Span::styled(buf, base));
        }

        spans
    }
}

impl Pane for MessagesPane {
    fn height_constraint(&self) -> Constraint {
        Constraint::Fill(3)
    }

    fn update(&mut self, action: Action, state: &mut PaneState<'_>) -> Result<Option<Action>> {
        match action {
            Action::Focus => {
                self.focused = true;
                let status = "[j/k → scroll] [c → copy] [/help → commands]";
                return Ok(Some(Action::TimedStatusLine(status.into(), 3)));
            }
            Action::UnFocus => {
                self.focused = false;
            }
            Action::Down => {
                // Total items = messages + optional loading line
                let total = state.messages.len()
                    + if state.loading_line.is_some() { 1 } else { 0 };
                if self.scroll_offset + 1 < total {
                    self.scroll_offset += 1;
                }
            }
            Action::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            Action::Update => {
                // Auto-scroll to bottom on new messages (and loading line)
                let total = state.messages.len()
                    + if state.loading_line.is_some() { 1 } else { 0 };
                if total > 0 {
                    self.scroll_offset = total.saturating_sub(1);
                }
            }
            Action::Tick => {
                // Keep the loading line pinned to the bottom while active
                if state.loading_line.is_some() {
                    let total = state.messages.len() + 1;
                    self.scroll_offset = total.saturating_sub(1);
                }
            }
            Action::CopyMessage => {
                if let Some(msg) = state.messages.get(self.scroll_offset) {
                    match Self::copy_to_clipboard(&msg.content) {
                        Ok(()) => {
                            return Ok(Some(Action::TimedStatusLine(
                                "Copied to clipboard ✓".into(),
                                2,
                            )));
                        }
                        Err(e) => {
                            return Ok(Some(Action::TimedStatusLine(
                                format!("Copy failed: {}", e),
                                3,
                            )));
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, state: &PaneState<'_>) -> Result<()> {
        let mut items: Vec<ListItem> = state
            .messages
            .iter()
            .map(|msg| {
                let color = Self::role_color(&msg.role);
                let icon = msg.role.icon();

                let mut spans = vec![
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                ];

                // For assistant messages, render inline markdown
                if matches!(msg.role, MessageRole::Assistant) {
                    spans.extend(Self::parse_inline_markdown(&msg.content, color));
                } else {
                    spans.push(Span::styled(&msg.content, Style::default().fg(color)));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        // Append the animated loading line at the bottom when active
        if let Some(ref line) = state.loading_line {
            items.push(
                ListItem::new(line.as_str())
                    .style(Style::default().fg(tp::ACCENT_BRIGHT)),
            );
        }

        let total = items.len();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_symbol(symbols::scrollbar::HORIZONTAL.end)
            .highlight_spacing(ratatui::widgets::HighlightSpacing::Always)
            .highlight_style(tp::selected());

        let mut list_state = ListState::default().with_selected(Some(self.scroll_offset));
        frame.render_stateful_widget(list, area, &mut list_state);

        let title_style = if self.focused {
            tp::title_focused()
        } else {
            tp::title_unfocused()
        };

        frame.render_widget(
            Block::default()
                .title(Span::styled(" Messages ", title_style))
                .borders(Borders::ALL)
                .border_style(self.border_style())
                .border_type(self.border_type())
                .title_bottom(
                    Line::from(Span::styled(
                        format!(
                            " {} of {} ",
                            self.scroll_offset.saturating_add(1),
                            total
                        ),
                        Style::default().fg(tp::MUTED),
                    ))
                    .right_aligned(),
                ),
            area,
        );

        Ok(())
    }
}
