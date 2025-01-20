use crate::{split_input::Splitter, Options};
use std::{process, thread, time::Duration};

/// A trait for anything that takes our `Options` struct as an argument
/// and returns a list of exit statuses of spawned child processes
pub trait Executor {
    fn execute(
        self,
        options: &Options,
        inputs: Splitter,
    ) -> anyhow::Result<Vec<process::ExitStatus>>;
}

/// Runs the child processes in sequence, waiting for each to finish before starting the next
pub struct Sequential;
impl Executor for Sequential {
    /// # Errors
    /// Will return an error if either:
    /// - The input buffer cannot be read from stdin
    /// - One of the child processes fails to start (at which point the function will return early)
    fn execute(
        self,
        options: &Options,
        inputs: Splitter,
    ) -> anyhow::Result<Vec<process::ExitStatus>> {
        inputs
            .chunks(options.nargs)
            .map(|child_args| {
                process::Command::new(&options.program)
                    .args(&options.program_args)
                    .args(child_args)
                    .stdin(process::Stdio::null()) // Make sure the child doesn't read from *our* stdin
                    .status()
                    .map_err(Into::into)
            })
            .collect()
    }
}

/// Runs the child processes in parallel, waiting for all to finish before returning
pub struct Parallel;
impl Executor for Parallel {
    /// # Errors
    /// Will only return an error if the input buffer cannot be read from stdin.
    /// Failures to start child processes are (currently) only handled by printing an error message to stderr.
    fn execute(
        self,
        options: &Options,
        inputs: Splitter,
    ) -> anyhow::Result<Vec<process::ExitStatus>> {
        let mut running = vec![];
        for child_args in inputs.chunks(options.nargs) {
            let child = process::Command::new(&options.program)
                .args(&options.program_args)
                .args(&child_args)
                .stdin(process::Stdio::null()) // Make sure the child doesn't read from *our* stdin
                .spawn();
            match child {
                Ok(child) => running.push(child),
                Err(e) => eprintln!(
                    "Failed to start process ({} {}): {e}",
                    options.program,
                    child_args.join(" ")
                ),
            }
        }

        let mut exited = Vec::with_capacity(running.len());
        let mut checked = Vec::with_capacity(running.len());
        while !running.is_empty() {
            // Wait for all child processes to finish
            while let Some(mut child) = running.pop() {
                // `Child.try_wait` is non-blocking, so is essentially a poll
                match child.try_wait() {
                    Ok(Some(status)) => exited.push(status), // Child process has exited
                    Ok(None) => checked.push(child),         // Child process is still running
                    Err(e) => eprintln!("Error checking child status ({child:?}): {e}"),
                }
            }
            // Sleep for a bit to avoid busy-waiting
            // 10ms is an arbitrary value, however ~16ms is enough for a 60fps refresh rate
            thread::sleep(Duration::from_millis(10));

            // Put the checked processes back into the running list, to check again
            running.extend(checked.drain(..));
        }
        Ok(exited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mode;
    use std::time::{Duration, Instant};
    const MOCK_STDIN: &[u8] = b"0.1 0.2 0.3";
    const TOTAL_SLEEP: f64 = 0.6;

    fn test_options(mode: Mode) -> Options {
        Options {
            nul: false,
            nargs: 1,
            mode,
            program: "sleep".to_string(),
            program_args: vec![],
        }
    }

    #[test]
    fn test_sequential() {
        let start_time = Instant::now();
        let statuses = Sequential
            .execute(
                &test_options(Mode::Simple),
                Splitter::whitespace(MOCK_STDIN),
            )
            .unwrap();
        let total_time = Instant::now() - start_time;
        assert_eq!(statuses.len(), 3);
        assert!(statuses.iter().all(|status| status.success()));
        // The total time should be *at least* the *sum* of all sleeps
        assert!(
            total_time >= Duration::from_secs_f64(TOTAL_SLEEP),
            "{total_time:?}"
        );
    }

    #[test]
    fn test_parallel() {
        let start_time = Instant::now();
        let statuses = Parallel
            .execute(
                &test_options(Mode::Parallel),
                Splitter::whitespace(MOCK_STDIN),
            )
            .unwrap();
        let total_time = Instant::now() - start_time;
        assert_eq!(statuses.len(), 3);
        assert!(statuses.iter().all(|status| status.success()));
        // The total time should only be as long as the longest sleep
        // Testing for *less* than the *sum* of all sleeps to account for variable system load
        assert!(
            total_time < Duration::from_secs_f64(TOTAL_SLEEP),
            "{total_time:?}"
        );
    }
}
