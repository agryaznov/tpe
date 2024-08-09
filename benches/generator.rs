use std::{error::Error, ffi::OsString, fs::create_dir_all, fs::File, io::Write};

/// Input CSV files generator for benchmarks.
/// Generates a accounts having t transactions each,
/// saves it all to CSV file, and returns its path.
pub fn generate(a: u16, t: u16) -> Result<OsString, Box<dyn Error>> {
    let dirname = "fixtures/benches/";
    let filename = format!("bench_{a}x{t}.csv");

    let mut fixture = OsString::new();
    fixture.push(dirname);

    create_dir_all(&fixture)?;
    fixture.push(filename.as_str());

    let mut file = File::create_new(&fixture)?;
    write!(file, "type, client, tx, amount")?;

    // just deposit, in arithmetic progression from a to a+t
    // TODO add other txs
    let mut tx = 0;
    for client in 0..a {
        for amount in a..a + t {
            tx += 1;
            write!(file, "deposit, {client}, {tx}, {amount}")?;
        }
    }

    Ok(fixture)
}
