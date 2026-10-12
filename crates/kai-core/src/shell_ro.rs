//! `shell_ro`: the strict read-only command allowlist for outcome probes
//! (`docs/VERIFICATION.md`). A [`ReadOnlyCmd`] can only be built by
//! [`ReadOnlyCmd::parse`], and `kai-os` only runs that type, so a command
//! that was not checked here can never reach the OS through a probe.
//!
//! Allowed: up to 3 stages joined by `|`, every stage on the allowlist —
//! `ls`, `stat`, `grep`, `cat`, `test`, `wc`, `xdotool` with window-name
//! queries only, or PowerShell `Get-*` / `Test-Path`. Refused: chaining,
//! background jobs, redirection, substitution, paths to programs, and
//! anything else. Nothing is expanded except a leading `~/`; the stages run
//! without a shell.

use serde::{Deserialize, Serialize};

/// Longest command accepted.
pub const MAX_LEN: usize = 300;
/// Most pipeline stages.
pub const MAX_STAGES: usize = 3;

/// POSIX programs a probe may run (bare names only).
const POSIX: &[&str] = &["ls", "stat", "grep", "cat", "test", "wc", "xdotool"];
/// The only words `xdotool` may get: window-name queries, never actions.
const XDOTOOL_WORDS: &[&str] = &[
    "getactivewindow",
    "getwindowfocus",
    "getwindowname",
    "%1",
    "%@",
];
/// Characters that would chain, redirect or substitute in any shell.
const FORBIDDEN: &[char] = &[';', '&', '>', '<', '`', '$', '(', ')', '{', '}', '\n', '\r'];
/// Extra PowerShell syntax: splatting/arrays, type literals, static calls.
const PS_FORBIDDEN: &[&str] = &["@", "[", "]", "::"];

/// How the stages run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flavor {
    /// Programs run directly, stdout piped to the next stage's stdin.
    Posix,
    /// One `powershell -Command` line rebuilt from checked words.
    PowerShell,
}

/// A command that passed the allowlist. No public constructor but
/// [`ReadOnlyCmd::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOnlyCmd {
    flavor: Flavor,
    stages: Vec<Vec<String>>,
    source: String,
}

impl ReadOnlyCmd {
    /// Check `cmd` against the allowlist. The error says why it was refused.
    pub fn parse(cmd: &str) -> Result<Self, String> {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return Err("empty command".into());
        }
        if cmd.chars().count() > MAX_LEN {
            return Err(format!("longer than {MAX_LEN} characters"));
        }
        if let Some(c) = cmd.chars().find(|c| FORBIDDEN.contains(c)) {
            return Err(format!(
                "`{}` is not allowed (no chaining, redirection or substitution)",
                c.escape_default()
            ));
        }
        let parts: Vec<&str> = cmd.split('|').collect();
        if parts.len() > MAX_STAGES {
            return Err(format!("more than {MAX_STAGES} stages"));
        }
        let mut stages = Vec::new();
        let mut flavor = None;
        for part in parts {
            let words = split_words(part)?;
            let Some(program) = words.first() else {
                return Err("empty pipeline stage".into());
            };
            let f = stage_flavor(program)?;
            if flavor.is_some_and(|prev| prev != f) {
                return Err("POSIX and PowerShell stages cannot be mixed".into());
            }
            flavor = Some(f);
            match f {
                Flavor::Posix if program == "xdotool" => check_xdotool(&words[1..])?,
                Flavor::Posix => {}
                Flavor::PowerShell => check_powershell(&words)?,
            }
            stages.push(words);
        }
        Ok(Self {
            flavor: flavor.unwrap_or(Flavor::Posix),
            stages,
            source: cmd.to_string(),
        })
    }

    pub fn flavor(&self) -> Flavor {
        self.flavor
    }

    /// The checked stages: program first, then its arguments.
    pub fn stages(&self) -> &[Vec<String>] {
        &self.stages
    }

    /// The command as the model wrote it (for records).
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The PowerShell line rebuilt from checked words: cmdlet names and
    /// `-Parameter` names bare, every other word single-quoted (nothing in
    /// single quotes is expanded).
    pub fn powershell_line(&self) -> String {
        self.stages
            .iter()
            .map(|words| {
                words
                    .iter()
                    .enumerate()
                    .map(|(i, w)| {
                        if i == 0 || is_ps_param(w) {
                            w.clone()
                        } else {
                            format!("'{}'", w.replace('\'', "''"))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Words of one stage. Single or double quotes group words; nothing inside
/// is interpreted. An unclosed quote is refused.
fn split_words(s: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            None => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("unclosed quote".into());
    }
    if started {
        words.push(cur);
    }
    Ok(words)
}

fn stage_flavor(program: &str) -> Result<Flavor, String> {
    if POSIX.contains(&program) {
        return Ok(Flavor::Posix);
    }
    let lower = program.to_ascii_lowercase();
    let noun = lower.strip_prefix("get-").unwrap_or("");
    if lower == "test-path" || (!noun.is_empty() && noun.chars().all(|c| c.is_ascii_alphabetic())) {
        return Ok(Flavor::PowerShell);
    }
    Err(format!(
        "`{program}` is not on the read-only allowlist (ls, stat, grep, cat, test, wc, \
         xdotool getwindowname, Get-*, Test-Path)"
    ))
}

fn check_xdotool(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("xdotool needs a window-name query".into());
    }
    for a in args {
        if !XDOTOOL_WORDS.contains(&a.as_str()) && !a.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("xdotool `{a}` is not a read-only window query"));
        }
    }
    Ok(())
}

fn check_powershell(words: &[String]) -> Result<(), String> {
    for w in words {
        if let Some(bad) = PS_FORBIDDEN.iter().find(|b| w.contains(*b)) {
            return Err(format!("`{bad}` is not allowed in PowerShell probes"));
        }
    }
    Ok(())
}

fn is_ps_param(w: &str) -> bool {
    let name = w.strip_prefix('-').unwrap_or("");
    let name = name.strip_suffix(':').unwrap_or(name);
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(c: &str) -> ReadOnlyCmd {
        ReadOnlyCmd::parse(c).unwrap_or_else(|e| panic!("{c:?} refused: {e}"))
    }
    fn refused(c: &str) {
        assert!(ReadOnlyCmd::parse(c).is_err(), "{c:?} should be refused");
    }

    #[test]
    fn read_only_commands_pass() {
        let c = ok("grep -qi 'march invoice' ~/kai-bench/server/submission.json");
        assert_eq!(c.flavor(), Flavor::Posix);
        assert_eq!(
            c.stages()[0],
            vec![
                "grep",
                "-qi",
                "march invoice",
                "~/kai-bench/server/submission.json"
            ]
        );
        ok("ls -la ~/Downloads");
        ok("stat ~/kai-note.txt");
        ok("test -f ~/kai-note.txt");
        ok("wc -l notes.txt");
        ok("cat ~/kai-note.txt | grep -q Hello");
        ok("ls ~/Pictures | wc -l");
        ok("xdotool getactivewindow getwindowname | grep -qi gemma");
        ok("xdotool getwindowname 12345");
        let ps = ok("Get-ChildItem -Path C:\\Users\\me\\Downloads | Get-Member");
        assert_eq!(ps.flavor(), Flavor::PowerShell);
        assert_eq!(
            ps.powershell_line(),
            "Get-ChildItem -Path 'C:\\Users\\me\\Downloads' | Get-Member"
        );
        ok("Test-Path ~/kai-note.txt");
        ok("Get-Content -Path \"it's here.txt\"");
        assert_eq!(
            ok("Get-Content -Path \"it's here.txt\"").powershell_line(),
            "Get-Content -Path 'it''s here.txt'"
        );
    }

    #[test]
    fn writes_and_anything_off_the_allowlist_are_refused() {
        for c in [
            // Writing programs and interpreters.
            "rm -rf ~",
            "touch x",
            "mv a b",
            "cp a b",
            "tee out.txt",
            "sed -i s/a/b/ f",
            "find . -delete",
            "sh -c 'ls'",
            "bash -c ls",
            "python3 -c 'import os'",
            "curl http://x",
            "dd if=/dev/zero of=f",
            // A path to a program, even an allowed name.
            "/bin/ls",
            "./ls",
            // Chaining, background, redirection, substitution.
            "ls; rm -rf ~",
            "ls && rm x",
            "ls || rm x",
            "ls & rm x",
            "cat a > b",
            "cat a >> b",
            "grep x < f",
            "cat $(echo f)",
            "cat `echo f`",
            "cat ${HOME}/f",
            "echo $HOME",
            "ls\nrm x",
            // A pipe into something off the list.
            "cat f | sh",
            "ls | xargs rm",
            "cat f | tee g",
            // xdotool actions.
            "xdotool key ctrl+w",
            "xdotool type hello",
            "xdotool search --name x windowkill",
            "xdotool getactivewindow windowkill",
            "xdotool",
            // PowerShell writes and tricks.
            "Remove-Item C:\\x",
            "Set-Content -Path f -Value x",
            "Get-Process | Stop-Process",
            "Get-ChildItem | ForEach-Object { Remove-Item $_ }",
            "Get-Item @args",
            "Get-Item [IO.File]::Delete('x')",
            "Invoke-Expression ls",
            "Get-",
            "Get-Item;Remove-Item x",
            // Mixed flavours, shape limits, bad quoting.
            "Get-Content f | grep x",
            "ls | ls | ls | ls",
            "",
            "   ",
            "grep 'unclosed",
        ] {
            refused(c);
        }
        refused(&format!("cat {}", "a".repeat(MAX_LEN)));
    }
}
