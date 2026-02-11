use anyhow::Result;
use ratatui::{
    layout::{Constraint, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem},
};

use crate::action::Action;
use crate::panes::{Pane, PaneState};
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
        let agent_access = if state.secrets_manager.has_agent_access() {
            "Enabled"
        } else {
            "Disabled"
        };

        let lines = vec![
            format!("Agent Access: {}", agent_access),
            String::new(),
            "Commands (via footer):".to_string(),
            "  enable-access".to_string(),
            "  disable-access".to_string(),
        ];

        let items: Vec<ListItem> = lines.iter().map(|t| ListItem::new(t.as_str())).collect();

        let secrets_list = List::new(items).block(
            Block::default()
                .title("Secrets")
                .borders(Borders::ALL)
                .border_style(self.border_style())
                .border_type(self.border_type()),
        );

        frame.render_widget(secrets_list, area);
        Ok(())
    }
}
