use std::process::Command;
use std::env;

fn main() {
    let git_executable = env::var("GIT_EXECUTABLE")
        .unwrap_or_else(|_| {
            let out = Command::new("which").arg("git")
                .output()
                .expect("unable to run `which git`");

            let stdout = String::from_utf8(out.stdout)
                .expect("stdout is invalid utf-8");
            let stdout = stdout.trim();

            if !out.status.success() {
                panic!("which: {}", stdout);
            }

            stdout.to_owned()
        });

    println!("cargo:rustc-env=GIT_EXECUTABLE={}", git_executable);
}
