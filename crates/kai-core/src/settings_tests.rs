use super::*;
use crate::client::Backend;

#[test]
fn defaults_are_safe() {
    let s = Settings::default();
    assert_eq!(s.approval, ApprovalLevel::HighRiskOnly);
    assert_eq!(s.max_steps, 40);
    assert_eq!(s.limits().1, Duration::from_secs(600));
    assert_eq!(s.fast_model, "gemma-4-26b-a4b-it");
    assert_eq!(s.executor_endpoint, "http://127.0.0.1:8017");
    assert_eq!(s.executor_client().base(), "http://127.0.0.1:8017");
    assert!(!s.use_browser, "the browser lane is opt-in");
}

#[test]
fn unknown_fields_and_missing_fields_load() {
    let s: Settings = serde_json::from_str(r#"{"max_steps": 7, "future_field": true}"#).unwrap();
    assert_eq!(s.max_steps, 7);
    assert_eq!(s.parallelism, 4);
}

#[test]
fn both_with_only_gemini_uses_gemini() {
    let s = Settings::default();
    let (fast, careful) = s
        .clients(&Keys {
            vllm: None,
            gemini: Some("k".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(fast.backend(), Backend::Gemini);
    assert!(careful.is_some());
}

#[test]
fn both_with_both_fails_over() {
    let s = Settings {
        vllm_endpoint: "https://x/v1".into(),
        ..Default::default()
    };
    let (fast, _) = s
        .clients(&Keys {
            vllm: Some("a".into()),
            gemini: Some("b".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(fast.backend(), Backend::Vllm);
}

#[test]
fn nothing_configured_is_a_clear_error() {
    let err = Settings::default().clients(&Keys::default()).err().unwrap();
    assert!(
        err.to_string()
            .contains("set up the DigitalOcean server or a Gemini key")
    );
}

#[test]
fn gateway_backend_maps_ids_and_backs_careful_with_fast() {
    let s = Settings {
        backend: BackendChoice::Gateway,
        ..Default::default()
    };
    let keys = Keys {
        gateway: Some("g".into()),
        ..Default::default()
    };
    let (fast, careful) = s.clients(&keys).unwrap();
    assert_eq!(fast.backend(), Backend::OpenAi);
    assert_eq!(careful.unwrap().backend(), Backend::OpenAi);
    let err = s.clients(&Keys::default()).err().unwrap();
    assert!(err.to_string().contains("AI_GATEWAY_API_KEY"));
    assert_eq!(gateway_model_id(MODELS[0].id), "google/gemma-4-26b-a4b-it");
    assert_eq!(gateway_model_id("openai/gpt-5-mini"), "openai/gpt-5-mini");
}

#[test]
fn openai_backend_takes_openai_ids_only() {
    let keys = Keys {
        openai: Some("o".into()),
        ..Default::default()
    };
    let s = Settings {
        backend: BackendChoice::OpenAi,
        fast_model: "openai/gpt-5-mini".into(),
        careful_model: "gpt-5".into(),
        ..Default::default()
    };
    let (fast, careful) = s.clients(&keys).unwrap();
    assert_eq!(fast.backend(), Backend::OpenAi);
    assert!(careful.is_some());
    let bad = Settings {
        backend: BackendChoice::OpenAi,
        ..Default::default()
    };
    assert!(bad.clients(&keys).is_ok()); // plain gemma id: OpenAI will 404 at call time
    let bad = Settings {
        backend: BackendChoice::OpenAi,
        fast_model: "google/gemma-4-26b-a4b-it".into(),
        ..Default::default()
    };
    assert!(bad.clients(&keys).is_err());
}

#[test]
fn both_falls_back_to_gateway_when_it_is_the_only_key() {
    let (fast, _) = Settings::default()
        .clients(&Keys {
            gateway: Some("g".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(fast.backend(), Backend::OpenAi);
}

#[test]
fn comparison_models_are_gateway_ids() {
    let cmp: Vec<_> = MODELS
        .iter()
        .filter(|m| m.role == ModelRole::Comparison)
        .collect();
    assert!(cmp.len() >= 3);
    assert!(cmp.iter().all(|m| m.id.starts_with("openai/")));
}

#[test]
fn limits_are_clamped() {
    let s = Settings {
        max_steps: 0,
        max_minutes: 100_000,
        ..Default::default()
    };
    assert_eq!(s.limits(), (1, Duration::from_secs(240 * 60)));
}

#[test]
fn experimental_switches_are_off_until_turned_on() {
    let s = Settings::default();
    let c = s.engine_config();
    assert!(!c.outcomes && !c.macro_history && !c.procedures && !c.recover && !c.executor);
    assert!(!c.memo && !c.task_checks);
    assert_eq!(c.best_of, 0);
    let s = Settings {
        verify_outcomes: true,
        macro_history: true,
        use_procedures: true,
        best_of_plans: true,
        recover: true,
        memo: true,
        task_checks: true,
        ..Settings::default()
    };
    let c = s.engine_config();
    assert!(c.outcomes && c.macro_history && c.procedures && c.recover && c.memo && c.task_checks);
    assert_eq!(c.best_of, 3);
}

#[test]
fn obsolete_launcher_setting_is_ignored_and_not_written_back() {
    let settings: Settings =
        serde_json::from_value(serde_json::json!({"launcher_hotkey":"ctrl+shift+Space"})).unwrap();
    assert!(
        serde_json::to_value(settings)
            .unwrap()
            .get("launcher_hotkey")
            .is_none()
    );
}
