# Findings and fixes

| Finding | Severity | Why it matters | Fix |
| --- | --- | --- | --- |
| Certificate expires in under 14 days | High | Visitors get a hard browser error when it lapses. | Renew; automate with certbot or your host's auto-renew. |
| Certificate hostname mismatch | High | Browsers block the site. | Reissue the certificate including every hostname you serve. |
| HTTP does not redirect to HTTPS | Medium | Visitors on plain HTTP can be read or tampered with. | Redirect `http://` to `https://` with a 301. |
| No `Strict-Transport-Security` | Medium | Browsers may still try HTTP first. | `Strict-Transport-Security: max-age=31536000; includeSubDomains` |
| No `Content-Security-Policy` | Medium | Injected scripts run freely. | Start with `Content-Security-Policy: default-src 'self'` and widen as needed. |
| No `X-Content-Type-Options` | Low | Browsers may guess file types. | `X-Content-Type-Options: nosniff` |
| No `Referrer-Policy` | Low | Full URLs leak to other sites. | `Referrer-Policy: strict-origin-when-cross-origin` |
| `Server` / `X-Powered-By` shows versions | Low | Tells attackers what to look up. | Hide version strings in the web server config. |
| No SPF record | Medium | Others can send mail that looks like yours. | TXT `v=spf1 include:<your mail provider> -all` |
| No DMARC record | Medium | Spoofed mail is not rejected or reported. | TXT on `_dmarc`: `v=DMARC1; p=quarantine; rua=mailto:you@<domain>` |
| No CAA record | Low | Any CA may issue certificates for you. | CAA `0 issue "letsencrypt.org"` (your CA) |
