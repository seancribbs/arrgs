// - [gray text args] ---------
// lines of output
// - Check [green text args] ----
// - X [red text args] -----
// lines of output

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::prelude::*;
use ratatui::symbols::border;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::DefaultTerminal;

use crate::split_input::Splitter;

#[derive(Debug, Default)]
struct App {
    processes: Vec<Process>,
    exit: bool,
}

enum AppEvent {
    KeyEvent(crossterm::event::KeyEvent),
    Input(Vec<String>),
    Output { pid: usize, lines: Vec<String> },
    Exit { pid: usize, status: ProcessStatus },
}

impl App {
    fn run(
        &mut self,
        options: crate::Options,
        terminal: &mut DefaultTerminal,
        input_program: Child,
    ) -> anyhow::Result<()> {
        let (sender, mut receiver) = std::sync::mpsc::channel::<AppEvent>();

        let _keyboard_thread = spawn_keyboard_events_thread(&sender);
        let _input_thread = spawn_input_process(&sender, input_program, &options);

        while !self.exit {
            self.handle_events(&mut receiver, &sender, &options)?;
            terminal.draw(|frame| self.draw(frame))?;
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
        match key_event.code {
            KeyCode::Char('q') if key_event.kind == KeyEventKind::Press => self.exit = true,
            // KeyCode::Left => self.decrement_counter(),
            // KeyCode::Right => self.increment_counter(),
            _ => {}
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
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
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
                        let mut stdout = child.stdout.as_mut().map(BufReader::new).unwrap();
                        let mut buffer = String::new();
                        match stdout.read_line(&mut buffer) {
                            Ok(0) => continue,
                            Ok(_amount) => {
                                let _ = process_tx.send(AppEvent::Output {
                                    pid,
                                    lines: vec![buffer],
                                });
                            }
                            Err(e) => (),
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
            handle,
        });
    }

    fn handle_output_event(&mut self, pid: usize, lines: Vec<String>) {
        self.processes[pid].output_lines.extend(lines);
    }

    fn handle_exit_event(&mut self, pid: usize, status: ProcessStatus) {
        self.processes[pid].status = Some(status);
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

fn spawn_input_process(
    sender: &Sender<AppEvent>,
    mut input_program: Child,
    options: &crate::Options,
) -> JoinHandle<()> {
    let inputs_tx = sender.clone();
    let options = options.clone();
    std::thread::spawn(move || {
        let mut input_buffer = vec![];
        input_program
            .stdout
            .as_mut()
            .unwrap()
            .read_to_end(&mut input_buffer)
            .expect("could not read stdout from input process");
        let inputs = if options.nul {
            Splitter::null(&input_buffer)
        } else {
            Splitter::whitespace(&input_buffer)
        };
        for chunk in inputs.chunks(options.nargs) {
            let chunk_inputs = chunk.into_iter().map(|s| s.to_string()).collect();
            let _ = inputs_tx.send(AppEvent::Input(chunk_inputs));
        }
        input_program.wait().unwrap();
    })
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let layout = Layout::vertical((0..self.processes.len()).map(|_| Constraint::Max(5)));
        for (rect, process) in layout.split(area).iter().zip(self.processes.iter()) {
            process.render(*rect, buf);
        }
    }
}

#[derive(Debug)]
struct Process {
    args: Vec<String>,
    output_lines: Vec<String>,
    status: Option<ProcessStatus>,
    handle: JoinHandle<()>,
}

impl Widget for &Process {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let title: String = self
            .args
            .iter()
            .map(|s| s.as_str())
            .intersperse(" ")
            .collect();
        let title_style = match self.status {
            None => Color::Gray,
            Some(ProcessStatus::Success) => Color::Green,
            Some(_) => Color::Red,
        };
        let contents: Text = self.output_lines.iter().map(|s| s.as_str()).collect();
        let block = Block::default()
            .title(title)
            .title_style(title_style)
            .borders(Borders::TOP);
        Paragraph::new(contents).block(block).render(area, buf);
    }
}

#[derive(Clone, Debug)]
pub enum ProcessStatus {
    Success,
    Failure(i32),
    Signal(std::process::ExitStatus),
}

pub fn run(options: crate::Options) -> anyhow::Result<()> {
    let mut input_program = Command::new("echo")
        .args(["1", "2", "3", "4", "5"])
        .stdout(Stdio::piped())
        // .stderr(Stdio::piped())
        .spawn()?;
    let mut terminal = ratatui::init();
    let result = App::default().run(options, &mut terminal, input_program);
    ratatui::restore();
    result
}
