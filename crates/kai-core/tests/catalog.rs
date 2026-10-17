//! Provider/model pairing, catalog origins and the subscription guard.

use kai_core::catalog::{self, ModelOrigin, RemoteModel};
use kai_core::connections::codex_model;
use kai_core::settings::{BackendChoice, Keys, Settings};
use serde_json::json;

#[test]
fn claude_code_refuses_models_of_other_providers() {
    for id in ["gemma-4-26b-a4b-it", "gpt-5.6-luna", "gemini-2.5-pro", ""] {
        assert!(catalog::check(BackendChoice::Claude, id).is_err(), "{id}");
    }
    for id in ["sonnet", "opus", "haiku", "claude-sonnet-4-5"] {
        assert!(catalog::check(BackendChoice::Claude, id).is_ok(), "{id}");
    }
}

#[test]
fn codex_and_openai_refuse_foreign_families() {
    for id in [
        "gemma-4-31b-it",
        "sonnet",
        "claude-opus-4",
        "google/gemma-4-31b-it",
    ] {
        assert!(catalog::check(BackendChoice::Codex, id).is_err(), "{id}");
        assert!(catalog::check(BackendChoice::OpenAi, id).is_err(), "{id}");
    }
    assert!(catalog::check(BackendChoice::Codex, "gpt-5.6-luna").is_ok());
    assert!(catalog::check(BackendChoice::OpenAi, "openai/gpt-5-mini").is_ok());
    assert!(catalog::check(BackendChoice::Gemini, "gpt-5").is_err());
}

#[test]
fn ids_with_spaces_are_refused_everywhere() {
    for b in [
        BackendChoice::Vllm,
        BackendChoice::Gateway,
        BackendChoice::Codex,
    ] {
        assert!(catalog::check(b, "gpt 5").is_err());
        assert!(catalog::check(b, " gpt-5").is_err());
    }
}

#[test]
fn every_backend_default_fits_it_or_is_empty_for_codex() {
    for b in [
        BackendChoice::Both,
        BackendChoice::Vllm,
        BackendChoice::Gemini,
        BackendChoice::Gateway,
        BackendChoice::OpenAi,
        BackendChoice::Claude,
    ] {
        let id = catalog::default_model(b);
        assert!(catalog::check(b, id).is_ok(), "{b:?} default {id}");
        assert!(
            catalog::documented(b)
                .iter()
                .all(|m| catalog::check(b, &m.id).is_ok())
        );
    }
    assert_eq!(catalog::default_model(BackendChoice::Codex), "");
    assert!(catalog::documented(BackendChoice::Codex).is_empty());
}

#[test]
fn documented_entries_say_so_and_keep_exact_ids() {
    let claude = catalog::documented(BackendChoice::Claude);
    assert_eq!(
        claude.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["sonnet", "opus", "haiku"]
    );
    assert!(claude.iter().all(|m| m.origin == ModelOrigin::Documented));
    assert!(claude[0].describe().contains("documented alias"));
}

#[test]
fn hand_typed_selection_stays_visible_but_only_when_it_fits() {
    let listed = vec![RemoteModel::documented("sonnet", "Claude sonnet")];
    let with = catalog::choices(BackendChoice::Claude, &listed, "claude-opus-4-1");
    assert_eq!(with.len(), 2);
    assert_eq!(with[1].id, "claude-opus-4-1");
    assert_eq!(with[1].origin, ModelOrigin::Manual);
    let foreign = catalog::choices(BackendChoice::Claude, &listed, "gemma-4-26b-a4b-it");
    assert_eq!(foreign, listed);
    assert!(catalog::choices(BackendChoice::Codex, &[], "").is_empty());
}

#[test]
fn codex_catalog_entry_keeps_slug_and_advertised_modalities() {
    let m = codex_model(
        &json!({"id":"preset-1","model":"gpt-5.6-luna","displayName":"Luna",
        "hidden":false,"inputModalities":["text","image"]}),
    )
    .unwrap();
    assert_eq!(m.id, "gpt-5.6-luna");
    assert_eq!(m.origin, ModelOrigin::Discovered);
    assert_eq!(m.image_input, Some(true));
    assert!(m.describe().contains("advertises image input"));
    let text = codex_model(&json!({"model":"gpt-x","inputModalities":["text"]})).unwrap();
    assert_eq!(text.image_input, Some(false));
    assert!(codex_model(&json!({"model":"secret","hidden":true})).is_none());
}

#[test]
fn subscription_clients_refuse_a_foreign_model() {
    let mut s = Settings {
        backend: BackendChoice::Claude,
        ..Settings::default()
    };
    let err = s
        .client(&Keys::default(), "gemma-4-26b-a4b-it")
        .err()
        .expect("Claude Code must not get a Gemma model");
    assert!(err.to_string().contains("Claude models only"));
    s.backend = BackendChoice::Codex;
    assert!(s.client(&Keys::default(), "").is_err());
    assert!(s.client(&Keys::default(), "sonnet").is_err());
}

#[test]
fn settings_check_covers_both_slots() {
    let mut s = Settings {
        backend: BackendChoice::Claude,
        fast_model: "sonnet".into(),
        careful_model: "gemma-4-31b-it".into(),
        ..Settings::default()
    };
    assert!(
        catalog::check_settings(&s)
            .unwrap_err()
            .starts_with("Careful model")
    );
    s.careful_model = "opus".into();
    assert!(catalog::check_settings(&s).is_ok());
}
