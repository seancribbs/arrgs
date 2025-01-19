use std::process::{self, Command};

pub fn exec(program: &str, program_args: &[String], items: Vec<&str>) {
    let status = Command::new(program)
        .args(program_args)
        .args(&items)
        .status()
        .expect("command could not be spawned");
    match status.code() {
        None | Some(0) => (),
        Some(code) => {
            eprintln!("Command {program} {program_args:?} {items:?} failed with status {code}");
            process::exit(code);
        }
    }
}
