---
sidebar_position: 7.7
title: Raspberry Pi & small devices
---

# Raspberry Pi and small devices

Tess is a single Rust binary with an embedded database. It doesn't need a JVM,
Node.js or a database server, so it runs well on small ARM boards like the
Raspberry Pi 3, 4 and 5. Tess's creator runs their own Tess on a Raspberry Pi 3,
without n8n.

On a PC, a fresh Tess with RocksDB or SQLite uses about **30 MB of memory**
while idle. It needs more as your content, traffic and plugins grow, but the
CMS itself leaves most of a Pi free.

## Which Pi for what

| Board | Memory | Suggested setup |
|---|---|---|
| **Raspberry Pi 3** (B, B+, A+) | 512 MB – 1 GB | Tess alone: the native binary under systemd, RocksDB or SQLite, files on disk. No n8n, Typesense or PostgreSQL. |
| **Raspberry Pi 4** | 1 – 8 GB | Tess alone on 1–2 GB. Docker Compose works well. Add n8n or PostgreSQL from 4 GB. |
| **Raspberry Pi 5** | 2 – 16 GB | Docker Compose. From 4 GB, room for n8n, PostgreSQL, a cache server and Typesense. |

What uses the memory is the optional services, not Tess:

- **n8n** is a Node.js application and needs several hundred MB by itself.
  On a Pi 3, leave it off: Tess's [webhooks](../integrations/webhooks.md)
  can still call an n8n (or any other service) running on another machine.
- **Typesense** keeps its index in memory. On a Pi 3, keep `TESS_SEARCH=none`,
  or `memory` for a small site (that index is rebuilt after each restart, see
  [Search](search.md)).
- **cal.diy** is built from source, which needs about 6 GB of memory. Don't
  build it on a Pi. A [cal.diy calendar](../integrations/calendars.md) only
  needs its webhook to reach Tess, so cal.diy can run anywhere.

## Requirements

- A **64-bit OS** is best: Raspberry Pi OS Lite (64-bit), Debian or Ubuntu.
  The Pi 3, 4 and 5 all support it.
- **glibc 2.35 or later**: Raspberry Pi OS 12 "Bookworm" or later, or Ubuntu
  22.04 or later. Raspberry Pi OS 11 "Bullseye" is too old for the release
  binaries.
- A **good storage device**. The database writes often, and cheap SD cards are
  slow and wear out. Prefer an A2-rated card, or a USB SSD (the Pi 4 and 5 boot
  from USB, the Pi 5 also from an NVMe HAT).

Check what you have:

```sh
uname -m          # aarch64: 64-bit (arm64); armv7l: 32-bit (armv7)
ldd --version     # first line ends with the glibc version
```

## Install the native binary (recommended on a Pi 3)

Every [release](https://github.com/tessheadless/core/releases) has native
Linux archives: `arm64` for 64-bit systems, `armv7` for 32-bit ones. They hold
`tess`, `tess-transfer` and the admin UI.

```sh
VERSION=0.1.1     # the release you want
ARCH=arm64        # or armv7 on a 32-bit OS
curl -LO https://github.com/tessheadless/core/releases/download/v$VERSION/tess-$VERSION-linux-$ARCH.tar.xz
curl -LO https://github.com/tessheadless/core/releases/download/v$VERSION/SHA256SUMS
sha256sum -c SHA256SUMS --ignore-missing

sudo useradd --system --home /opt/tess --shell /usr/sbin/nologin tess
sudo mkdir -p /opt/tess
sudo tar -xJf tess-$VERSION-linux-$ARCH.tar.xz -C /opt/tess --strip-components=1
```

Create the configuration from the template, with random secrets:

```sh
cd /opt/tess
sudo cp .env.example .env
for key in TESS_ADMIN_TOKEN TESS_SESSION_SECRET TESS_ID_SECRET TESS_REF_SECRET TESS_SECRETS_KEY; do
  sudo sed -i "s|^$key=.*|$key=$(openssl rand -hex 32)|" .env
done
sudo mkdir -p /opt/tess/data
sudo chown -R tess:tess /opt/tess
sudo chmod 600 /opt/tess/.env
```

The defaults already suit a small device: RocksDB in `data/rocksdb`, uploads
in `data/media`, no search engine, no cache and no n8n. To use SQLite instead,
set `TESS_DATABASE=sqlite` and `DATABASE_URL=sqlite:///opt/tess/data/tess.db?mode=rwc`
(it can be [backed up while running](operations.md#backups)).

Run it as a systemd service, `/etc/systemd/system/tess.service`:

```ini
[Unit]
Description=Tess headless CMS
After=network-online.target
Wants=network-online.target

[Service]
User=tess
Group=tess
WorkingDirectory=/opt/tess
EnvironmentFile=/opt/tess/.env
ExecStart=/opt/tess/tess
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/tess/data
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now tess
curl http://localhost:8080/health
journalctl -u tess -f      # logs
```

Then open `http://<your-pi>:8080/admin/`, sign in with `TESS_ADMIN_TOKEN`
(`sudo grep TESS_ADMIN_TOKEN /opt/tess/.env`), create the **Owner** account,
empty `TESS_ADMIN_TOKEN` in `.env` and run `sudo systemctl restart tess`.

**Upgrading:** extract the new archive over `/opt/tess` (your `.env` and `data/`
are kept), then `sudo systemctl restart tess`.

## Or use Docker (Pi 4 and 5)

The Docker images are published for `arm64`, so the
[Docker](docker.md) guide works on a Pi with a 64-bit OS: download the
`arm64` Compose bundle and follow the same steps. On a 2 GB board, start
Tess alone, then add the profiles you need, one at a time.

There is no 32-bit (`armv7`) Docker image: on a 32-bit OS, use the native
binary.

## Tips for small devices

- **Don't build Tess on the Pi.** Compiling RocksDB and the rest of Tess takes a
  long time and can run out of memory on a 1 GB board. Use the release
  archives, or cross-compile on a PC for `aarch64-unknown-linux-gnu`
  (`.github/workflows/release.yml` shows the toolchains used).
- **Cache:** `TESS_CACHE=memory` speeds up repeated public queries at almost
  no cost. Lower `TESS_CACHE_MAX_ENTRIES` (default 10,000) if memory is tight.
- **HTTPS and public access:** put a reverse proxy like Caddy or nginx in front
  of port 8080, or reach the Pi through a VPN or a tunnel instead of opening a
  port on your router. See the [security checklist](security.md).
- **Back up** `/opt/tess/.env` and `/opt/tess/data` to another machine. An SD
  card can fail without warning. See [Backups](operations.md#backups).
