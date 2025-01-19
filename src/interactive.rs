// - [gray text args] ---------
// lines of output
// - Check [green text args] ----
// - X [red text args] -----
// lines of output

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::Block;
use ratatui::DefaultTerminal;
use symbols::border;

#[derive(Debug, Default)]
struct App {
    processes: Vec<Process>,
    exit: bool,
}

impl App {
    fn run(
        &mut self,
        options: crate::Options,
        terminal: &mut DefaultTerminal,
    ) -> anyhow::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_events(&mut self) -> std::io::Result<()> {
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit = true,
            // KeyCode::Left => self.decrement_counter(),
            // KeyCode::Right => self.increment_counter(),
            _ => {}
        }
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let title = Line::from(
            std::env::args()
                .intersperse(String::from(" "))
                .map(From::from)
                .collect::<Vec<_>>(),
        );

        let block = Block::bordered()
            .title(title.left_aligned().bold().white())
            .border_set(border::PLAIN);

        block.render(area, buf);
    }
}

#[derive(Debug)]
struct Process {
    args: Vec<String>,
    output_lines: Vec<String>,
    status: Option<ProcessStatus>,
}

#[derive(Clone, Debug)]
pub enum ProcessStatus {
    Success,
    Failure(i32),
    Signal(std::process::ExitStatus),
}

pub fn run(options: crate::Options) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let result = App::default().run(options, &mut terminal);
    ratatui::restore();
    result
}
