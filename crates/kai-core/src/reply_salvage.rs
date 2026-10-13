//! A reply cut off by the reply budget keeps what was complete before the
//! cut, instead of costing a repair call: every top-level field that ended
//! before it and, inside `then`, every follow-up that was complete. This is
//! the rule [`crate::reply`] already applies to an invalid follow-up: it
//! drops itself and every one after it, never the main decision. The usual
//! validation still guards the result: a cut required field (`risk`), or the
//! `target` or `text` an action needs, fails to parse and is repaired as
//! before. Seen on Holo3.1 4B (2026-10-13): 10 of ~50 replies were cut, 9 of
//! them in `then`, and each repair cost a full extra model call (one to two
//! minutes on a CPU).

/// The cut reply closed after its last complete part, or `None` for a whole
/// reply, one with nothing complete to keep, or one whose `risk` was cut.
pub fn salvage_cut_then(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let bytes = raw.as_bytes();
    let (mut depth, mut in_str, mut escaped) = (0usize, false, false);
    let mut str_start = 0;
    // Index of the last comma at the top level, before the current key.
    let mut comma = None;
    // `then`'s comma (where to cut when no follow-up is complete), whether
    // the scan is inside `then`, and where its last complete item ends.
    let mut then_comma = None;
    let mut in_then = false;
    let mut last_item = None;
    // The last string closed at the top level since the last comma: the key
    // whose value comes next.
    let mut pending_key: Option<String> = None;
    for i in start..bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
                if depth == 1 {
                    pending_key = Some(raw[str_start + 1..i].to_string());
                }
            }
            continue;
        }
        match c {
            b'"' => {
                in_str = true;
                str_start = i;
            }
            b',' if depth == 1 => {
                comma = Some(i);
                pending_key = None;
            }
            b'[' if depth == 1 && pending_key.as_deref() == Some("then") => {
                then_comma = comma;
                in_then = true;
                depth += 1;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return None; // a whole object: nothing to salvage
                }
                if in_then && depth == 2 && c == b'}' {
                    last_item = Some(i + 1);
                }
                if depth == 1 {
                    in_then = false;
                }
            }
            _ => {}
        }
    }
    let fixed = match (in_then, last_item) {
        (true, Some(end)) => format!("{}]}}", &raw[start..end]),
        (true, None) => format!("{}}}", &raw[start..then_comma?]),
        (false, _) => format!("{}}}", &raw[start..comma?]),
    };
    // The model's own `risk` must have been complete: a missing one would
    // parse as "none" and could hide the "high" it was about to write.
    let v: serde_json::Value = serde_json::from_str(&fixed).ok()?;
    v.get("risk")?.as_str()?;
    Some(fixed)
}

#[cfg(test)]
mod tests {
    use super::salvage_cut_then;
    use crate::reply::StepReply;

    const MAIN: &str = r#"{"thought":"fill it","action":"type","target":16,"text":"Ravi Kumar","risk":"none","expect":"name typed""#;

    #[test]
    fn keeps_the_main_decision_and_complete_follow_ups() {
        let raw = format!(
            r#"{MAIN},"then":[{{"action":"type","target":17,"text":"ravi@example.com","risk":"none"}},{{"action":"click","target":18,"te"#
        );
        let fixed = salvage_cut_then(&raw).unwrap();
        let r = StepReply::parse_checked(&fixed).unwrap();
        assert_eq!(r.decision.target, Some(16));
        assert_eq!(r.then.len(), 1);
        assert_eq!(r.then[0].text.as_deref(), Some("ravi@example.com"));
    }

    #[test]
    fn a_cut_first_follow_up_leaves_only_the_main_decision() {
        // Seen live: a `keys` list repeating "left" until the budget ran out.
        let raw = format!(r#"{MAIN}, "then": [{{"action":"key","keys":["left","left","left","le"#);
        let r = StepReply::parse_checked(&salvage_cut_then(&raw).unwrap()).unwrap();
        assert_eq!(r.decision.text.as_deref(), Some("Ravi Kumar"));
        assert!(r.then.is_empty());
    }

    #[test]
    fn braces_and_brackets_inside_strings_do_not_confuse_it() {
        let raw = r#"{"thought":"a } ] \" then [","action":"shell","text":"echo '{[}]'","risk":"low","then":[{"action":"key","keys":["enter"],"risk":"none"},{"action":"sh"#;
        let r = StepReply::parse_checked(&salvage_cut_then(raw).unwrap()).unwrap();
        assert_eq!(r.decision.text.as_deref(), Some("echo '{[}]'"));
        assert_eq!(r.then.len(), 1);
    }

    #[test]
    fn a_cut_optional_field_at_the_end_is_dropped() {
        // Seen live: cut inside `expect`, and cut just after `then` closed.
        let raw = r#"{"thought":"save","action":"key","keys":["ctrl","s"],"risk":"none","expect":"the save dia"#;
        let r = StepReply::parse_checked(&salvage_cut_then(raw).unwrap()).unwrap();
        assert_eq!(r.decision.keys, ["ctrl", "s"]);
        let raw = format!(r#"{MAIN},"then":[{{"action":"key","keys":["enter"],"risk":"none"}}], "#);
        let r = StepReply::parse_checked(&salvage_cut_then(&raw).unwrap()).unwrap();
        assert_eq!(r.then.len(), 1);
    }

    /// Guard: what an action needs is never guessed. A cut `text`, `target`
    /// or `risk` leaves a reply that fails validation, so it is repaired.
    #[test]
    fn a_cut_needed_field_is_still_repaired() {
        for raw in [
            r#"{"thought":"run","action":"shell","risk":"low","text":"rm -rf ~/kai-bench/ol"#,
            r#"{"thought":"x","action":"click","risk":"none","target":1"#,
            r#"{"thought":"x","action":"click","target":14,"ri"#,
            r#"{"thought":"then","action":"wait","keys":["a","#,
        ] {
            let fixed = salvage_cut_then(raw);
            assert!(
                fixed
                    .as_deref()
                    .is_none_or(|f| StepReply::parse_checked(f).is_err()),
                "{raw} -> {fixed:?}"
            );
        }
    }

    #[test]
    fn whole_replies_and_nothing_complete_are_left_alone() {
        assert_eq!(salvage_cut_then(&format!(r#"{MAIN},"then":[]}}"#)), None);
        assert_eq!(
            salvage_cut_then(r#"{"thought":"cut in the main decis"#),
            None
        );
        assert_eq!(salvage_cut_then("no json at all"), None);
    }
}
