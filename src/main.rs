use std::{env, error::Error, ffi::OsString, process};

use toy_payments_engine::Engine;

fn run() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();
    let file_path = get_first_arg()?;

    engine.run(&file_path)
}

/// Returns the first positional argument sent to this process. If there are no
/// positional arguments, then this returns an error.
fn get_first_arg() -> Result<OsString, Box<dyn Error>> {
    match env::args_os().nth(1) {
        None => Err(From::from("expected 1 argument, but got none")),
        Some(file_path) => Ok(file_path),
    }
}

fn main() {
    if let Err(err) = run() {
        println!("{}", err);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests;
