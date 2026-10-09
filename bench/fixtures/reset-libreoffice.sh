#!/bin/sh
# Close every LibreOffice instance and forget crash-recovery state, so the
# next start opens a plain Calc window instead of a "Document Recovery"
# dialog. Used by spreadsheet task setup/teardown. Idempotent.
pkill -x soffice.bin >/dev/null 2>&1
i=0
while pgrep -x soffice.bin >/dev/null 2>&1 && [ $i -lt 40 ]; do
  sleep 0.25
  i=$((i + 1))
done
pkill -9 -x soffice.bin >/dev/null 2>&1
user="${XDG_CONFIG_HOME:-$HOME/.config}/libreoffice/4/user"
rm -f "$user/.lock" "${XDG_CONFIG_HOME:-$HOME/.config}/libreoffice/4/.lock"
reg="$user/registrymodifications.xcu"
if [ -f "$reg" ]; then
  # Drop the recovery list entries LibreOffice writes when it is killed.
  sed -i '/RecoveryList/d; /org.openoffice.Office.Recovery:RecoveryInfo/d' "$reg"
fi
exit 0
