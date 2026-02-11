use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::pages::Page;
use crate::panes::{
    config_pane::ConfigPane,
    messages::MessagesPane,
    secrets_pane::SecretsPane,
    skills_pane::SkillsPane,
    InputMode, Pane, PaneState,
};
use crate::tui::EventResponse;

pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    panes: Vec<Box<dyn Pane>>,
    focused_pane_index: usize,
    fullscreen_pane_index: Option<usize>,
}

impl Home {
    pub fn new() -> Result<Self> {
        let focused_border_style = Style::default().fg(Color::LightGreen);

        Ok(Self {
            command_tx: None,
            panes: vec![
                Box::new(MessagesPane::new(true, focused_border_style)),
                Box::new(SkillsPane::new(false, focused_border_style)),
                Box::new(SecretsPane::new(false, focused_border_style)),
                Box::new(ConfigPane::new(false, focused_border_style)),
            ],
            focused_pane_index: 0,
            fullscreen_pane_index: None,
        })
    }
}

impl Page for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn init(&mut self, state: &PaneState<'_>) -> Result<()> {
        for pane in &mut self.panes {
            pane.init(state)?;
        }
        Ok(())
    }

    fn focus(&mut self) -> Result<()> {
        Ok(())
    }

    fn handle_key_events(
        &mut self,
        key: KeyEvent,
        state: &mut PaneState<'_>,
    ) -> Result<Option<EventResponse<Action>>> {
        // When the user is typing in the input bar, don't capture keys here
        if state.input_mode == InputMode::Input {
            return Ok(None);
        }

        match key.code {
            KeyCode::Tab => {
                return Ok(Some(EventResponse::Stop(Action::FocusNext)));
            }
            KeyCode::BackTab => {
                return Ok(Some(EventResponse::Stop(Action::FocusPrev)));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                return Ok(Some(EventResponse::Stop(Action::Down)));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                return Ok(Some(EventResponse::Stop(Action::Up)));
            }
            KeyCode::Char('f') => {
                return Ok(Some(EventResponse::Stop(Action::ToggleFullScreen)));
            }
            _ => {}
        }

        Ok(None)
    }

    fn update(&mut self, action: Action, state: &mut PaneState<'_>) -> Result<Option<Action>> {
        match action {
            Action::FocusNext => {
                self.panes[self.focused_pane_index].update(Action::UnFocus, state)?;
                self.focused_pane_index = (self.focused_pane_index + 1) % self.panes.len();
                return self.panes[self.focused_pane_index].update(Action::Focus, state);
            }
            Action::FocusPrev => {
                self.panes[self.focused_pane_index].update(Action::UnFocus, state)?;
                self.focused_pane_index = self
                    .focused_pane_index
                    .checked_sub(1)
                    .unwrap_or(self.panes.len() - 1);
                return self.panes[self.focused_pane_index].update(Action::Focus, state);
            }
            Action::ToggleFullScreen => {
                self.fullscreen_pane_index = match self.fullscreen_pane_index {
                    Some(_) => None,
                    None => Some(self.focused_pane_index),
                };
            }
            Action::Tab(n) => {
                let idx = (n as usize).saturating_sub(1);
                if idx < self.panes.len() {
                    self.panes[self.focused_pane_index].update(Action::UnFocus, state)?;
                    self.focused_pane_index = idx;
                    return self.panes[self.focused_pane_index].update(Action::Focus, state);
                }
            }
            _ => {
                // Forward action to focused pane
                if let Some(result) =
                    self.panes[self.focused_pane_index].update(action, state)?
                {
                    return Ok(Some(result));
                }
            }
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, state: &PaneState<'_>) -> Result<()> {
        if let Some(fullscreen_idx) = self.fullscreen_pane_index {
            self.panes[fullscreen_idx].draw(frame, area, state)?;
        } else {
            // Two-column layout: left = messages, right = stacked (skills, secrets, config)
            let outer = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Fill(1), Constraint::Fill(1)])
                .split(area);

            // Left: messages pane takes full height
            self.panes[0].draw(frame, outer[0], state)?;

            // Right: stack the remaining panes vertically
            let right_constraints: Vec<Constraint> = self.panes[1..]
                .iter()
                .map(|p| p.height_constraint())
                .collect();

            let right_panes = Layout::default()
                .direction(Direction::Vertical)
                .constraints(right_constraints)
                .split(outer[1]);

            for (i, pane) in self.panes[1..].iter_mut().enumerate() {
                pane.draw(frame, right_panes[i], state)?;
            }
        }

        Ok(())
    }
}
