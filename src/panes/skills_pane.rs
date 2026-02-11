use anyhow::Result;
use ratatui::{
    layout::{Constraint, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem},
};

use crate::action::Action;
use crate::panes::{Pane, PaneState};
use crate::tui::Frame;

pub struct SkillsPane {
    focused: bool,
    focused_border_style: Style,
}

impl SkillsPane {
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

impl Pane for SkillsPane {
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
                    "[skills pane focused]".into(),
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
        let skills = state.skill_manager.get_skills();
        let items: Vec<ListItem> = skills
            .iter()
            .map(|s| {
                let status = if s.enabled { "✓" } else { "✗" };
                let text = format!(
                    "{} {} - {}",
                    status,
                    s.name,
                    s.description.as_deref().unwrap_or("No description")
                );
                ListItem::new(text)
            })
            .collect();

        let skills_list = List::new(items)
            .block(
                Block::default()
                    .title("Skills")
                    .borders(Borders::ALL)
                    .border_style(self.border_style())
                    .border_type(self.border_type()),
            )
            .style(Style::default().fg(Color::White));

        frame.render_widget(skills_list, area);
        Ok(())
    }
}
