use anyhow::Result;
use ratatui::{
    layout::{Constraint, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem},
};

use crate::action::Action;
use crate::panes::{Pane, PaneState};
use crate::theme::tui_palette as tp;
use crate::tui::Frame;

pub struct SecretsPane {
    focused: bool,
    focused_border_style: Style,
}

impl SecretsPane {
    pub fn new(focused: bool, focused_border_style: Style) -> Self {
        Self {
            focused,
            focused_border_style,
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

impl Pane for SecretsPane {
    fn height_constraint(&self) -> Constraint {
        match self.focused {
            true => Constraint::Fill(3),
            false => Constraint::Fill(1),
        }
    }

    fn update(&mut self, action: Action, _state: &mut PaneState<'_>) -> Result<Option<Action>> {
        match action {
            Action::Focus => {
                self.focused = true;
                return Ok(Some(Action::TimedStatusLine(
                    "[secrets pane focused]".into(),
                    3,
                )));
            }
            Action::UnFocus => {
                self.focused = false;
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, state: &PaneState<'_>) -> Result<()> {
        let (access_label, access_style) = if state.secrets_manager.has_agent_access() {
            ("Enabled", Style::default().fg(tp::SUCCESS))
        } else {
            ("Disabled", Style::default().fg(tp::WARN))
        };

        let items: Vec<ListItem> = vec![
            ListItem::new(Line::from(vec![
                Span::styled("Agent Access: ", Style::default().fg(tp::TEXT_DIM)),
                Span::styled(access_label, access_style),
            ])),
            ListItem::new(""),
            ListItem::new(Span::styled("Commands:", Style::default().fg(tp::TEXT_DIM))),
            ListItem::new(Span::styled("  /enable-access", Style::default().fg(tp::ACCENT_BRIGHT))),
            ListItem::new(Span::styled("  /disable-access", Style::default().fg(tp::ACCENT_BRIGHT))),
        ];

        let title_style = if self.focused {
            tp::title_focused()
        } else {
            tp::title_unfocused()
        };

        let secrets_list = List::new(items).block(
            Block::default()
                .title(Span::styled(" Secrets ", title_style))
                .borders(Borders::ALL)
                .border_style(self.border_style())
                .border_type(self.border_type()),
        );

        frame.render_widget(secrets_list, area);
        Ok(())
    }
}
