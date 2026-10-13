//! Errors in words a person can act on.

/// Turn an error into words a person can act on.
pub fn friendly(e: &anyhow::Error) -> String {
    let s = format!("{e:#}");
    let l = s.to_ascii_lowercase();
    // Kai's own browser explains itself (no browser found, closed, profile in
    // use); its words must not be mistaken for a model-server problem.
    if l.contains("kai's browser") || l.contains("kai browser") || l.contains("chromium") {
        return s;
    }
    if l.contains("401")
        || l.contains("403")
        || l.contains("api key not valid")
        || l.contains("unauthorized")
    {
        "The key was refused (401/403). Check the API key in Settings › Model & server.".into()
    } else if l.contains("timed out") || l.contains("timeout") {
        "The model didn't answer in time. Is the Droplet up? Run deploy/digitalocean/up.sh, or add a Gemini key so Kai can fall back.".into()
    } else if l.contains("dns") || l.contains("connect") || l.contains("transport") {
        "Kai couldn't reach the model server. Check the address and your internet connection."
            .into()
    } else if l.contains("no digitalocean")
        || l.contains("no gemini")
        || l.contains("set up the digitalocean")
    {
        "No model is set up yet. Add the DigitalOcean server or a Gemini key in Settings › Model & server.".into()
    } else if l.contains("404") {
        "The server answered 404: the model name or address is wrong. The address should end in /v1.".into()
    } else {
        format!("Something went wrong: {s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_read_as_next_steps() {
        assert!(
            friendly(&anyhow::anyhow!("backend returned status 401: nope"))
                .contains("key was refused")
        );
        assert!(friendly(&anyhow::anyhow!("backend timed out after 45s")).contains("Droplet up"));
        assert!(
            friendly(&anyhow::anyhow!(
                "set up the DigitalOcean server or a Gemini key"
            ))
            .contains("No model is set up")
        );
        let lane = "Kai's browser was closed; could not connect";
        assert_eq!(friendly(&anyhow::anyhow!(lane)), lane);
    }
}
