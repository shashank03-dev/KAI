---
name: pc-security-checkup
description: Check this computer's own security basics - firewall, pending updates, services listening on the network, disk encryption and antivirus status - and explain what to fix. Use when the user asks if their PC or laptop is secure, or to harden it.
license: MIT
metadata:
  author: kai
  version: "1.0"
---
# Security check-up of this PC

Read-only checks of the machine Kai runs on. Nothing is changed unless the user
asks for a specific fix afterwards (and every fix is `risk: high`).

Run each as its own `shell` step (they are independent, so sub-agents can run
them in parallel):

**Windows (PowerShell)**
- Firewall: `Get-NetFirewallProfile | Select Name,Enabled`
- Antivirus: `Get-MpComputerStatus | Select AMServiceEnabled,RealTimeProtectionEnabled,AntivirusSignatureLastUpdated`
- Updates: `Get-HotFix | Sort InstalledOn -Desc | Select -First 5`
- Listening services: `Get-NetTCPConnection -State Listen | Select LocalAddress,LocalPort,OwningProcess | Sort LocalPort`
- Disk encryption: `manage-bde -status C:` (needs administrator; if refused, say so)
- Remote Desktop: `(Get-ItemProperty 'HKLM:\System\CurrentControlSet\Control\Terminal Server').fDenyTSConnections`

**Linux**
- Firewall: `ufw status 2>/dev/null || firewall-cmd --state 2>/dev/null || echo "no firewall tool found"`
- Updates: `apt list --upgradable 2>/dev/null | tail -n +2 | wc -l` (or `dnf check-update -q | wc -l`)
- Listening services: `ss -tulpn 2>/dev/null | head -40`
- SSH: `grep -Ei '^(PermitRootLogin|PasswordAuthentication)' /etc/ssh/sshd_config 2>/dev/null`
- Disk encryption: `lsblk -o NAME,TYPE,FSTYPE,MOUNTPOINT | grep -i crypt || echo "no encrypted volumes found"`

Then report in plain words, most important first: what is fine, what is risky
(for example a service listening on all interfaces that the user does not
recognise, a disabled firewall, many pending updates), and the exact command
or settings page that fixes each one. Ask before running any fix.
