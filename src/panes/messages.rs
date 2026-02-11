use anyhow::Result;
use ratatui::{
    layout::{Constraint, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
};

use crate::action::Action;
use crate::panes::{Pane, PaneState};
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
}

impl Pane for MessagesPane {
    fn height_constraint(&self) -> Constraint {
        Constraint::Fill(3)
    }

    fn update(&mut self, action: Action, state: &mut PaneState<'_>) -> Result<Option<Action>> {
        match action {
            Action::Focus => {
                self.focused = true;
                let status = "[j/k → scroll] [/help → commands]";
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
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, state: &PaneState<'_>) -> Result<()> {
        let mut items: Vec<ListItem> = state
            .messages
            .iter()
            .map(|m| {
                let style = if m.starts_with('▶') {
                    tp::user_message()
                } else if m.starts_with('(') || m.starts_with('[') {
                    tp::system_message()
                } else {
                    tp::gateway_message()
                };
                ListItem::new(m.as_str()).style(style)
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
