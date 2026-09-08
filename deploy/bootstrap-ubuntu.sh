#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install --yes ca-certificates caddy curl
id ctx >/dev/null 2>&1 || useradd --system --home-dir /var/lib/ctx --shell /usr/sbin/nologin ctx
install -d -m 0755 /opt/ctx/releases
install -d -o ctx -g ctx -m 0700 /var/lib/ctx

install -m 0644 /dev/stdin /etc/systemd/system/ctx.service <<'UNIT'
[Unit]
Description=CTX AI-native Markdown Context CMS
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ctx
Group=ctx
WorkingDirectory=/var/lib/ctx
ExecStart=/opt/ctx/current/ctx
Restart=on-failure
RestartSec=5s
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/ctx

[Install]
WantedBy=multi-user.target
UNIT

install -m 0644 /dev/stdin /etc/caddy/Caddyfile <<'CADDY'
ctx.ntnl.io {
    reverse_proxy 127.0.0.1:8080
}
CADDY

systemctl daemon-reload
systemctl enable ctx.service caddy.service
systemctl restart caddy.service
