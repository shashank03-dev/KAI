//! Task variants for training data: a task may declare
//! `vars: {name: [Ravi Kumar, Asha Rao]}` and use `{name}` in its title,
//! prompt, setup, check, teardown and recorded replies. The bench always runs
//! the first value of each var (the default rendering, unchanged from before
//! vars existed). `kai bench variants` writes the other combinations to
//! `bench/variants.yaml` — for recording training runs, never for the
//! head-to-head numbers.

use super::tasks::{PerOs, Task, load_tasks, parse_raw, validate};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

/// Characters a var value may not contain: they would break the shell
/// quoting in setup/check commands.
const FORBIDDEN: &[char] = &['\'', '"', '\\', '$', '`', '{', '}', '\n'];

/// Var names are identifiers; values are non-empty and shell-safe.
pub fn check_vars(t: &Task) -> anyhow::Result<()> {
    for (k, vals) in &t.vars {
        anyhow::ensure!(
            !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "task {}: var name {k:?} must be letters, digits or _",
            t.id
        );
        anyhow::ensure!(!vals.is_empty(), "task {}: var {k} has no values", t.id);
        for v in vals {
            anyhow::ensure!(
                !v.trim().is_empty() && !v.contains(FORBIDDEN),
                "task {}: var {k} value {v:?} is empty or has one of {FORBIDDEN:?}",
                t.id
            );
        }
        let ph = format!("{{{k}}}");
        let used = t.prompt.contains(&ph)
            || t.title.contains(&ph)
            || t.mock.iter().any(|m| m.contains(&ph))
            || [&t.setup, &t.check, &t.teardown]
                .iter()
                .any(|p| per_os_has(p, &ph));
        anyhow::ensure!(used, "task {}: var {k} is never used as {ph}", t.id);
    }
    Ok(())
}

fn per_os_has(p: &PerOs, ph: &str) -> bool {
    [&p.linux, &p.windows]
        .iter()
        .any(|c| c.as_deref().is_some_and(|c| c.contains(ph)))
}

fn subst(s: &str, vals: &BTreeMap<String, String>, json: bool) -> String {
    let mut out = s.to_string();
    for (k, v) in vals {
        let v = if json {
            // Inside a JSON string literal.
            let q = serde_json::to_string(v).unwrap_or_default();
            q[1..q.len() - 1].to_string()
        } else {
            v.clone()
        };
        out = out.replace(&format!("{{{k}}}"), &v);
    }
    out
}

fn subst_os(p: &PerOs, vals: &BTreeMap<String, String>) -> PerOs {
    PerOs {
        linux: p.linux.as_deref().map(|c| subst(c, vals, false)),
        windows: p.windows.as_deref().map(|c| subst(c, vals, false)),
        macos: p.macos.as_deref().map(|c| subst(c, vals, false)),
    }
}

/// The task with `vals` filled in. `vars` is kept so variants can still be
/// generated from a rendered list.
pub fn render(t: &Task, vals: &BTreeMap<String, String>) -> Task {
    let mut r = t.clone();
    r.title = subst(&t.title, vals, false);
    r.prompt = subst(&t.prompt, vals, false);
    r.setup = subst_os(&t.setup, vals);
    r.check = subst_os(&t.check, vals);
    r.teardown = subst_os(&t.teardown, vals);
    r.mock = t.mock.iter().map(|m| subst(m, vals, true)).collect();
    r
}

/// The task with every var at its first value.
pub fn render_default(t: &Task) -> Task {
    if t.vars.is_empty() {
        return t.clone();
    }
    render(t, &combo(t, 0))
}

/// Number of value combinations of a task (1 without vars).
pub fn combos(t: &Task) -> usize {
    t.vars.values().map(Vec::len).product::<usize>().max(1)
}

/// Combination `i` (mixed radix over the vars in name order; 0 = defaults).
pub fn combo(t: &Task, mut i: usize) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (k, vals) in &t.vars {
        out.insert(k.clone(), vals[i % vals.len()].clone());
        i /= vals.len();
    }
    out
}

/// Every combination, defaults first, then the "diagonal" (the k-th value of
/// every var together, so paired values such as a name and its email stay
/// paired), then the rest in mixed-radix order.
pub fn ordered_combos(t: &Task) -> Vec<BTreeMap<String, String>> {
    let longest = t.vars.values().map(Vec::len).max().unwrap_or(1);
    let mut out: Vec<BTreeMap<String, String>> = (0..longest)
        .map(|k| {
            t.vars
                .iter()
                .map(|(name, vals)| (name.clone(), vals[k % vals.len()].clone()))
                .collect()
        })
        .collect();
    for i in 0..combos(t) {
        let c = combo(t, i);
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Up to `n` non-default variants of every task with vars, ids `<id>--v<k>`.
pub fn variants(tasks: &[Task], n: usize) -> Vec<Task> {
    let mut out = Vec::new();
    for t in tasks.iter().filter(|t| !t.vars.is_empty()) {
        let all = ordered_combos(t);
        for (i, vals) in all.iter().enumerate().skip(1).take(n) {
            let mut v = render(t, vals);
            v.id = format!("{}--v{i}", t.id);
            v.variant_of = Some(t.id.clone());
            v.vars.clear();
            out.push(v);
        }
    }
    out
}

const HEADER: &str = "\
# Kai bench task VARIANTS, generated by `kai bench variants` from the tasks
# in tasks.yaml that declare `vars:`. Do not edit by hand; regenerate.
#
# These are for recording TRAINING DATA only (`kai bench --tasks
# bench/variants.yaml`, recorded under bench/runs/). They are NEVER part of the
# head-to-head numbers: those come from tasks.yaml with each var at its first
# (default) value, so results stay comparable over time. A bench run over
# variants writes its results to <out>/variants/, which `kai bench report`
# does not read.
#
# Each variant keeps the original setup/check/teardown with the placeholders
# filled in; `variant_of` names the task it came from.

";

/// `kai bench variants`.
pub fn write(tasks: &Path, out: &Path, n: usize) -> anyhow::Result<ExitCode> {
    let raw = parse_raw(&std::fs::read_to_string(tasks)?)?;
    // The defaults must still validate as the real task list does.
    load_tasks(tasks)?;
    let vs = variants(&raw, n);
    anyhow::ensure!(
        !vs.is_empty(),
        "no task in {} declares vars",
        tasks.display()
    );
    validate(&vs)?;
    let yaml = serde_yaml::to_string(&vs)?;
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, format!("{HEADER}{yaml}"))?;
    let from = raw.iter().filter(|t| !t.vars.is_empty()).count();
    eprintln!(
        "wrote {} variants of {from} tasks to {}",
        vs.len(),
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::super::tasks::parse_tasks;
    use super::*;

    const T: &str = r#"
- id: note
  title: Note for {name}
  prompt: Write "Call {name} about {topic}" into note.txt.
  vars:
    name: [Ravi Kumar, Asha Rao, Li Wei]
    topic: [billing, technical]
  check:
    linux: grep -q 'Call {name} about {topic}' note.txt
  mock:
    - '{"thought":"x","action":"shell","text":"echo Call {name} about {topic} > note.txt","risk":"low"}'
- id: plain
  title: Plain
  prompt: do it
  mock:
    - '{"thought":"x","action":"done","risk":"none"}'
"#;

    #[test]
    fn default_rendering_uses_first_values() {
        let t = parse_tasks(T).unwrap();
        assert_eq!(
            t[0].prompt,
            r#"Write "Call Ravi Kumar about billing" into note.txt."#
        );
        assert_eq!(t[0].title, "Note for Ravi Kumar");
        assert_eq!(
            t[0].check.linux.as_deref(),
            Some("grep -q 'Call Ravi Kumar about billing' note.txt")
        );
        assert!(t[0].mock[0].contains("echo Call Ravi Kumar about billing"));
        assert_eq!(t[1].prompt, "do it");
    }

    #[test]
    fn variants_cover_other_combinations_and_round_trip() {
        let raw = parse_raw(T).unwrap();
        assert_eq!(combos(&raw[0]), 6);
        let vs = variants(&raw, 10);
        assert_eq!(vs.len(), 5, "all non-default combinations");
        assert!(vs.iter().all(|v| v.variant_of.as_deref() == Some("note")));
        assert_eq!(vs[0].id, "note--v1");
        // The diagonal comes first: every var moves to its next value.
        assert!(
            vs[0].prompt.contains("Asha Rao about technical"),
            "{}",
            vs[0].prompt
        );
        assert!(
            vs[1].prompt.contains("Li Wei about billing"),
            "{}",
            vs[1].prompt
        );
        assert!(vs.iter().all(|v| !v.prompt.contains('{')));
        assert!(
            !vs.iter()
                .any(|v| v.prompt.contains("Ravi Kumar about billing"))
        );
        assert_eq!(variants(&raw, 2).len(), 2);
        // The written file parses back as a normal task list.
        let yaml = serde_yaml::to_string(&vs).unwrap();
        let back = parse_tasks(&yaml).unwrap();
        assert_eq!(back.len(), 5);
        assert_eq!(back[0].prompt, vs[0].prompt);
    }

    #[test]
    fn bad_vars_are_rejected() {
        let unused = T.replace(
            "topic: [billing, technical]",
            "topic: [billing, technical]\n    other: [a]",
        );
        assert!(
            parse_raw(&unused)
                .unwrap_err()
                .to_string()
                .contains("never used")
        );
        let quote = T.replace("Li Wei", "O'Brien");
        assert!(parse_raw(&quote).is_err());
    }

    #[test]
    fn shipped_vars_render_cleanly_and_make_variants() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/tasks.yaml");
        let raw = parse_raw(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let with_vars: Vec<&Task> = raw.iter().filter(|t| !t.vars.is_empty()).collect();
        assert!(with_vars.len() >= 5, "{} tasks with vars", with_vars.len());
        let rendered = load_tasks(&path).unwrap();
        for t in &with_vars {
            let r = rendered.iter().find(|r| r.id == t.id).unwrap();
            for k in t.vars.keys() {
                let ph = format!("{{{k}}}");
                assert!(!r.prompt.contains(&ph) && !r.mock.iter().any(|m| m.contains(&ph)));
            }
        }
        // Default rendering is the text from before vars existed.
        let note = rendered.iter().find(|t| t.id == "write-note").unwrap();
        assert_eq!(
            note.prompt,
            "Create a file called kai-note.txt in my home folder that says \"Hello from Kai\"."
        );
        let vs = variants(&raw, 2);
        assert_eq!(vs.len(), with_vars.len() * 2);
        validate(&vs).unwrap();
    }
}
