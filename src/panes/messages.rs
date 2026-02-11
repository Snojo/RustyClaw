use anyhow::Result;
use ratatui::{
    layout::{Constraint, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
};

use crate::action::Action;
use crate::panes::{Pane, PaneState};
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
            Style::default()
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
                if self.scroll_offset + 1 < state.messages.len() {
                    self.scroll_offset += 1;
                }
            }
            Action::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            Action::Update => {
                // Auto-scroll to bottom on new messages
                if !state.messages.is_empty() {
                    self.scroll_offset = state.messages.len().saturating_sub(1);
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, state: &PaneState<'_>) -> Result<()> {
        let items: Vec<ListItem> = state
            .messages
            .iter()
            .map(|m| ListItem::new(m.as_str()))
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_symbol(symbols::scrollbar::HORIZONTAL.end)
            .highlight_spacing(ratatui::widgets::HighlightSpacing::Always)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        let mut list_state = ListState::default().with_selected(Some(self.scroll_offset));
        frame.render_stateful_widget(list, area, &mut list_state);

        frame.render_widget(
            Block::default()
                .title("Messages")
                .borders(Borders::ALL)
                .border_style(self.border_style())
                .border_type(self.border_type())
                .title_bottom(
                    Line::from(format!(
                        "{} of {}",
                        self.scroll_offset.saturating_add(1),
                        state.messages.len()
                    ))
                    .right_aligned(),
                ),
            area,
        );

        Ok(())
    }
}
