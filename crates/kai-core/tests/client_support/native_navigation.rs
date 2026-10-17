//! Native coordinate clicks may authorize a bounded observed navigation hop.
use super::*;
use kai_core::{ApprovalChoice, Element, Role};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct LinkPage<'a> {
    hands: &'a Hands,
    external_navigation: &'a AtomicBool,
    ambiguous: bool,
    two_hops: bool,
}

#[async_trait]
impl Perceptor for LinkPage<'_> {
    async fn observe(&self) -> anyhow::Result<Observation> {
        let mut obs = Screen.observe().await?;
        let actions = self.hands.0.lock().unwrap().len();
        let arrived = self.external_navigation.load(Ordering::SeqCst) || actions > 0;
        obs.url = Some(
            if self.two_hops {
                match actions {
                    0 => "https://example.org/",
                    1 => "https://docs.example.net/guide",
                    2 | 3 => "https://manuals.vendor.dev/",
                    _ => "https://third.example.com/",
                }
            } else if arrived {
                "https://docs.example.net/guide"
            } else {
                "https://example.org/"
            }
            .into(),
        );
        let link = Element {
            mark: 1,
            role: Role::Link,
            name: "Documentation".into(),
            bounds: Rect::new(100., 0., 100., 100.),
            enabled: true,
            focused: false,
            secure: false,
            value: None,
        };
        let mut elements = vec![link.clone()];
        if self.ambiguous {
            elements.push(Element { mark: 2, ..link });
        }
        obs.grounding = Grounding::Marks(elements);
        Ok(obs)
    }

    async fn observe_tree_only(&self) -> anyhow::Result<Observation> {
        let mut obs = self.observe().await?;
        obs.images.screenshot_jpeg.clear();
        Ok(obs)
    }
}

struct LinkApprover<'a> {
    skip_first: bool,
    external_navigation: &'a AtomicBool,
    ordinary: AtomicUsize,
    prompts: Mutex<Vec<String>>,
}

#[async_trait]
impl kai_core::Approver for LinkApprover<'_> {
    async fn approve(&self, _: &Decision, _: &Grounding) -> ApprovalChoice {
        let first = self.ordinary.fetch_add(1, Ordering::SeqCst) == 0;
        if first && self.skip_first {
            self.external_navigation.store(true, Ordering::SeqCst);
            ApprovalChoice::Skip
        } else {
            ApprovalChoice::Approve
        }
    }

    async fn approve_safety_check(&self, decision: &Decision, _: &Grounding) -> ApprovalChoice {
        self.prompts.lock().unwrap().push(decision.thought.clone());
        ApprovalChoice::Stop
    }
}

async fn link_hop_case(ambiguous: bool, skip_first: bool) {
    let (server, base) = Server::start(vec![
        Reply::Json(
            200,
            json!({"id":"link","status":"completed","output":[
            {"type":"computer_call","call_id":"link_click","actions":[
                {"type":"click","x":10,"y":20,"button":"left"}]}]}),
        ),
        Reply::Json(
            200,
            json!({"id":"follow","status":"completed","output":[
            {"type":"computer_call","call_id":"follow_move","actions":[
                {"type":"move","x":20,"y":30}]}]}),
        ),
        Reply::Json(
            200,
            json!({"id":"done","status":"completed","output":[
            {"type":"message","content":[{"type":"output_text","text":"done"}]}]}),
        ),
    ])
    .await;
    let hands = Hands::default();
    let navigation = AtomicBool::new(false);
    let page = LinkPage {
        hands: &hands,
        external_navigation: &navigation,
        ambiguous,
        two_hops: false,
    };
    let approver = LinkApprover {
        skip_first,
        external_navigation: &navigation,
        ordinary: AtomicUsize::new(0),
        prompts: Mutex::default(),
    };
    let mut r = runner(&base, Some("fixture"), &hands);
    r.config.local_safety_checks = true;
    r.config.computer_tools_v1 = true;
    r.perceptor = &page;
    let run = r
        .run_with_approver(
            "Visit example.org, then follow its documentation link",
            Some(&approver),
        )
        .await
        .unwrap();
    let actions = hands.0.lock().unwrap();
    let prompts = approver.prompts.lock().unwrap();
    if !ambiguous && !skip_first {
        assert_eq!(run.message.as_deref(), Some("done"));
        assert_eq!(
            actions.len(),
            2,
            "the linked destination should be permitted"
        );
        assert!(
            prompts.is_empty(),
            "observed navigation should not flag domain"
        );
        assert_eq!(server.bodies().len(), 3);
    } else {
        assert_eq!(run.trace.outcome, Some(Outcome::AskedUser));
        assert!(
            prompts.iter().any(|p| p.contains("irrelevant_domain")),
            "ambiguous or skipped link must not grant navigation: {prompts:?}"
        );
        assert_eq!(actions.len(), usize::from(!skip_first));
        assert_eq!(server.bodies().len(), 2);
    }
}

#[tokio::test]
async fn native_observed_link_allows_a_single_new_domain() {
    link_hop_case(false, false).await;
}

#[tokio::test]
async fn native_ambiguous_link_never_grants_a_domain_hop() {
    link_hop_case(true, false).await;
}

#[tokio::test]
async fn skipped_native_link_never_grants_a_domain_hop() {
    link_hop_case(false, true).await;
}

#[tokio::test]
async fn native_link_hops_allow_two_destinations_but_deny_the_third() {
    let response = |id: &str, action: serde_json::Value| {
        Reply::Json(
            200,
            json!({"id":id,"status":"completed","output":[
            {"type":"computer_call","call_id":id,"actions":[action]}]}),
        )
    };
    let click = json!({"type":"click","x":10,"y":20,"button":"left"});
    let move_action = json!({"type":"move","x":20,"y":30});
    let (server, base) = Server::start(vec![
        response("first", click.clone()),
        response("second", click.clone()),
        response("on_second_hop", move_action.clone()),
        response("third", click),
        response("beyond_limit", move_action),
    ])
    .await;
    let hands = Hands::default();
    let navigation = AtomicBool::new(false);
    let page = LinkPage {
        hands: &hands,
        external_navigation: &navigation,
        ambiguous: false,
        two_hops: true,
    };
    let approver = LinkApprover {
        skip_first: false,
        external_navigation: &navigation,
        ordinary: AtomicUsize::new(0),
        prompts: Mutex::default(),
    };
    let mut r = runner(&base, Some("fixture"), &hands);
    r.config.local_safety_checks = true;
    r.config.computer_tools_v1 = true;
    r.perceptor = &page;
    let run = r
        .run_with_approver("Visit example.org and follow the links", Some(&approver))
        .await
        .unwrap();
    assert_eq!(run.trace.outcome, Some(Outcome::AskedUser));
    assert_eq!(hands.0.lock().unwrap().len(), 4);
    let prompts = approver.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1, "first two hops should need no review");
    assert!(prompts[0].contains("irrelevant_domain"));
    assert_eq!(server.bodies().len(), 5);
}
