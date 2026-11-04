# boston-time.sh — the one shell renderer of Boston time (#4885, the time domain).
# @domain: time
# Every script that writes or shows a time of day calls boston_time; none runs
# its own `date` in New York time. Each form carries its zone, so no stamp is
# ambiguous across the DST change (the 14 lossy scripts, inventory 2026-11-04).
#   boston_time stamp   2026-11-04T04:12:49-0500   a record's time: an instant
#   boston_time show    2026-11-04 04:12 EST       what a person reads
#   boston_time clock   04:12 EST                  a line in a running log
boston_time() {
  local fmt
  case "${1:-stamp}" in
    stamp) fmt='%Y-%m-%dT%H:%M:%S%z' ;;
    show)  fmt='%Y-%m-%d %H:%M %Z' ;;
    clock) fmt='%H:%M:%S %Z' ;;
    *) echo "boston_time: stamp, show or clock, not '$1'" >&2; return 2 ;;
  esac
  TZ=America/New_York date "+$fmt"
}
