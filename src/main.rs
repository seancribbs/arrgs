use clap::Parser;

mod split_input;

use crate::split_input::Splitter;

use std::io;
use std::io::Read;
use std::process::{self, Command};

#[derive(Parser, Debug)]
struct Args {
    /// Use null-separated inputs, e.g. output from `find -0`
    #[arg(short = '0', long)]
    nul: bool,

    program: String,
    program_args: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // First version: 1 argument, one subprocess, sequential
    //
    let options = Args::parse();

    let mut input_buffer = vec![];
    io::stdin().read_to_end(&mut input_buffer)?;

    let inputs = if options.nul {
        Splitter::null(&input_buffer)
    } else {
        Splitter::whitespace(&input_buffer)
    };
    // Read inputs from stdin
    //   - whitespace-separated, or NUL-separated with a flag
    //   - options: number of arguments to pass, number of concurrent sub-processes
    for item in inputs {
        // For each batch of inputs:
        //   - spawn a sub-process, appending the inputs to its command-line args
        let status = Command::new(&options.program)
            .args(&options.program_args)
            .arg(item)
            .status()?;
        match status.code() {
            None | Some(0) => (),
            Some(code) => {
                eprintln!(
                    "Command {} {:?} {} failed with status {code}",
                    &options.program, &options.program_args, item
                );
                process::exit(code);
            }
        }
    }

    Ok(())
}
