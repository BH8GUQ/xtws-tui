//! Ratatui-based terminal interface and keyboard-driven interaction logic.
//!
//! The state in this module owns the panes, selection cursor, and focus transitions
//! used to review received messages and choose a reply target.

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use super::feed::{ColorKey, DecodeTag, UiFeed};

#[derive(Clone, Copy, PartialEq)]
pub enum Focus {
    Left,
    Right,
}

pub struct UiLine {
    pub text: String,
    pub selectable: bool,
    pub tag: Option<DecodeTag>,
    pub slot_index: Option<u64>,
}

pub struct UiState {
    pub left_lines: Vec<UiLine>,
    pub right_lines: Vec<UiLine>,
    pub left_cursor: usize,
    pub left_scroll: usize,
    pub right_cursor: usize,
    pub right_scroll: usize,
    pub focus: Focus,
    pub header: String,
    pub should_quit: bool,
    pub visible_height: usize,
    pub my_call: String,
    pub pending: Option<(String, u64)>,
    pub pending_cq: bool,
    pub render_gen: u64,
    pub rendered_gen: u64,
}

impl UiState {
    pub fn new(my_call: String) -> Self {
        Self {
            left_lines: Vec::new(),
            right_lines: Vec::new(),
            left_cursor: 0,
            left_scroll: 0,
            right_cursor: 0,
            right_scroll: 0,
            focus: Focus::Left,
            header: String::new(),
            should_quit: false,
            visible_height: 0,
            my_call,
            pending: None,
            pending_cq: false,
            render_gen: 0,
            rendered_gen: 0,
        }
    }

    pub fn set_left_lines(&mut self, raw: Vec<(String, Option<DecodeTag>, Option<u64>)>) {
        let was_bottom = left_at_bottom(self);
        self.left_lines = raw
            .into_iter()
            .map(|(text, tag, slot_index)| {
                let selectable = !text.starts_with("---");
                UiLine {
                    text,
                    selectable,
                    tag,
                    slot_index,
                }
            })
            .collect();
        if self.left_lines.is_empty() {
            self.left_cursor = 0;
            self.left_scroll = 0;
        } else if was_bottom {
            if let Some(last) = last_selectable(&self.left_lines) {
                self.left_cursor = last;
            }
        } else if self.left_cursor >= self.left_lines.len() {
            self.left_cursor = last_selectable(&self.left_lines).unwrap_or(0);
        }
        self.render_gen += 1;
    }

    pub fn set_right_lines(&mut self, raw: Vec<(String, Option<u64>)>) {
        let was_bottom = right_at_bottom(self);
        self.right_lines = raw
            .into_iter()
            .map(|(text, slot_index)| {
                let selectable = !text.starts_with("---");
                UiLine {
                    text,
                    selectable,
                    tag: None,
                    slot_index,
                }
            })
            .collect();
        if self.right_lines.is_empty() {
            self.right_cursor = 0;
            self.right_scroll = 0;
        } else if was_bottom {
            if let Some(last) = last_selectable(&self.right_lines) {
                self.right_cursor = last;
            }
        } else if self.right_cursor >= self.right_lines.len() {
            self.right_cursor = last_selectable(&self.right_lines).unwrap_or(0);
        }
        self.render_gen += 1;
    }

    fn ensure_cursor_visible(&mut self) {
        let visible = self.visible_height;
        if visible == 0 {
            return;
        }
        match self.focus {
            Focus::Left => ensure_visible(
                &self.left_lines,
                &mut self.left_cursor,
                &mut self.left_scroll,
                visible,
            ),
            Focus::Right => ensure_visible(
                &self.right_lines,
                &mut self.right_cursor,
                &mut self.right_scroll,
                visible,
            ),
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Left => move_up_in(&self.left_lines, &mut self.left_cursor, &mut self.left_scroll),
            Focus::Right => move_up_in(&self.right_lines, &mut self.right_cursor, &mut self.right_scroll),
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Left => move_down_in(
                &self.left_lines,
                &mut self.left_cursor,
                &mut self.left_scroll,
                self.visible_height,
            ),
            Focus::Right => move_down_in(
                &self.right_lines,
                &mut self.right_cursor,
                &mut self.right_scroll,
                self.visible_height,
            ),
        }
    }

    fn confirm(&mut self) {
        match self.focus {
            Focus::Left => {
                if let Some(line) = self.left_lines.get(self.left_cursor) {
                    if line.selectable {
                        if let (Some(call), Some(slot)) = (
                            super::feed::extract_reply_call_from_line(&line.text, &self.my_call),
                            line.slot_index,
                        ) {
                            self.pending = Some((call, slot));
                        }
                        if let Some(last) = last_selectable(&self.left_lines) {
                            self.left_cursor = last;
                            self.left_scroll = 0;
                        }
                    }
                }
            }
            Focus::Right => {
                if let Some(last) = last_selectable(&self.right_lines) {
                    self.right_cursor = last;
                    self.right_scroll = 0;
                }
            }
        }
    }
}

fn last_selectable(lines: &[UiLine]) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.selectable)
        .map(|(index, _)| index)
}

fn prev_selectable(lines: &[UiLine], from: usize) -> Option<usize> {
    let end = from.min(lines.len());
    if end == 0 {
        return None;
    }

    lines[..end]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.selectable)
        .map(|(index, _)| index)
}

fn next_selectable(lines: &[UiLine], from: usize) -> Option<usize> {
    let start = from.saturating_add(1).min(lines.len());
    if start >= lines.len() {
        return None;
    }

    lines[start..]
        .iter()
        .enumerate()
        .find(|(_, line)| line.selectable)
        .map(|(offset, _)| start + offset)
}

fn left_at_bottom(s: &UiState) -> bool {
    if s.left_lines.is_empty() {
        return true;
    }
    last_selectable(&s.left_lines) == Some(s.left_cursor)
}

fn right_at_bottom(s: &UiState) -> bool {
    if s.right_lines.is_empty() {
        return true;
    }
    last_selectable(&s.right_lines) == Some(s.right_cursor)
}

fn ensure_visible(lines: &[UiLine], cursor: &mut usize, scroll: &mut usize, visible: usize) {
    if lines.is_empty() || visible == 0 {
        return;
    }
    if *cursor < *scroll {
        *scroll = *cursor;
    } else if *cursor >= *scroll + visible {
        *scroll = *cursor + 1 - visible;
    }
    let max_scroll = lines.len().saturating_sub(visible);
    if *scroll > max_scroll {
        *scroll = max_scroll;
    }
}

fn move_up_in(lines: &[UiLine], cursor: &mut usize, scroll: &mut usize) {
    if let Some(prev) = prev_selectable(lines, *cursor) {
        *cursor = prev;
    } else if *scroll > 0 {
        *scroll -= 1;
    }
}

fn move_down_in(lines: &[UiLine], cursor: &mut usize, scroll: &mut usize, visible: usize) {
    if let Some(next) = next_selectable(lines, *cursor) {
        *cursor = next;
    } else if *scroll + visible < lines.len() {
        *scroll += 1;
    }
}

pub fn run_tui(
    state: Arc<Mutex<UiState>>,
    feed: UiFeed,
    config: crate::config::UiConfig,
    shutdown: Arc<AtomicBool>,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick = Duration::from_millis(config.refresh_ms as u64);

    loop {
        let size = terminal.size()?;
        let visible = size.height.saturating_sub(1) as usize;
        {
            let mut s = state.lock().unwrap();
            s.header = feed.header(SystemTime::now());
            s.visible_height = visible;
            s.ensure_cursor_visible();
            terminal.draw(|f| draw(f, &s, &config))?;
            s.rendered_gen = s.render_gen;
            if s.should_quit {
                break;
            }
        }

        if event::poll(tick)? {
            if let Event::Key(key) = event::read()? {
                let mut s = state.lock().unwrap();
                match key.code {
                    KeyCode::Char('q') => {
                        s.should_quit = true;
                        shutdown.store(true, Ordering::Relaxed);
                    }
                    KeyCode::Char('d') => s.pending_cq = true,
                    KeyCode::Left => s.focus = Focus::Left,
                    KeyCode::Right => s.focus = Focus::Right,
                    KeyCode::Up => s.move_up(),
                    KeyCode::Down => s.move_down(),
                    KeyCode::Enter => s.confirm(),
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw(f: &mut ratatui::Frame, s: &UiState, config: &crate::config::UiConfig) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    f.render_widget(
        Paragraph::new(s.header.as_str()).style(Style::default().fg(Color::Cyan)),
        chunks[0],
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(config.left_percent),
            Constraint::Percentage(100 - config.left_percent),
        ])
        .split(chunks[1]);

    render_pane(
        f,
        body[0],
        &s.left_lines,
        s.left_cursor,
        s.left_scroll,
        s.focus == Focus::Left,
        config,
    );
    render_pane(
        f,
        body[1],
        &s.right_lines,
        s.right_cursor,
        s.right_scroll,
        s.focus == Focus::Right,
        config,
    );
}

fn render_pane(
    f: &mut ratatui::Frame,
    area: Rect,
    lines: &[UiLine],
    cursor: usize,
    scroll: usize,
    focused: bool,
    config: &crate::config::UiConfig,
) {
    let visible_rows = area.height as usize;
    let start = scroll.min(lines.len());
    let end = (start + visible_rows).min(lines.len());
    let width = area.width.saturating_sub(1) as usize;

    let items: Vec<ListItem> = lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let abs = start + i;
            let selected = focused && abs == cursor && line.selectable;

            let mut text = line.text.clone();
            if let Some(ref tag) = line.tag {
                if !tag.label.is_empty() {
                    let cur_len = text.chars().count();
                    let label_len = tag.label.chars().count();
                    let pad = width.saturating_sub(cur_len + label_len + 1);
                    text.push_str(&" ".repeat(pad));
                    text.push(' ');
                    text.push_str(&tag.label);
                }
            }

            let text_style = if !line.selectable {
                Style::default().fg(Color::DarkGray)
            } else if let Some(ref tag) = line.tag {
                match color_from_key(tag.color_key, config) {
                    Some(c) => Style::default().fg(c),
                    _ => Style::default(),
                }
            } else {
                Style::default()
            };

            let mut spans = vec![Span::styled(text, text_style)];
            if selected {
                spans.push(Span::styled(
                    "   <(--)",
                    Style::default().fg(Color::Rgb(255, 165, 0)),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let block = if focused {
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default().borders(Borders::LEFT)
    };

    f.render_widget(List::new(items).block(block), area);
}

fn color_from_key(key: ColorKey, config: &crate::config::UiConfig) -> Option<Color> {
    let name = match key {
        ColorKey::NewCall => &config.color_new_call,
        ColorKey::NewGrid => &config.color_new_grid,
        ColorKey::NewDxcc => &config.color_new_dxcc,
        ColorKey::Working => &config.color_working,
        ColorKey::Worked => &config.color_worked,
    };
    parse_color_name(name)
}

fn parse_color_name(s: &str) -> Option<Color> {
    Some(match s {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" => Color::Gray,
        "darkgray" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        "orange" => Color::Rgb(255, 165, 0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{next_selectable, prev_selectable, UiLine};

    fn make_lines() -> Vec<UiLine> {
        vec![
            UiLine { text: "one".to_string(), selectable: true, tag: None, slot_index: None },
            UiLine { text: "two".to_string(), selectable: false, tag: None, slot_index: None },
            UiLine { text: "three".to_string(), selectable: true, tag: None, slot_index: None },
        ]
    }

    #[test]
    fn prev_selectable_handles_cursor_at_end() {
        let lines = make_lines();
        assert_eq!(prev_selectable(&lines, lines.len()), Some(2));
    }

    #[test]
    fn next_selectable_handles_cursor_at_end() {
        let lines = make_lines();
        assert_eq!(next_selectable(&lines, lines.len() - 1), None);
    }
}
