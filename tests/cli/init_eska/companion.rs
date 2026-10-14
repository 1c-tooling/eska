//! A standalone wrapper intervenes only inside its caller's unique playground after real child init.

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{self, Command},
};

/// Delegate probes and source operations unchanged, then make post-publication races reproducible.
fn main() {
    let arguments: Vec<_> = env::args().skip(1).collect();
    let output = Command::new(env::var_os("INIT_FIXTURE_REAL_PROJECT").unwrap())
        .args(&arguments)
        .output()
        .unwrap();
    if let Some(index) = arguments.iter().position(|value| value == "init") {
        let root = PathBuf::from(&arguments[index + 1]);
        assert!(output.status.success(), "{output:?}");
        let source = arguments
            .iter()
            .position(|value| value == "--source")
            .map_or_else(
                || PathBuf::from("src"),
                |index| PathBuf::from(&arguments[index + 1]),
            );
        match env::var("INIT_FIXTURE_MODE").unwrap().as_str() {
            "manifest" => append(&root.join("eska.toml"), "\n# intervening manifest\n"),
            "source" => append(
                &root.join(source).join("configuration.yaml"),
                "\n# intervening source\n",
            ),
            "accepted" => append(&root.join(".eska/accepted/accepted.json"), "invalid"),
            "state" => fs::write(root.join(".eska/source-import/state.json"), "invalid").unwrap(),
            "git" => {
                fs::create_dir(root.join(".git")).unwrap();
                fs::write(root.join(".git/user-marker"), "raced Git").unwrap();
            }
            "workspace" => append(
                &PathBuf::from(env::var_os("INIT_FIXTURE_ROOT").unwrap()).join("eska.toml"),
                "\n# intervening root\n",
            ),
            "malformed" => {
                println!("malformed receipt");
                return;
            }
            "failed" => {
                io::stdout().write_all(&output.stdout).unwrap();
                process::exit(23);
            }
            _ => {}
        }
        fs::write(root.join("intervening-user.txt"), "retained user bytes").unwrap();
    }
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
    process::exit(output.status.code().unwrap_or(1));
}

/// Edit bytes without inventing a new candidate hash or confirmation reference.
fn append(path: &Path, suffix: &str) {
    let mut contents = fs::read_to_string(path).unwrap();
    contents.push_str(suffix);
    fs::write(path, contents).unwrap();
}
