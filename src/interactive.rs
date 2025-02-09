// - [gray text args] ---------
// lines of output
// - Check [green text args] ----
// - X [red text args] -----
// lines of output

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::ops::Deref;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::DefaultTerminal;

use crate::split_input::Splitter;

#[derive(Debug, Default)]
struct App {
    processes: Vec<Process>,
    exit: bool,
    selected: usize,
    scroll_position: (u16, u16),
    wrap: bool,
    expanded: bool,
    max_lines: u16,
    keys: VecDeque<KeyCode>,
}

enum AppEvent {
    KeyEvent(crossterm::event::KeyEvent),
    Input(Vec<String>),
    Output { pid: usize, lines: Vec<String> },
    Exit { pid: usize, status: ProcessStatus },
}

impl App {
    fn run<R: std::io::Read + Send + 'static>(
        &mut self,
        options: crate::Options,
        terminal: &mut DefaultTerminal,
        input: &Arc<Mutex<R>>,
    ) -> anyhow::Result<()> {
        let (sender, mut receiver) = std::sync::mpsc::channel::<AppEvent>();

        let _keyboard_thread = spawn_keyboard_events_thread(&sender);
        let _input_thread = spawn_input_process(&sender, input, &options);

        while !self.exit {
            terminal.draw(|frame| {
                self.max_lines = frame.area().height.saturating_sub(2);
                self.draw(frame)
            })?;
            self.handle_events(&mut receiver, &sender, &options)?;
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_events(
        &mut self,
        rx: &mut Receiver<AppEvent>,
        tx: &Sender<AppEvent>,
        options: &crate::Options,
    ) -> std::io::Result<()> {
        loop {
            match rx.try_recv() {
                Ok(event) => match event {
                    AppEvent::KeyEvent(key_event) => self.handle_key_event(key_event),
                    AppEvent::Input(inputs) => self.spawn_sub_process(inputs, tx, options),
                    AppEvent::Output { pid, lines } => self.handle_output_event(pid, lines),
                    AppEvent::Exit { pid, status } => self.handle_exit_event(pid, status),
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => panic!("all event senders disconnected"),
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Press {
            self.keys.push_front(key_event.code);
            self.keys.truncate(8);
            match key_event.code {
                KeyCode::Char('q') | KeyCode::Esc => self.exit = true,
                KeyCode::Char('/') => {
                    self.expanded = !self.expanded;
                    self.reset_scroll_position();
                }
                KeyCode::Char('w') => self.wrap = !self.wrap,
                KeyCode::PageUp => {
                    self.selected = self
                        .selected
                        .saturating_sub(1)
                        .min(self.processes.len().saturating_sub(1));
                    self.reset_scroll_position();
                }
                KeyCode::PageDown => {
                    self.selected = self
                        .selected
                        .saturating_add(1)
                        .min(self.processes.len().saturating_sub(1));
                    self.reset_scroll_position();
                }
                KeyCode::Up => {
                    self.scroll_position.0 = self.scroll_position.0.saturating_sub(1).min(
                        self.processes[self.selected]
                            .output_lines
                            .len()
                            .saturating_sub(1) as u16,
                    );
                }
                KeyCode::Down => {
                    self.scroll_position.0 = self.scroll_position.0.saturating_add(1).min(
                        self.processes[self.selected]
                            .output_lines
                            .len()
                            .saturating_sub(1) as u16,
                    );
                }
                KeyCode::Left => {
                    self.scroll_position.1 = if self.wrap {
                        0
                    } else {
                        self.scroll_position.1.saturating_sub(4)
                    };
                }
                KeyCode::Right => {
                    self.scroll_position.1 = if self.wrap {
                        0
                    } else {
                        self.scroll_position.1.saturating_add(4)
                    };
                }
                KeyCode::Home => {
                    self.scroll_position.0 = 0;
                }
                KeyCode::End => {
                    self.scroll_position.0 = self.processes[self.selected]
                        .output_lines
                        .len()
                        .saturating_sub(1) as u16;
                }
                _ => {}
            }
        }
    }

    fn spawn_sub_process(
        &mut self,
        inputs: Vec<String>,
        tx: &Sender<AppEvent>,
        options: &crate::Options,
    ) {
        let pid = self.processes.len();
        let args = inputs.clone();
        let process_tx = tx.clone();
        let options = options.clone();
        let handle = std::thread::spawn(move || {
            let mut child = Command::new(&options.program)
                .args(&options.program_args)
                .args(inputs)
                .stdout(Stdio::piped())
               // .stderr(Stdio::piped())
                .spawn()
                .expect("could not spawn output process");
            let mut stdout = child.stdout.take().map(BufReader::new).unwrap();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Read the rest of stdout
                        let mut buffer = String::new();
                        while let Ok(amount) = stdout.read_line(&mut buffer) {
                            if amount == 0 {
                                break;
                            }
                            let _ = process_tx.send(AppEvent::Output {
                                pid,
                                lines: vec![buffer.clone()],
                            });
                            buffer.clear();
                        }
                        // Capture the exit status
                        let process_status = if status.success() {
                            ProcessStatus::Success
                        } else {
                            status
                                .code()
                                .map(ProcessStatus::Failure)
                                .unwrap_or_else(|| ProcessStatus::Signal(status))
                        };
                        let _ = process_tx.send(AppEvent::Exit {
                            pid,
                            status: process_status,
                        });
                        break;
                    }
                    Ok(None) => {
                        // TODO: handle stderr
                        // Read stdout for output
                        let mut buffer = String::new();
                        if let Ok(amount) = stdout.read_line(&mut buffer) {
                            if amount == 0 {
                                continue;
                            }
                            let _ = process_tx.send(AppEvent::Output {
                                pid,
                                lines: vec![buffer],
                            });
                        }
                    }
                    Err(e) => {
                        panic!("could not wait on subprocess {e}")
                    }
                }
            }
        });
        self.processes.push(Process {
            args,
            output_lines: Default::default(),
            status: None,
            handle: Some(handle),
        });
        self.selected = self.processes.len() - 1;
    }

    fn handle_output_event(&mut self, pid: usize, lines: Vec<String>) {
        self.processes[pid].output_lines.extend(lines);
        if self.selected == pid {
            self.reset_scroll_position();
        }
    }

    fn handle_exit_event(&mut self, pid: usize, status: ProcessStatus) {
        self.processes[pid].status = Some(status);
        // TODO: maybe handle when a child thread panics?
        let _ = self.processes[pid].handle.take().unwrap().join();
    }

    fn reset_scroll_position(&mut self) {
        self.scroll_position = (
            if self.expanded {
                // We want to display the last N lines of the output
                // (where N is the height of the pane that we're rendering into)
                (self.processes[self.selected].output_lines.len() as u16)
                    .saturating_sub(self.max_lines)
            } else {
                self.processes[self.selected]
                    .output_lines
                    .len()
                    .saturating_sub(5) as u16
            },
            0,
        );
    }
}

fn spawn_keyboard_events_thread(sender: &Sender<AppEvent>) -> JoinHandle<()> {
    let events_tx = sender.clone();
    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if let Event::Key(key_event) = event {
                events_tx
                    .send(AppEvent::KeyEvent(key_event))
                    .expect("could not send to main thread");
            }
        }
    })
}

fn spawn_input_process<R: std::io::Read + Send + 'static>(
    sender: &Sender<AppEvent>,
    input: &Arc<Mutex<R>>,
    options: &crate::Options,
) -> JoinHandle<()> {
    let inputs_tx = sender.clone();
    let options = options.clone();
    let input = Arc::clone(input);
    std::thread::spawn(move || {
        let mut input_buffer = vec![];
        input
            .lock()
            .unwrap()
            .read_to_end(&mut input_buffer)
            .expect("could not read stdin");
        let inputs = if options.nul {
            Splitter::null(&input_buffer)
        } else {
            Splitter::whitespace(&input_buffer)
        };
        for chunk in inputs.chunks(options.nargs) {
            let chunk_inputs = chunk.into_iter().map(|s| s.to_string()).collect();
            let _ = inputs_tx.send(AppEvent::Input(chunk_inputs));
        }
    })
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if self.expanded {
            let layout = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]);
            let rects = layout.split(area);
            Paragraph::new(Text::from(format!(
                "Selected: {} Keys: {}",
                self.selected,
                self.keys
                    .iter()
                    .rev()
                    .map(|k| format!("{k:?}"))
                    .intersperse(String::from(" "))
                    .collect::<String>()
            )))
            .render(rects[0], buf);
            let process_widget = ProcessWidget {
                process: &self.processes[self.selected],
                scroll_position: Some(self.scroll_position),
                wrap: self.wrap,
            };
            process_widget.render(rects[1], buf);
        } else {
            let process_widgets: Vec<ProcessWidget<'_>> = self
                .processes
                .iter()
                .enumerate()
                .map(|(i, p)| ProcessWidget {
                    process: p,
                    scroll_position: (i == self.selected).then_some(self.scroll_position),
                    wrap: self.wrap,
                })
                .collect();

            let widget_constraints = process_widgets.iter().map(ProcessWidget::layout_constraint);
            let layout =
                Layout::vertical(std::iter::once(Constraint::Length(1)).chain(widget_constraints));
            let rects = layout.split(area);
            let mut areas = rects.iter();
            let first = areas.next().unwrap();
            Paragraph::new(Text::from(format!(
                "Selected: {} Keys: {}",
                self.selected,
                self.keys
                    .iter()
                    .rev()
                    .map(|k| format!("{k:?}"))
                    .intersperse(String::from(" "))
                    .collect::<String>()
            )))
            .render(*first, buf);
            for (rect, process) in areas.zip(process_widgets.iter()) {
                process.render(*rect, buf);
            }
        }
    }
}

#[derive(Debug)]
struct Process {
    args: Vec<String>,
    output_lines: Vec<String>,
    status: Option<ProcessStatus>,
    handle: Option<JoinHandle<()>>,
}

struct ProcessWidget<'a> {
    process: &'a Process,
    scroll_position: Option<(u16, u16)>,
    wrap: bool,
}

impl Deref for ProcessWidget<'_> {
    type Target = Process;

    fn deref(&self) -> &Self::Target {
        self.process
    }
}

impl ProcessWidget<'_> {
    fn layout_constraint(&self) -> Constraint {
        // Succeeded: 1 line title only
        if self.status == Some(ProcessStatus::Success) && self.scroll_position.is_none() {
            Constraint::Length(1)
        } else {
            // Failed or unfinished: up to 5 lines of text
            //   - title + min(output_lines.len(), 5)
            Constraint::Max(1 + self.output_lines.len().min(5) as u16)
        }
    }
}

impl Widget for &ProcessWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let title: String = self
            .args
            .iter()
            .map(|s| s.as_str())
            .chain(std::iter::once(
                format!("({})", self.output_lines.len()).as_str(),
            ))
            .intersperse(" ")
            .collect();
        let title_style = match self.status {
            None => Color::Gray,
            Some(ProcessStatus::Success) => Color::Green,
            Some(_) => Color::Red,
        };
        let border_style = if self.scroll_position.is_some() {
            Color::Yellow
        } else {
            Color::Gray
        };
        let contents: Text = if self.scroll_position.is_some() {
            self.output_lines.iter().map(|s| s.as_str()).collect()
        } else {
            Text::default()
        };
        let block = Block::default()
            .title(title)
            .title_style(Style::from(title_style).bg(Color::Black))
            .borders(Borders::TOP)
            .border_style(border_style);
        let mut para = Paragraph::new(contents).block(block);
        if self.wrap {
            para = para.wrap(Default::default());
        }
        if let Some(sp) = self.scroll_position {
            para = para.scroll(sp);
        }
        para.render(area, buf);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    Success,
    Failure(i32),
    Signal(std::process::ExitStatus),
}

pub fn run(options: crate::Options) -> anyhow::Result<()> {
    if options.simulate {
        let mut input_program = Command::new("echo")
            .args((1..10).map(|_| "loremipsum.txt"))
            .stdout(Stdio::piped())
            // .stderr(Stdio::piped())
            .spawn()?;
        let input = Arc::new(Mutex::new(input_program.stdout.take().unwrap()));
        let mut terminal = ratatui::try_init().context("initializing TUI")?;
        let result = App::default().run(options, &mut terminal, &input);
        ratatui::restore();
        input_program.wait().unwrap();
        result
    } else {
        let input = Arc::new(Mutex::new(std::io::stdin()));
        let mut terminal = ratatui::try_init().context("initializing TUI")?;
        let result = App::default().run(options, &mut terminal, &input);
        ratatui::restore();
        result
    }
}
