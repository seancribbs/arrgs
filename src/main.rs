#![feature(iter_intersperse)]

use std::io::{stdin, Read};

use clap::{Parser, ValueEnum};
use exec::{Executor, Parallel, Sequential};
use split_input::Splitter;

mod exec;
mod interactive;
mod split_input;

#[derive(Default, ValueEnum, Copy, Clone, PartialEq, Eq, Debug)]
enum Mode {
    #[default]
    #[value(alias("sequential"))]
    Simple,
    Parallel,
    Interactive,
}

#[derive(Parser, Debug, Clone)]
struct Options {
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

fn main() -> anyhow::Result<()> {
    let options = Options::parse();
    let mut input_buffer = vec![];
    if options.mode == Mode::Interactive {
        return interactive::run(options);
    }
    stdin().read_to_end(&mut input_buffer)?;
    let inputs = if options.nul {
        Splitter::null(&input_buffer)
    } else {
        Splitter::whitespace(&input_buffer)
    };
    match options.mode {
        Mode::Simple => Sequential.execute(&options, inputs).map(|_| ()),
        Mode::Parallel => Parallel.execute(&options, inputs).map(|_| ()),
        Mode::Interactive => unreachable!(),
    }
}
