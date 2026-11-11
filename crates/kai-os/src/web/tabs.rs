//! Tabs between steps: a tab the working page opened becomes the working
//! tab, and a closed working tab hands back to the tab that opened it, as
//! a browser does for a person.

use super::lane::{BrowserLane, Marked, PageRef, attach, new_tab};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;

impl BrowserLane {
    /// Follow what happened between steps: a tab this page opened
    /// (`target=_blank`, `window.open`) becomes the working tab; a closed
    /// working tab hands back ([`hand_back`]); a crashed page is reloaded; a
    /// file picker is remembered for the action.
    pub(super) async fn sync_tabs(&self) -> anyhow::Result<()> {
        let now = self.current();
        let (mut opened, mut closed, mut crashed) = (None, false, false);
        for (method, p, session) in self.cdp.drain_events() {
            let mine = session.as_deref() == Some(now.session.as_str());
            match method.as_str() {
                "Target.targetCreated"
                    if p["targetInfo"]["type"] == "page"
                        && p["targetInfo"]["openerId"] == now.target.as_str() =>
                {
                    opened = p["targetInfo"]["targetId"].as_str().map(str::to_string);
                }
                "Target.targetDestroyed" if p["targetId"] == now.target.as_str() => closed = true,
                "Inspector.targetCrashed" if mine => crashed = true,
                "Page.fileChooserOpened" if mine => self.file_chooser.store(true, Ordering::SeqCst),
                "Browser.downloadWillBegin" => {
                    let name = p["suggestedFilename"].as_str().unwrap_or("a file");
                    anyhow::bail!(
                        "the page tried to download {name:?}; Kai blocked the download. Ask the user to save it manually"
                    );
                }
                _ => {}
            }
        }
        let next = match (opened, closed) {
            (Some(t), _) => Some((t, Some(now.target.clone()))),
            (None, true) => Some((self.successor(&now).await?, None)),
            _ => None,
        };
        if let Some((target, opener)) = next {
            self.switch_to(target, opener).await?;
        } else if crashed {
            log::warn!("browser lane: the page crashed; reloading it");
            let _ = self.page("Page.reload", json!({})).await;
            self.wait_loaded(Duration::from_secs(10)).await;
        }
        Ok(())
    }

    /// The tab to work in after `closed` closed, or a new one.
    async fn successor(&self, closed: &PageRef) -> anyhow::Result<String> {
        let t = self.cdp.call("Target.getTargets", json!({}), None).await?;
        let infos = t["targetInfos"].as_array().map_or(&[][..], Vec::as_slice);
        match hand_back(infos, &closed.target, closed.opener.as_deref()) {
            Some(t) => Ok(t),
            None => new_tab(&self.cdp).await,
        }
    }

    async fn switch_to(&self, target: String, opener: Option<String>) -> anyhow::Result<()> {
        let session = attach(&self.cdp, &target, self.headless, self.size).await?;
        let _ = self
            .cdp
            .call("Target.activateTarget", json!({"targetId": target}), None)
            .await;
        *self.page.lock().unwrap_or_else(|e| e.into_inner()) = PageRef {
            target,
            session,
            opener,
        };
        *self.marked.lock().unwrap_or_else(|e| e.into_inner()) = Marked::default();
        *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.close_dropdown();
        log::info!("browser lane: now working in another tab");
        Ok(())
    }
}

/// Which listed page takes over from the closed tab: the tab that opened it
/// while it is still open, else the newest web page. Never the closed tab
/// (right after `window.close()` it can still be listed), and never one of
/// the browser's own pages (`chrome://`, a first-run notice), which a fresh
/// profile opens at start.
fn hand_back(targets: &[Value], closed: &str, opener: Option<&str>) -> Option<String> {
    let pages: Vec<&Value> = targets
        .iter()
        .filter(|t| t["type"] == "page" && t["targetId"] != closed)
        .collect();
    let id = |t: &&Value| t["targetId"].as_str().map(str::to_string);
    let web = |t: &&&Value| {
        let url = t["url"].as_str().unwrap_or("");
        ![
            "chrome://",
            "chrome-untrusted://",
            "devtools://",
            "chrome-extension://",
        ]
        .iter()
        .any(|s| url.starts_with(s))
    };
    opener
        .and_then(|o| pages.iter().find(|t| t["targetId"] == o))
        .or_else(|| pages.iter().rev().find(web))
        .and_then(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: &str, url: &str) -> Value {
        json!({"targetId": id, "type": "page", "url": url})
    }

    #[test]
    fn the_opener_takes_over_from_the_tab_it_opened() {
        let tabs = [
            page("home", "http://x/"),
            page("other", "http://y/"),
            page("popup", "http://x/second"),
        ];
        assert_eq!(
            hand_back(&tabs, "popup", Some("home")).as_deref(),
            Some("home")
        );
    }

    #[test]
    fn without_an_opener_the_newest_web_page_takes_over() {
        // Seen with Chrome for Testing 153: its own pages are listed after
        // the user's, and handing back to one left Kai on a page with no
        // tree.
        let tabs = [
            page("home", "http://x/"),
            page("notice", "chrome://terms/"),
            page("closing", "http://x/second"),
        ];
        assert_eq!(hand_back(&tabs, "closing", None).as_deref(), Some("home"));
        // A closed opener does not count.
        assert_eq!(
            hand_back(&tabs, "closing", Some("gone")).as_deref(),
            Some("home")
        );
    }

    #[test]
    fn nothing_left_means_a_new_tab() {
        let tabs = [
            page("closing", "http://x/"),
            page("ntp", "chrome://newtab/"),
        ];
        assert_eq!(hand_back(&tabs, "closing", None), None);
        let worker = json!({"targetId": "w", "type": "service_worker", "url": "http://x/"});
        assert_eq!(hand_back(&[worker], "closing", None), None);
    }
}
