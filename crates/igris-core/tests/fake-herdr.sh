#!/bin/sh
# Fake `herdr` for tests. Records each call and replays recorded output.
#   $IGRIS_FAKE_HERDR/calls/NNNNNN   one file per call, arguments separated by NUL
#   $IGRIS_FAKE_HERDR/<key>.out      stdout, exit 0   (key = first two non-flag args joined by '-')
#   $IGRIS_FAKE_HERDR/<key>.err      stderr, exit 1
#   $IGRIS_FAKE_HERDR/queue/<key>.NNNN.(out|err)   served once, in order, before the plain file
# No fixture for a command: message on stderr, exit 97, so a test never passes by accident.
dir=${IGRIS_FAKE_HERDR:?IGRIS_FAKE_HERDR not set}

n=$(ls "$dir/calls" | wc -l | tr -d ' ')
printf '%s\0' "$@" > "$dir/calls/$(printf '%06d' "$n")"

key= count=0 skip=0
for a in "$@"; do
  if [ "$skip" = 1 ]; then skip=0; continue; fi
  case "$a" in
    --machine|--session) skip=1 ;;
    -*) ;;
    *) key=${key:+$key-}$a; count=$((count + 1)); [ "$count" = 2 ] && break ;;
  esac
done

serve() { # $1 = file
  case "$1" in
    *.err) cat "$1" >&2; code=1 ;;
    *) cat "$1"; code=0 ;;
  esac
  [ "${2:-}" = once ] && rm -f "$1"
  exit "$code"
}

for f in "$dir"/queue/"$key".*; do
  [ -e "$f" ] && serve "$f" once
done
for f in "$dir/$key.out" "$dir/$key.err"; do
  [ -e "$f" ] && serve "$f"
done
echo "fake-herdr: no fixture for $key" >&2
exit 97
