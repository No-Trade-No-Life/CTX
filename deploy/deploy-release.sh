#!/usr/bin/env bash
set -euo pipefail

tag="${1:?release tag is required}"
archive_url="${2:?archive URL is required}"
checksum_url="${3:?checksum URL is required}"
archive="ctx-x86_64-unknown-linux-gnu.tar.gz"
release_dir="/opt/ctx/releases/$tag"
temporary_dir="$(mktemp -d)"
cleanup() { rm -rf "$temporary_dir"; }
trap cleanup EXIT
database_backup=""
previous_release=""
activated=false

backup_database() {
  local database="/var/lib/ctx/.ctx/ctx.sqlite3"
  local backup_directory="/var/lib/ctx/backups"
  local backup
  [ -f "$database" ] || return 0
  install -d -o ctx -g ctx -m 0700 "$backup_directory"
  backup="$backup_directory/ctx-$(date -u +%Y%m%dT%H%M%SZ).sqlite3"
  if command -v sqlite3 >/dev/null; then
    runuser -u ctx -- sqlite3 "$database" ".backup '$backup'"
  else
    runuser -u ctx -- python3 - "$database" "$backup" <<'PY'
import sqlite3
import sys

source = sqlite3.connect(sys.argv[1])
destination = sqlite3.connect(sys.argv[2])
with destination:
    source.backup(destination)
PY
  fi
  test -s "$backup"
  database_backup="$backup"
  printf 'Backed up CTX database to %s\n' "$backup"
}

rollback_release() {
  local status=$?
  trap - ERR
  if [ "$activated" != true ]; then
    exit "$status"
  fi
  set +e
  printf 'CTX release failed; restoring the previous release and database backup.\n' >&2
  if ! systemctl stop ctx.service; then
    printf 'Rollback failed: could not stop CTX before restoring its database.\n' >&2
    exit "$status"
  fi
  if [ -n "$database_backup" ]; then
    if ! install -o ctx -g ctx -m 0600 "$database_backup" /var/lib/ctx/.ctx/ctx.sqlite3; then
      printf 'Rollback failed: could not restore the CTX database backup.\n' >&2
      exit "$status"
    fi
    if ! rm -f /var/lib/ctx/.ctx/ctx.sqlite3-wal /var/lib/ctx/.ctx/ctx.sqlite3-shm; then
      printf 'Rollback failed: could not clear CTX WAL files after restoring the database.\n' >&2
      exit "$status"
    fi
  fi
  if [ -n "$previous_release" ]; then
    if ! ln -sfn "$previous_release" /opt/ctx/current; then
      printf 'Rollback failed: could not restore the previous CTX release link.\n' >&2
      exit "$status"
    fi
    if ! systemctl start ctx.service; then
      printf 'Rollback failed: could not restart the previous CTX release.\n' >&2
      exit "$status"
    fi
  fi
  exit "$status"
}

trap rollback_release ERR

curl --fail --location --retry 5 --retry-all-errors --output "$temporary_dir/$archive" "$archive_url"
curl --fail --location --retry 5 --retry-all-errors --output "$temporary_dir/$archive.sha256" "$checksum_url"
cd "$temporary_dir"
sha256sum --check "$archive.sha256"
mkdir package
tar -xzf "$archive" -C package
install -d -m 0755 "$release_dir"
install -m 0755 package/ctx "$release_dir/ctx"
if [ -L /opt/ctx/current ]; then
  previous_release="$(readlink -f /opt/ctx/current)"
fi
backup_database
ln -sfn "$release_dir" /opt/ctx/current
activated=true
systemctl restart ctx.service
for _ in $(seq 1 30); do
  if curl --fail --silent http://127.0.0.1:8080/api/health >/dev/null; then
    activated=false
    trap - ERR
    exit 0
  fi
  sleep 2
done
systemctl status ctx.service --no-pager || true
false
