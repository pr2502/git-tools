//! `git-tools/git-shell` is a simplified replacement for `git/git-shell`.
//!
//! It can only be called with `-c cmd` option and will give better error messages than the default
//! git. It however doesn't honor the `GIT_EXEC_PATH` runtime environment variable, instead it uses
//! hard-coded path provided by the `GIT_EXECUTABLE` at *compile time*.


use anyhow::{bail, Context, Result};
use std::env;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;


fn git_shell_dequote(input: &str) -> Option<String> {
    let mut input = input
        .strip_prefix('\'')?
        .strip_suffix('\'')?
        .as_bytes();

    let mut output = Vec::with_capacity(input.len());

    loop {
        match input {
            // done
            [] => break String::from_utf8(output).ok(),

            // null inside quotes is not permitted because C is bad
            &[b'\0', ..] => break None,

            // escaped characters, according to the comment on function `sq_quote_buf` in git code
            //
            // > Any single quote is replaced with '\'', any exclamation point
            // > is replaced with '\!', and the whole thing is enclosed in a
            // > single quote pair.
            // >
            // > E.g.
            // >  original     sq_quote     result
            // >  name     ==> name      ==> 'name'
            // >  a b      ==> a b       ==> 'a b'
            // >  a'b      ==> a'\''b    ==> 'a'\''b'
            // >  a!b      ==> a'\!'b    ==> 'a'\!'b'
            [b'\'', b'\\', chr @ (b'\'' | b'!'), b'\'', rest @ ..] => {
                output.push(*chr);
                input = rest;
            }

            // `'` and `!` have to be escaped,
            // escapes got caught by the previous arm, whatever got here is malformed
            [b'\'' | b'!', ..] => break None,

            // pass all other characters through
            [chr, rest @ ..] => {
                output.push(*chr);
                input = rest;
            }
        }
    }
}

const ALLOWED_GIT_COMMANDS: &[&str] = &[
    "receive-pack",
    "upload-pack",
    "upload-archive",
];

const GIT_EXECUTABLE: &str = env!("GIT_EXECUTABLE");

fn main() -> Result<()> {
    let git = Path::new(GIT_EXECUTABLE);
    if !git.is_absolute() {
        bail!("GIT_EXECUTABLE={:?} was not an absolute path, please recompile the binary", &git);
    }
    if !git.exists() {
        bail!("GIT_EXECUTABLE={:?} does not exist", &git);
    }
    let mut git = Command::new(git);

    let args = env::args().collect::<Vec<_>>();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();

    match args.as_slice() {
        &[_, "-c", cmd] => {
            let cmd = cmd.strip_prefix("git-")
                .or_else(|| cmd.strip_prefix("git "))
                .context("command is not a git command")?;

            let (cmd, arg) = cmd.split_once(" ")
                .context("missing command argument")?;

            if !ALLOWED_GIT_COMMANDS.contains(&cmd) {
                bail!("disallowed or unknown git subcommand `git-{}`", cmd);
            }

            let arg = git_shell_dequote(arg)
                .context("command argument is incorrectly quoted")?;

            Err(git.arg(arg).exec())
                .context("failed to execute git")
        }
        &[_, "cvs server"] => bail!("cvs server is not supported in git-tools, use `-c cmd`"),
        &[_] => bail!("interactive git shell is not supported in git-tools, use `-c cmd`"),
        _ => bail!("invalid arguments, use `-c cmd`"),
    }
}
