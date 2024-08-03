use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn integration_tests() {
    let inns = fs::read_dir("./fixtures/in").unwrap();

    for fixt in inns {
        run_fixture(fixt.unwrap())
    }
}

fn run_fixture(fixt: fs::DirEntry) {
    let fixt = fixt.path();
    let name = fixt.file_name().unwrap();

    let mut inn = PathBuf::new();
    inn.push("./fixtures/in/");
    inn.push(name);

    let mut out = PathBuf::new();
    out.push("./fixtures/out/");
    out.push(name);

    let res = Command::new("cargo")
        .arg("run")
        .arg("--")
        .arg(inn)
        .output()
        .expect("failed to execute process")
        .stdout;

    let expected = fs::read_to_string(out).expect("can't read output fixture");
    let result = String::from_utf8(res).unwrap();

    let mut e = expected.split('\n').collect::<Vec<_>>();
    let mut r = result.split('\n').collect::<Vec<_>>();

    e.sort();
    r.sort();

    assert_eq!(r, e, "fixture: {:?}", &name)
}
