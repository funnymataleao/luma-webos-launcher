#!/bin/sh
# Luma on wake. Quick Start+ resumes the TV without booting, so
# 70-luma-autostart never runs after the power button. This watches the
# power state and opens Luma each time the TV comes back to Active.
# LG system apps are not changed.
# Installed and switched by Luma Settings › System › Root features.
# Disable: touch /var/lib/webosbrew/luma-autostart-disabled
if [ -z "$LUMA_WAKE_CHILD" ]; then
  LUMA_WAKE_CHILD=1 nohup "$0" > /tmp/luma-wake.log 2>&1 < /dev/null &
  exit 0
fi
pidfile=/tmp/luma-wake.pid
if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
  exit 0
fi
echo $$ > "$pidfile"
app=/media/developer/apps/usr/palm/applications/com.denisefremov.luma/appinfo.json

open_luma() {
  [ -e /var/lib/webosbrew/luma-autostart-disabled ] && return
  [ -f "$app" ] || return
  for attempt in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
    # Launch first: right after resume the foreground report can be stale.
    reply=$(luna-send -n 1 luna://com.webos.service.applicationManager/launch '{"id":"com.denisefremov.luma"}')
    echo "$(date '+%T') wake attempt $attempt: $reply"
    sleep 1
    active=$(luna-send -n 1 luna://com.webos.applicationManager/getForegroundAppInfo '{}')
    echo "$(date '+%T') foreground: $active"
    case "$active" in
      *'"appId":"com.denisefremov.luma"'*) [ "$attempt" -ge 3 ] && return ;;
    esac
  done
}

# Resume reports "Prepare Resume", "LastInput Ready", "Screen On" (onOff on)
# about four seconds before "Active"; open Luma at the first of them.
while true; do
  armed=1
  luna-send -i luna://com.webos.service.tvpower/power/getPowerState '{"subscribe":true}' |
    while read -r line; do
      echo "$(date '+%T') power: $line"
      line=$(echo "$line" | tr -d ' ')
      case "$line" in
        *'"onOff":"off"'*) armed=1 ;;
        *'"onOff":"on"'*)
          if [ "$armed" = 1 ]; then
            armed=0
            open_luma &
          fi
          ;;
      esac
    done
  sleep 2
done
