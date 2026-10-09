---
name: system-settings-dark-mode
description: Switch the computer between dark mode and light mode. Use when the user asks for dark mode, light mode, or to change the system theme.
license: MIT
compatibility: Linux with GNOME or KDE Plasma, or Windows 10/11.
metadata:
  author: kai
  version: "1.0"
---
# Dark mode or light mode

This changes a system setting, so every command here is `risk: high` and the
user approves it.

Use `shell` with the command for the user's OS and desktop. For light mode use
the values in brackets.

- **Windows 10/11** (PowerShell):
  `Set-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize -Name AppsUseLightTheme -Value 0; Set-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize -Name SystemUsesLightTheme -Value 0`
  (`-Value 1` for light)
- **GNOME** (Ubuntu, Fedora):
  `gsettings set org.gnome.desktop.interface color-scheme prefer-dark`
  (`default` for light)
- **KDE Plasma**: `plasma-apply-colorscheme BreezeDark` (`BreezeLight`)

If you cannot tell the desktop, first run `echo $XDG_CURRENT_DESKTOP` (Linux).
After the change, take one more look: if the windows turned dark (or light),
reply `done`. If the command failed, open the Settings app's Appearance page and
use the switch there instead.
