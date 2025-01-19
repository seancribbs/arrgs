use clap::Parser;

mod split_input;

use std::io;
use std::io::Read;
use std::process::{self, Command};

use rayon::prelude::*;

use crate::split_input::Splitter;

#[derive(Parser, Debug)]
struct Args {
    /// Use null-separated inputs, e.g. output from `find -0`
    #[arg(short = '0', long)]
    nul: bool,

    /// Number of inputs to pass to the sub-command at a time
    #[arg(short = 'n', long, default_value = "1")]
    nargs: usize,

    /// The program to invoke for each set of inputs
    program: String,

    /// Additional arguments to the program. Inputs are added after these
    /// arguments.
    program_args: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    inputs.chunks(options.nargs).par_bridge().for_each(|items| {
        let status = Command::new(&options.program)
            .args(&options.program_args)
            .args(&items)
            .status()
            .expect("command could not be spawned");
        match status.code() {
            None | Some(0) => (),
            Some(code) => {
                eprintln!(
                    "Command {} {:?} {items:?} failed with status {code}",
                    &options.program, &options.program_args
                );
                process::exit(code);
            }
        }
    });
    Ok(())
}
