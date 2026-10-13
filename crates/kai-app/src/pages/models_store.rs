//! Models › More from Hugging Face: search the Hub for GGUF vision models
//! and pull one into your catalog (`crate::store`, `docs/MODEL_STORE.md`).

use crate::app::KaiApp;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    feedback::InlineMessage,
    forms::SearchInput,
    layout::{Card, CardHeader},
    motion::Spinner,
    primitives::{IconName, Severity},
    theme::{ActiveTheme, ControlSize, TextSize},
};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div};

/// "12,345" for a download count.
pub fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn hit_row(repo: &str, downloads: u64, busy: bool, ix: usize, cx: &Context<KaiApp>) -> AnyElement {
    let theme = cx.theme();
    let me = cx.entity();
    let pull = repo.to_string();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .py_1()
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(div().truncate().child(repo.to_string()))
                .child(
                    div()
                        .text_size(theme.text_size(TextSize::Sm))
                        .text_color(theme.colors.fg_muted)
                        .child(format!("{} downloads", count(downloads))),
                ),
        )
        .child(
            Button::new(("store-pull", ix), "Pull")
                .variant(ButtonVariant::Outline)
                .size(ControlSize::Sm)
                .icon(IconName::Download)
                .disabled(busy)
                .on_click(move |_, _, cx| me.update(cx, |a, cx| a.store_pull(pull.clone(), cx))),
        )
        .into_any_element()
}

pub fn card(app: &KaiApp, cx: &Context<KaiApp>) -> AnyElement {
    let s = &app.pages.store;
    let busy = s.busy.is_some();
    let me = cx.entity();
    let search = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .child(SearchInput::new("store-search", &app.pages.store_query)),
        )
        .child(
            Button::new("store-go", "Search")
                .variant(ButtonVariant::Primary)
                .size(ControlSize::Sm)
                .disabled(busy)
                .on_click(move |_, _, cx| me.update(cx, |a, cx| a.store_search(cx))),
        );
    let status = s.busy.as_ref().map(|what| {
        let text = if what == "search" {
            "Searching Hugging Face…".to_string()
        } else {
            format!("Pulling {what}: reading its files and checksums…")
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(Spinner::new("store-spin"))
            .child(text)
    });
    let message = s.message.as_ref().map(|(ok, m)| {
        InlineMessage::new(
            if *ok {
                Severity::Success
            } else {
                Severity::Warning
            },
            m.clone(),
        )
    });
    let rows = s
        .hits
        .iter()
        .enumerate()
        .map(|(ix, h)| hit_row(&h.repo, h.downloads, busy, ix, cx));
    Card::new()
        .header(CardHeader::new("More from Hugging Face").description(
            "Any GGUF model that can see the screen. Pulling pins it to the repo's current \
             version and checks every file's hash on download. Pulled models are untested \
             with Kai until a benchmark says otherwise.",
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(search)
                .children(status)
                .children(message)
                .child(div().flex().flex_col().children(rows)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    fn counts_read_with_separators() {
        assert_eq!(super::count(0), "0");
        assert_eq!(super::count(999), "999");
        assert_eq!(super::count(6029), "6,029");
        assert_eq!(super::count(1229091), "1,229,091");
    }
}
