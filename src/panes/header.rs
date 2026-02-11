use anyhow::Result;
use ratatui::{
    layout::{Constraint, Rect},
    prelude::*,
};

use crate::panes::{Pane, PaneState};
use crate::tui::Frame;

#[derive(Default)]
pub struct HeaderPane {}

impl HeaderPane {
    pub fn new() -> Self {
        Self {}
    }
}

impl Pane for HeaderPane {
    fn height_constraint(&self) -> Constraint {
        Constraint::Max(1)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, state: &PaneState<'_>) -> Result<()> {
        let soul_name = state
            .soul_manager
            .get_content()
            .and_then(|c| c.lines().find(|l| l.starts_with("# ")).map(|l| l.trim_start_matches("# ").to_string()))
            .unwrap_or_else(|| "RustyClaw".to_string());

        frame.render_widget(
            Line::from(vec![
                Span::styled(
                    format!("[ {} ", soul_name),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{} ", symbols::DOT),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("v{} ", env!("CARGO_PKG_VERSION")),
                    Style::default().fg(Color::LightCyan),
                ),
                Span::styled("]", Style::default().fg(Color::Cyan)),
            ])
            .right_aligned(),
            area,
        );

        Ok(())
    }
}
