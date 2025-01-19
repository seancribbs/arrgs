use clap::{Parser, ValueEnum};

mod exec;
mod split_input;

use std::io;
use std::io::Read;

use rayon::prelude::*;

use crate::split_input::Splitter;

#[derive(Default, ValueEnum, Copy, Clone, PartialEq, Eq, Debug)]
enum Mode {
    #[default]
    Simple,
    Grouped,
    Interactive,
}

#[derive(Parser, Debug)]
struct Args {
    /// Use null-separated inputs, e.g. output from `find -0`
    #[arg(short = '0', long)]
    nul: bool,

    /// Number of inputs to pass to the sub-command at a time
    #[arg(short = 'n', long, default_value = "1")]
    nargs: usize,

    /// Display mode
    #[arg(short = 'm', long, value_enum, default_value_t = Mode::Simple)]
    mode: Mode,

    /// The program to invoke for each set of inputs
    program: String,

    /// Additional arguments to the program. Inputs read from stdin are added
    /// after these. arguments.
    program_args: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Args::parse();

    match options.mode {
        Mode::Simple => simple(options),
        Mode::Grouped => todo!(),
        Mode::Interactive => todo!(),
    }
}

fn simple(options: Args) -> Result<(), Box<dyn std::error::Error>> {
    let mut input_buffer = vec![];
    io::stdin().read_to_end(&mut input_buffer)?;

    let inputs = if options.nul {
        Splitter::null(&input_buffer)
    } else {
        Splitter::whitespace(&input_buffer)
    };

    inputs.chunks(options.nargs).par_bridge().for_each(|items| {
        exec::exec(&options.program, &options.program_args, items);
    });
    Ok(())
}
