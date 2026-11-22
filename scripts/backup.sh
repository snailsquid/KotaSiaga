#!/bin/sh
# Backs up both SQLite databases of a running KotaSiaga stack (AGENTS.md section 9).
#
#   scripts/backup.sh [backup_dir]                 default: ./backups
#   PROJECT=<compose project> scripts/backup.sh     when several stacks run on the host
#
# Works on any Docker host with a shell, without the repo's compose file: it finds the containers
# by their compose labels (com.docker.compose.service=core-service / identity-service). That is
# what makes it usable on Coolify, where the compose project is named after the app's UUID (find
# it with: docker ps --format '{{.Label "com.docker.compose.project"}} {{.Names}}' | grep core).
# On an STB the project is `kotasiaga`. Run it on the host itself (SSH), not in a container.
#
# Each service binary makes a consistent online copy with `VACUUM INTO` (safe in WAL mode; never
# copy only the .db file) and checks it with `PRAGMA integrity_check`. The images are distroless
# (no shell), so the copy is made by the binary itself (`/app backup /data/backup.db --overwrite`;
# it refuses to write over the live database) and fetched with `docker cp`.
# Keeps the newest 14 backups per service. Schedule it, e.g. daily with cron:
#   15 3 * * * PROJECT=kotasiaga /path/to/backup.sh /var/backups/kotasiaga >> /var/backups/kotasiaga/backup.log 2>&1
set -eu
dir="${1:-backups}"
keep=14
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$dir"

find_container() {
  filter="--filter label=com.docker.compose.service=$1 --filter status=running"
  [ -n "${PROJECT:-}" ] && filter="$filter --filter label=com.docker.compose.project=$PROJECT"
  # shellcheck disable=SC2086
  names="$(docker ps $filter --format '{{.Names}}')"
  count="$(printf '%s\n' "$names" | grep -c . || true)"
  if [ "$count" -ne 1 ]; then
    echo "backup.sh: expected 1 running $1 container, found $count${names:+: $names}. Set PROJECT=<compose project>." >&2
    exit 1
  fi
  printf '%s' "$names"
}

for svc in core-service identity-service; do
  name="${svc%-service}"
  c="$(find_container "$svc")"
  docker exec "$c" /app backup /data/backup.db --overwrite
  docker cp "$c:/data/backup.db" "$dir/$name-$stamp.db"
  echo "saved $dir/$name-$stamp.db ($(wc -c < "$dir/$name-$stamp.db" | tr -d ' ') bytes) from $c"
  # Rotate: newest $keep stay.
  ls -1t "$dir/$name-"*.db 2>/dev/null | tail -n +$((keep + 1)) | while read -r old; do rm -f "$old"; done
done
