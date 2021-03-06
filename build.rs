#![feature(exit_status_error)]

use std::process::Command;
use std::env;

fn main() {
    ////////////////////////////////////////
    // git-shell
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

    ////////////////////////////////////////
    // git-site
    Command::new("sassc")
        .arg("templates/style.scss")
        .arg("static/style.css")
        .spawn()
        .expect("unable to run `sassc`")
        .wait()
        .unwrap()
        .exit_ok()
        .expect("`sassc` failed");

    println!("cargo:rerun-if-changed=templates/style.scss");
}
