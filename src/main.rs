use csv::Trim;
use std::{env, error::Error, ffi::OsString, fs::File, io, process};

mod account;
mod engine;
mod transaction;

use crate::account::AccountSer;
use crate::engine::Engine;
use crate::transaction::*;

fn run() -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new();

    let file_path = get_first_arg()?;
    let file = File::open(file_path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .trim(Trim::All)
        .flexible(true)
        .from_reader(file);
    // input
    // ignores failed to be parsed entries
    for entry in rdr.deserialize().flatten() {
        // load
        let mut tx: Transaction = entry;
        let s = Box::new(Received);
        if tx.init(s).is_ok() {
            // process
            // infalible run, we ignore errors,
            // faulty transactions are simply discarded
            let _ = engine.process(tx);
        }
    }

    // output
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(io::stdout());
    for client in engine.accounts() {
        wtr.serialize(AccountSer::from(*client))?
    }
    wtr.flush()?;

    Ok(())
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
