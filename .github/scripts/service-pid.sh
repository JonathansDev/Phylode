#!/bin/sh
# Prints the process ID launchd records for the service, or nothing when it is not running.
# The daily list update and the upgrade helper run the same program, so `pgrep -x phylode`
# can find them instead.
sudo launchctl print system/com.Phylode.gatekeeper 2>/dev/null \
  | awk '$1 == "pid" && $2 == "=" { print $3; exit }'
