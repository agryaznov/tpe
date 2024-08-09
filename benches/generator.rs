use std::{error::Error, ffi::OsString, fs::create_dir_all, fs::File, io::Write};

/// Input CSV files generator for benchmarks.
/// Generates a accounts having 4*t transactions each,
/// saves it all to CSV file, and returns its path.
pub fn generate(a: usize, t: usize) -> Result<OsString, Box<dyn Error>> {
    let dirname = "fixtures/benches/";
    let filename = format!("bench_{a}x4x{}.csv", t + 1);

    let mut fixture = OsString::new();
    fixture.push(dirname);

    create_dir_all(&fixture)?;
    fixture.push(filename.as_str());

    println!();
    print!("Generating {} entries, hang tight...", a * 4 * (t + 1));

    // generate fixture csv file if does not exist
    if let Ok(mut file) = File::create_new(&fixture) {
        writeln!(file, "type, client, tx, amount")?;
        // For each account we generate:
        //
        // + t+1 deposits (last one goes after chargeback and fails),
        // + t disputes,
        // + t-1 resolves,
        // + t-1 withdrawals
        // + 1 chargeback,
        //
        // totaling in 4*(t+1) transactions, (2*t + 1 of which are stored)
        //   for _every_ account.
        // Amounts go in series from `client` to `t`.
        let mut tx = 0;
        for client in 1..=a {
            for amount in client..client + t {
                tx += 1;
                writeln!(file, "deposit, {client}, {tx}, {amount}")?;
                writeln!(file, "dispute, {client}, {tx}")?;
                writeln!(file, "resolve, {client}, {tx}")?;
                tx += 1;
                writeln!(file, "withdrawal, {client}, {tx}, {amount}")?;
            }

            let amount = client + t;
            tx += 1;
            writeln!(file, "deposit, {client}, {tx}, {amount}")?;
            writeln!(file, "dispute, {client}, {tx}")?;
            writeln!(file, "chargeback, {client}, {tx}, 0")?;
            tx += 1;
            writeln!(file, "deposit, {client}, {tx}, {amount}")?;
        }
    }
    println!("done into {:?}", &fixture);
    Ok(fixture)
}
