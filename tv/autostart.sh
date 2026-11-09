#!/bin/sh
# Luma's optional cold-boot hook. Installed only when explicitly requested:
#   sh /media/developer/apps/usr/palm/applications/com.archbtw.luma/autostart.sh enable
#   sh /media/developer/apps/usr/palm/applications/com.archbtw.luma/autostart.sh disable
# Requires a rooted TV with Homebrew Channel, which runs /var/lib/webosbrew/init.d at boot.
APP_ID=com.archbtw.luma
APP_DIR=/media/developer/apps/usr/palm/applications/$APP_ID
HOOK=/var/lib/webosbrew/init.d/65-luma
case "${1:-}" in
  enable)
    [ "$(id -u)" = 0 ] || { printf '%s\n' 'Run as root.' >&2; exit 1; }
    [ -f "$APP_DIR/appinfo.json" ] || { printf '%s\n' 'Install Luma first.' >&2; exit 1; }
    mkdir -p /var/lib/webosbrew/init.d
    if [ -e "$HOOK" ] || [ -L "$HOOK" ]; then
      [ "$(readlink "$HOOK")" = "$APP_DIR/autostart.sh" ] || { printf '%s\n' '65-luma is already used by another file.' >&2; exit 1; }
    fi
    chmod 755 "$APP_DIR/autostart.sh"
    ln -sf "$APP_DIR/autostart.sh" "$HOOK"
    printf '%s\n' 'Luma will open after a full boot.'
    exit 0 ;;
  disable)
    [ "$(id -u)" = 0 ] || { printf '%s\n' 'Run as root.' >&2; exit 1; }
    if [ "$(readlink "$HOOK")" = "$APP_DIR/autostart.sh" ]; then rm -f "$HOOK"; fi
    printf '%s\n' 'Luma boot launch disabled.'
    exit 0 ;;
  status)
    if [ "$(readlink "$HOOK" 2>/dev/null)" = "$APP_DIR/autostart.sh" ]; then
      printf '%s\n' 'Luma boot launch is enabled.'
    else
      printf '%s\n' 'Luma boot launch is disabled.'
    fi
    exit 0 ;;
  ''|start) ;;
  *)
    printf '%s\n' "Usage: $0 enable|disable|status" >&2
    exit 2 ;;
esac
# A lock lives for this boot only; do not launch twice when the hook is invoked again.
(
    exec 9>/tmp/luma-boot.lock
    flock -n 9 || exit 0
    [ -f /tmp/luma-boot-done ] && exit 0
    [ -f "$APP_DIR/appinfo.json" ] || exit 0
    sleep 25
    # -n 1 prints the single reply; -w 5000 waits up to five seconds for it.
    RESULT="$(/usr/bin/luna-send -n 1 -w 5000 luna://com.webos.service.applicationmanager/launch "{\"id\":\"$APP_ID\"}" 2>/dev/null)"
    printf '%s\n' "$RESULT"
    case "$RESULT" in *'"returnValue":true'*|*'"returnValue": true'*) touch /tmp/luma-boot-done ;; esac
) </dev/null >/tmp/luma-boot.log 2>&1 &
