//! Native `<select>` dropdowns. Their popup is drawn by the browser, outside
//! the page, so neither the screenshot nor the page's tree shows it: a model
//! that clicks a dropdown and looks for its options finds none (measured
//! live: Gemma then clicked the form's Send button instead of "Billing").
//! The lane therefore opens the popup itself: clicking a dropdown lists its
//! options as numbered marks just under it, and clicking one chooses it.
//! Typing an option's text into the dropdown also chooses it.

use super::lane::BrowserLane;
use super::snapshot::DomRef;
use kai_core::{Element, Rect, Role};
use serde_json::{Value, json};

/// Most options listed at once.
pub const MAX_OPTIONS: usize = 30;
/// Height of one listed option, in CSS pixels.
const ROW: f32 = 22.0;

/// Picks a `<select>` option by index (when its label still matches) or by
/// label (exact, then prefix, then contains; case-insensitive) or value, and
/// fires `input` and `change` as a person's choice would. Runs on the element
/// itself; the label is an argument, never pasted into the code.
const CHOOSE: &str = r#"function (label, index) {
  const want = String(label).trim().toLowerCase();
  const opts = Array.from(this.options || []);
  const text = o => (o.label || o.text || '').trim();
  const at = index >= 0 ? opts[index] : undefined;
  const o = (at && text(at).toLowerCase() === want ? at : undefined)
    || opts.find(o => text(o).toLowerCase() === want || o.value.toLowerCase() === want)
    || opts.find(o => text(o).toLowerCase().startsWith(want))
    || opts.find(o => text(o).toLowerCase().includes(want));
  if (!o) return { ok: false, options: opts.map(text).slice(0, 30) };
  if (o.disabled) return { ok: false, disabled: text(o) };
  this.focus();
  this.value = o.value;
  o.selected = true;
  this.dispatchEvent(new Event('input', { bubbles: true }));
  this.dispatchEvent(new Event('change', { bubbles: true }));
  return { ok: true, chosen: text(o) };
}"#;

/// The dropdown's options and its box in viewport CSS pixels.
const LIST: &str = r#"function () {
  const r = this.getBoundingClientRect();
  return {
    rect: [r.x, r.y, r.width, r.height],
    options: Array.from(this.options || []).slice(0, 30).map(o => ({
      label: (o.label || o.text || '').trim(), disabled: !!o.disabled, selected: !!o.selected,
    })),
  };
}"#;

/// One option of an open dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct Opt {
    pub label: String,
    pub disabled: bool,
    pub selected: bool,
}

/// The listed options as marks numbered from `first`, stacked under the
/// dropdown's box like the browser's own popup.
pub fn option_marks(options: &[Opt], under: Rect, first: u32) -> Vec<Element> {
    options
        .iter()
        .take(MAX_OPTIONS)
        .enumerate()
        .map(|(i, o)| Element {
            mark: first + i as u32,
            role: Role::ListItem,
            name: if o.selected {
                format!("{} (selected option)", o.label)
            } else {
                format!("{} (option)", o.label)
            },
            bounds: Rect::new(
                under.x,
                under.y + under.h + ROW * i as f32,
                under.w.max(120.0),
                ROW - 2.0,
            ),
            enabled: !o.disabled,
            secure: false,
            focused: false,
            value: None,
        })
        .collect()
}

/// Parse the `LIST` result.
pub fn parse_list(v: &Value) -> Option<(Rect, Vec<Opt>)> {
    let r = v["rect"].as_array()?;
    let f = |i: usize| r.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let rect = Rect::new(f(0), f(1), f(2), f(3));
    let options = v["options"]
        .as_array()?
        .iter()
        .map(|o| Opt {
            label: o["label"].as_str().unwrap_or("").to_string(),
            disabled: o["disabled"] == true,
            selected: o["selected"] == true,
        })
        .collect();
    Some((rect, options))
}

impl BrowserLane {
    async fn call_on(&self, r: DomRef, function: &str, args: Value) -> anyhow::Result<Value> {
        let node = self
            .page("DOM.resolveNode", json!({"backendNodeId": r.id}))
            .await?;
        let object = node["object"]["objectId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("the dropdown is no longer on the page"))?;
        let res = self
            .page(
                "Runtime.callFunctionOn",
                json!({
                    "objectId": object,
                    "functionDeclaration": function,
                    "arguments": args,
                    "returnByValue": true,
                }),
            )
            .await?;
        Ok(res["result"]["value"].clone())
    }

    /// The open dropdown's box and options, if one is open and still there.
    pub(super) async fn open_list(&self) -> Option<(DomRef, Rect, Vec<Opt>)> {
        let r = (*self.open_select.lock().unwrap_or_else(|e| e.into_inner()))?;
        match self
            .call_on(r, LIST, json!([]))
            .await
            .ok()
            .as_ref()
            .and_then(parse_list)
        {
            Some((rect, opts)) => Some((r, rect, opts)),
            None => {
                self.close_dropdown();
                None
            }
        }
    }

    pub(super) fn open_dropdown(&self, r: DomRef) {
        *self.open_select.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    }

    pub(super) fn close_dropdown(&self) {
        *self.open_select.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Choose an option by its label (and, from a listed mark, its index).
    pub(super) async fn choose(
        &self,
        r: DomRef,
        label: &str,
        index: Option<usize>,
    ) -> anyhow::Result<()> {
        self.close_dropdown();
        let i = index.map_or(-1, |i| i as i64);
        let v = self
            .call_on(r, CHOOSE, json!([{"value": label}, {"value": i}]))
            .await?;
        if v["ok"] == true {
            return Ok(());
        }
        if let Some(d) = v["disabled"].as_str() {
            anyhow::bail!("the option \"{d}\" is disabled");
        }
        let options: Vec<&str> = v["options"]
            .as_array()
            .map(|o| o.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        anyhow::bail!(
            "no option \"{label}\" in this dropdown; its options are: {}",
            options.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Vec<Opt> {
        ["General", "Billing", "Support"]
            .iter()
            .enumerate()
            .map(|(i, l)| Opt {
                label: l.to_string(),
                disabled: i == 2,
                selected: i == 0,
            })
            .collect()
    }

    #[test]
    fn options_are_listed_under_the_dropdown_numbered_after_the_page() {
        let m = option_marks(&opts(), Rect::new(100.0, 200.0, 80.0, 20.0), 7);
        assert_eq!(m.len(), 3);
        assert_eq!((m[0].mark, m[1].mark, m[2].mark), (7, 8, 9));
        assert_eq!(m[0].name, "General (selected option)");
        assert_eq!(m[1].name, "Billing (option)");
        assert!(!m[2].enabled, "disabled options stay disabled");
        assert_eq!(m[0].bounds.y, 220.0);
        assert!(m[1].bounds.y > m[0].bounds.y);
        assert_eq!(m[0].bounds.w, 120.0, "never narrower than a readable row");
    }

    #[test]
    fn long_lists_are_capped() {
        let many: Vec<Opt> = (0..100)
            .map(|i| Opt {
                label: format!("o{i}"),
                disabled: false,
                selected: false,
            })
            .collect();
        assert_eq!(
            option_marks(&many, Rect::new(0.0, 0.0, 1.0, 1.0), 1).len(),
            MAX_OPTIONS
        );
    }

    #[test]
    fn list_results_parse_and_garbage_does_not() {
        let v = json!({"rect": [1, 2, 3, 4], "options": [{"label": "A", "disabled": false, "selected": true}]});
        let (r, o) = parse_list(&v).unwrap();
        assert_eq!(r, Rect::new(1.0, 2.0, 3.0, 4.0));
        assert_eq!(
            o,
            vec![Opt {
                label: "A".into(),
                disabled: false,
                selected: true
            }]
        );
        assert!(parse_list(&json!(null)).is_none());
    }
}
