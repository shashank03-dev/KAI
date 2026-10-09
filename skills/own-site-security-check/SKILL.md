---
name: own-site-security-check
description: Review the public security configuration of a website or domain the user owns - HTTPS certificate, security headers, DNS and email records - and report findings with fixes. Use when the user asks to check, audit or harden their own site or domain.
license: MIT
compatibility: Uses curl, openssl and nslookup/dig (Linux) or PowerShell's Resolve-DnsName and Invoke-WebRequest (Windows).
metadata:
  author: kai
  version: "1.0"
---
# Security check of your own site

This is a read-only configuration review: it makes a few ordinary requests,
the same a browser makes. It does not probe, scan ports, guess passwords or
send attack payloads, and it is not a penetration test.

1. **Confirm ownership first.** Use `ask_user`: "Do you own or administer
   <domain>, or have permission to check it?" Continue only on a clear yes.
   If the user says no or is unsure, stop with `done` and explain why.
2. Run these as separate `shell` steps (sub-agents can run them in parallel):
   - **HTTPS certificate**: issuer, expiry and hostname.
     Linux: `echo | openssl s_client -connect <domain>:443 -servername <domain> 2>/dev/null | openssl x509 -noout -issuer -subject -dates`
     Windows: `$r=[Net.HttpWebRequest]::Create("https://<domain>"); $r.GetResponse().Close(); $c=$r.ServicePoint.Certificate; "$($c.Issuer) | expires $($c.GetExpirationDateString())"`
   - **HTTP to HTTPS redirect**: `curl -sI http://<domain>` (Windows: `curl.exe -sI http://<domain>`).
   - **Security headers**: `curl -sI https://<domain>`; look for
     `Strict-Transport-Security`, `Content-Security-Policy`,
     `X-Content-Type-Options`, `Referrer-Policy`, `Permissions-Policy`, and
     `Server`/`X-Powered-By` headers that reveal versions.
   - **Email records**: SPF, DMARC and the MX hosts.
     Linux: `dig +short TXT <domain>; dig +short TXT _dmarc.<domain>; dig +short MX <domain>`
     Windows: `Resolve-DnsName <domain> -Type TXT; Resolve-DnsName _dmarc.<domain> -Type TXT; Resolve-DnsName <domain> -Type MX`
   - **DNS CAA** (which CAs may issue certificates): `dig +short CAA <domain>`.
3. Report findings most important first, each with severity (high / medium /
   low), what it means in plain words, and the exact fix (the header line to
   add, the DNS record to create). Mention what is already good, too.

See [the reference](references/FINDINGS.md) for what each finding means and how
to fix it.
