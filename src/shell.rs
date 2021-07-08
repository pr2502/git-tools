//! `git-tools/git-shell` is a simplified replacement for `git/git-shell`.
//!
//! It can only be called with `-c cmd` option and will give better error messages than the default
//! git. It however doesn't honor the `GIT_EXEC_PATH` runtime environment variable, instead it uses
//! hard-coded path provided by the `GIT_EXECUTABLE` at *compile time*.


use anyhow::{bail, ensure, Context, Result};
use std::env;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;


/// Absolute path to the `git` executable passed at compile time
const GIT_EXECUTABLE: &str = env!("GIT_EXECUTABLE");

/// List of allowed commands and their handler functions
const ALLOWED_GIT_COMMANDS: &[(&str, fn(&str) -> Result<()>)] = &[
    ("receive-pack", standard_commands),
    ("upload-pack", standard_commands),
    ("upload-archive", standard_commands),
];

/// Prepare a git `Command` and check the environment
fn git() -> Result<Command> {
    let git = Path::new(GIT_EXECUTABLE);
    ensure!(git.is_absolute(), "GIT_EXECUTABLE={:?} was not an absolute path, please recompile the binary", &git);
    ensure!(git.exists(), "GIT_EXECUTABLE={:?} does not exist", &git);
    Ok(Command::new(git))
}

/// Execute commands supported by the standard `git-shell`
fn standard_commands(cmd: &str) -> Result<()> {
    let (cmd, arg) = cmd.split_once(" ")
        .context("missing command argument")?;

    let arg = git_shell_dequote(arg)
        .with_context(|| format!("malformed command argument: {}", arg))?;

    Err(git()?.arg(cmd).arg(arg).exec())
        .context("failed to exec git")
}

/// Undo git quoting for the standard commands.
///
/// From git source code file `quote.c` function `sq_quote_buf`:
///
/// > Any single quote is replaced with '\'', any exclamation point
/// > is replaced with '\!', and the whole thing is enclosed in a
/// > single quote pair.
/// >
/// > E.g.
/// >  original     sq_quote     result
/// >  name     ==> name      ==> 'name'
/// >  a b      ==> a b       ==> 'a b'
/// >  a'b      ==> a'\''b    ==> 'a'\''b'
/// >  a!b      ==> a'\!'b    ==> 'a'\!'b'
fn git_shell_dequote(input: &str) -> Result<String> {
    let mut input = input
        .strip_prefix('\'').context("missing ' at the beginning")?
        .strip_suffix('\'').context("missing ' at the end")?
        .as_bytes();

    let mut output = Vec::with_capacity(input.len());

    loop {
        match input {
            // done
            [] => return String::from_utf8(output).context("invalid UTF-8"),

            // null inside strings is not permitted because C is bad
            [b'\0', ..] => bail!("embedded \\0 in string"),

            // replace escape sequences with just the escaped character
            [b'\'', b'\\', chr @ (b'\'' | b'!'), b'\'', rest @ ..] => {
                output.push(*chr);
                input = rest;
            }

            // `'` and `!` have to be escaped,
            // escapes got caught by the previous arm, whatever got here is malformed
            [chr @ (b'\'' | b'!'), ..] => bail!("unquoted {}", chr),

            // pass all other characters through
            [chr, rest @ ..] => {
                output.push(*chr);
                input = rest;
            }
        }
    }
}

fn main() -> Result<()> {
    let args = env::args().collect::<Vec<_>>();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();

    match args.as_slice() {
        [_, "-c", cmd] => {
            let cmd = cmd.strip_prefix("git-")
                .or_else(|| cmd.strip_prefix("git "))
                .with_context(|| format!("`{}` is not a git command", cmd))?;

            let fun = ALLOWED_GIT_COMMANDS.iter()
                .find_map(|(name, fun)| cmd.starts_with(name).then(|| fun))
                .with_context(|| format!("disallowed or unknown git subcommand `git-{}`", cmd))?;

            fun(cmd)
        }
        [_, "cvs server"] => bail!("cvs server is not supported in git-tools, use `-c cmd`"),
        [_] => bail!("interactive git shell is not supported in git-tools, use `-c cmd`"),
        _ => bail!("invalid arguments, use `-c cmd`"),
    }
}
