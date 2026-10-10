use super::*;
use crate::element::Rect;
use serde_json::json;

fn el(mark: u32, role: Role, name: &str) -> Element {
    Element {
        mark,
        role,
        name: name.into(),
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        enabled: true,
        secure: false,
        value: None,
    }
}

fn form() -> Grounding {
    Grounding::Marks(vec![
        el(1, Role::Link, "Home"),
        el(2, Role::TextField, "Name"),
        el(3, Role::TextField, "Email address"),
        el(4, Role::TextField, "Message"),
        el(5, Role::Button, "Send message"),
        el(6, Role::Button, "Cancel"),
    ])
}

fn sg(v: serde_json::Value) -> Subgoal {
    serde_json::from_value(v).unwrap()
}

fn answers(v: serde_json::Value) -> BTreeMap<String, Answer> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, a)| (k.clone(), Answer::parse(a).unwrap()))
        .collect()
}

#[test]
fn candidates_fit_the_role_and_rank_by_overlap() {
    let g = form();
    let t = sg(json!({"do":"type","target":"Email field","text":"a@b.c","expect":"typed"}));
    let c = candidates(&g, &t, 12);
    assert_eq!(c.iter().map(|e| e.mark).collect::<Vec<_>>(), vec![3, 2, 4]);
    let k = sg(json!({"do":"click","target":"Send message button","expect":"sent"}));
    let c = candidates(&g, &k, 2);
    assert_eq!(c.iter().map(|e| e.mark).collect::<Vec<_>>(), vec![5, 1]);
    let grid = Grounding::Grid {
        cols: 2,
        rows: 2,
        screen: Rect::new(0.0, 0.0, 10.0, 10.0),
    };
    assert!(candidates(&grid, &k, 12).is_empty());
}

#[test]
fn one_request_carries_target_done_ready_and_previous_check() {
    let g = form();
    let prev = sg(json!({"do":"type","target":"Name","text":"Ada","expect":"Name holds Ada"}));
    let t = sg(json!({"do":"type","target":"Email","text":"a@b.c","expect":"Email holds a@b.c"}));
    let c = candidates(&g, &t, 12);
    let q = questions(&t, &c, Some(&prev));
    let ids: Vec<&str> = q.iter().map(|(i, _)| i.as_str()).collect();
    assert_eq!(ids, vec![Q_TARGET, Q_DONE, Q_READY, Q_PREV]);
    let Question::Criteria { criteria, .. } = &q[0].1 else {
        panic!("target is a choice with criteria")
    };
    assert_eq!(
        criteria[0],
        ("3".into(), "#3 textfield \"Email address\"".into())
    );
    assert_eq!(criteria.last().unwrap().0, NONE);
    let s = state("contact form", &t, &g, Some(&prev));
    assert_eq!(s["previous"]["expect"], "Name holds Ada");
    assert!(s["elements"].as_str().unwrap().contains("#5 button"));
    // Shell: no target, no already_done.
    let sh = sg(json!({"do":"shell","text":"ls","expect":"listing"}));
    let ids: Vec<String> = questions(&sh, &[], None).into_iter().map(|q| q.0).collect();
    assert_eq!(ids, vec![Q_READY]);
}

fn judge_type(a: serde_json::Value) -> Verdict {
    let g = form();
    let t = sg(json!({"do":"type","target":"Email","text":"a@b.c","expect":"typed"}));
    let c = candidates(&g, &t, 12);
    judge(&t, &answers(a), &c, &Gates::default()).0
}

#[test]
fn gates_act_only_when_confident() {
    let ok = json!({
        "target":{"type":"choice","choice":"3","probabilities":{"3":0.9,"2":0.05,"none":0.05}},
        "already_done":{"type":"noul","noul":0.1},
        "ready":{"type":"noul","noul":0.95}});
    assert_eq!(judge_type(ok), Verdict::Act(Some(3)));
    // Option given as its description.
    let desc = json!({
        "target":{"type":"choice","choice":"#3 textfield \"Email address\"","confidence":0.8},
        "ready":{"type":"noul","noul":0.9}});
    assert_eq!(judge_type(desc), Verdict::Act(Some(3)));
}

#[test]
fn gates_escalate_when_unsure_or_skip_when_done() {
    let cases = [
        // `none`
        json!({"target":{"type":"choice","choice":"none","confidence":0.9},
               "ready":{"type":"noul","noul":0.9}}),
        // low confidence
        json!({"target":{"type":"choice","choice":"3","probabilities":{"3":0.5,"2":0.1}},
               "ready":{"type":"noul","noul":0.9}}),
        // close runner-up
        json!({"target":{"type":"choice","choice":"3","probabilities":{"3":0.62,"2":0.48}},
               "ready":{"type":"noul","noul":0.9}}),
        // not ready
        json!({"target":{"type":"choice","choice":"3","confidence":0.95},
               "ready":{"type":"noul","noul":0.4}}),
        // no ready answer at all
        json!({"target":{"type":"choice","choice":"3","confidence":0.95}}),
        // a mark that was not offered (the Send button for a type subgoal)
        json!({"target":{"type":"choice","choice":"5","confidence":0.95},
               "ready":{"type":"noul","noul":0.9}}),
        // no confidence anywhere: margin unknown
        json!({"target":{"type":"choice","choice":"3"},"ready":{"type":"noul","noul":0.9}}),
    ];
    for c in cases {
        let v = judge_type(c.clone());
        assert!(
            matches!(&v, Verdict::Escalate(n) if n.contains("Executor was unsure")),
            "{c} -> {v:?}"
        );
    }
    let done = json!({"already_done":{"type":"noul","noul":0.9},
                      "ready":{"type":"noul","noul":0.1}});
    assert_eq!(judge_type(done), Verdict::Skip);
}

#[test]
fn untargeted_subgoals_need_only_ready() {
    let k = sg(json!({"do":"key","keys":["Return"],"expect":"sent"}));
    let a = answers(json!({"ready":{"type":"noul","noul":0.8}}));
    assert_eq!(judge(&k, &a, &[], &Gates::default()).0, Verdict::Act(None));
    let a = answers(json!({"ready":{"type":"noul","noul":0.6}}));
    assert!(matches!(
        judge(&k, &a, &[], &Gates::default()).0,
        Verdict::Escalate(_)
    ));
}
