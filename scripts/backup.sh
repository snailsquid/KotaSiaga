#!/bin/sh
# Backs up both SQLite databases from the running docker compose stack (AGENTS.md section 9).
#
#   scripts/backup.sh [backup_dir]        default: ./backups
#
# Each service binary makes a consistent online copy with `VACUUM INTO` (safe in WAL mode; never
# copy only the .db file) and checks it with `PRAGMA integrity_check`. The images are distroless
# (no shell), so the copy is made by the service binary itself and fetched with `docker compose cp`.
# Keeps the newest 14 backups per service. Schedule it, e.g. daily with cron:
#   15 3 * * * cd /path/to/repo && scripts/backup.sh >> backups/backup.log 2>&1
set -eu
cd "$(dirname "$0")/.."
dir="${1:-backups}"
keep=14
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$dir"
for svc in core-service identity-service; do
  name="${svc%-service}"
  docker compose exec -T "$svc" /app backup /data/backup.db
  docker compose cp "$svc:/data/backup.db" "$dir/$name-$stamp.db"
  echo "saved $dir/$name-$stamp.db ($(wc -c < "$dir/$name-$stamp.db") bytes)"
  # Rotate: newest $keep stay.
  ls -1t "$dir/$name-"*.db 2>/dev/null | tail -n +$((keep + 1)) | while read -r old; do rm -f "$old"; done
done
