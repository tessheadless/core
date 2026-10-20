---
sidebar_position: 7.5
title: Phusion Passenger
---

# Phusion Passenger

Some hosts (shared hosting, cPanel/Plesk "Ruby/Node.js app" features) only run
applications through [Phusion Passenger](https://www.phusionpassenger.com/).
Passenger cannot start a Rust binary itself, but it runs Ruby (Rack) apps, so
Tess ships a small Rack launcher, `config.ru`:

```text
browser ──▶ Passenger ──┬─▶ public/admin/…  static admin UI (served by Passenger)
                        └─▶ config.ru (rack-proxy) ──▶ tess on 127.0.0.1:18080
```

- Passenger serves the prebuilt admin UI directly from `public/admin/`: it is a
  static (SSG) site and needs no application server.
- Every other request (`/graphql`, `/media/*`, `/api/*`, `/health`, admin
  client-side routes that are not files) goes to `config.ru`, which starts the
  `tess` binary on localhost, restarts it if it exits, and proxies to it with
  [rack-proxy](https://github.com/ncr/rack-proxy).

## Requirements

- Passenger 6 with **Ruby 3.1 or later** and Bundler.
- The right to run a binary from your app folder (ask your host: some shared
  plans forbid executables or long-running child processes).
- A `tess` binary for the host's platform, usually Linux x86_64. Build on a
  matching machine, in Docker, or with `TARGET=x86_64-unknown-linux-gnu`.
- Settings that keep **exactly one** app process alive (see below).

## 1. Build the package

`config.ru` and its `Gemfile` live in `deploy/passenger/`. Every
`cargo build -p tess-server` copies them next to the binary, so
`target/release/` already holds `tess`, `config.ru` and `Gemfile`.

The packaging script also builds the admin UI and assembles the folder to
upload:

```bash
scripts/package-passenger.sh
# backends to compile in, and a cross target:
FEATURES=sql,rocksdb,s3 TARGET=x86_64-unknown-linux-gnu scripts/package-passenger.sh
```

```text
dist/passenger/            (also dist/passenger.tar.gz)
├── config.ru
├── Gemfile
├── tess
├── public/admin/          admin UI
├── log/                   tess.log
├── tmp/                   lock file
└── data/                  RocksDB / SQLite / media, if stored locally
```

## 2. Upload and install the gems

Upload the folder (or the tarball) as the application root, then:

```bash
cd ~/apps/tess
chmod +x tess
bundle config set --local path vendor/bundle
bundle install
```

Point the domain's **document root** at `public/` (Passenger apps always serve
`<app root>/public` statically).

## 3. Configure Passenger

Tess must run as **one** process that Passenger never stops: `config.ru`
supervises a single `tess` child, and the embedded backends (RocksDB, SQLite,
filesystem media) do not support several processes. Also use **direct**
spawning, so the child is not started in Passenger's preloader.

Nginx integration:

```nginx
server {
    server_name cms.example.com;
    root /home/me/apps/tess/public;

    passenger_enabled on;
    passenger_app_type rack;
    passenger_spawn_method direct;
    passenger_min_instances 1;
    passenger_max_pool_size 1;
    passenger_pool_idle_time 0;
    passenger_max_request_queue_size 200;
    client_max_body_size 100m;          # media uploads, see TESS_MEDIA_MAX_UPLOAD_BYTES

    passenger_env_var TESS_DATABASE rocksdb;
    passenger_env_var ROCKSDB_PATH /home/me/apps/tess/data/rocksdb;
    passenger_env_var TESS_MEDIA_DIR /home/me/apps/tess/data/media;
    passenger_env_var TESS_ID_SECRET …;
    passenger_env_var TESS_REF_SECRET …;
    passenger_env_var TESS_SESSION_SECRET …;
    passenger_env_var TESS_ADMIN_TOKEN …;   # first run only
}
# Keep the app (and tess) running after a restart, without waiting for a request:
passenger_pre_start https://cms.example.com/health;
```

Apache integration (`.htaccess` or virtual host):

```apache
PassengerEnabled on
PassengerAppType rack
PassengerSpawnMethod direct
PassengerMinInstances 1
PassengerMaxPoolSize 1
PassengerPoolIdleTime 0
SetEnv TESS_DATABASE rocksdb
SetEnv ROCKSDB_PATH /home/me/apps/tess/data/rocksdb
SetEnv TESS_ID_SECRET …
# … other variables
```

On control panels (cPanel "Setup Ruby App", Plesk), set the application root,
`config.ru` as the startup file, and the variables in the environment section.
If you cannot change the pool settings, ask your host, or see
[Running tess separately](#running-tess-separately).

Every [configuration variable](configuration.md) is passed to `tess` through
the environment of the Ruby process. Never put secrets in files inside the app
folder. `config.ru` itself reads:

| Variable | Default | Description |
|---|---|---|
| `TESS_INTERNAL_PORT` | `18080` | Local port `tess` listens on (`PORT` of the child). Change it if the port is taken. |
| `TESS_BIN` | `<app root>/tess` | Path of the binary. |
| `TESS_ADMIN_DIR` | `<app root>/public/admin` | Admin UI folder, used by `tess` for client-side routes. |
| `TESS_LOG_FILE` | `<app root>/log/tess.log` | Output of `tess`. |

## 4. Choose the storage

- **Database:** RocksDB or SQLite in `data/` (one instance, on a disk that is
  backed up), or a managed PostgreSQL/MySQL if your host provides one. See
  [Databases](databases.md).
- **Media:** `TESS_MEDIA_STORE=fs` with `TESS_MEDIA_DIR` in `data/`, or any
  S3-compatible bucket. See [Media storage](media-storage.md).

## 5. First run

1. Restart the app (`touch tmp/restart.txt` or the panel's restart button).
2. Check `https://cms.example.com/health`: it answers `{"status":"ok",…}`.
3. Open `https://cms.example.com/admin/`, sign in with `TESS_ADMIN_TOKEN` and
   create the Owner account.
4. Remove `TESS_ADMIN_TOKEN` and restart.

## Upgrading

Upload the new `tess` binary and `public/admin/` folder, then restart the app.
`tess` migrates the database at startup. Back up `data/` first; see
[Operations](operations.md).

## Troubleshooting

| Symptom | Check |
|---|---|
| `502 {"error":"tess is starting or unavailable"}` | `log/tess.log`: missing secrets, wrong binary platform (`exec format error`), port taken (change `TESS_INTERNAL_PORT`). |
| `413 Request Entity Too Large` on uploads | `client_max_body_size` (Nginx) or `LimitRequestBody` (Apache). |
| RocksDB "lock" errors | Several app processes: set the pool to one process and direct spawning. |
| The admin loads but data never appears | The document root must be `public/`, and `/graphql` must reach `config.ru`. |
| Restarts after idle periods are slow | `passenger_pool_idle_time 0` and `passenger_pre_start`. |

## Limitations

- **One instance.** Passenger cannot scale Tess horizontally here. Use
  [Docker](docker.md) or [AWS Lambda](aws-lambda.md) for several replicas.
- **Buffered responses.** rack-proxy runs with `streaming: false`, so each
  response is buffered before it is sent. Regular GraphQL, upload and media
  requests are fine, but long-lived streams (server-sent events) are not
  supported through this proxy.

## Running tess separately

If your host lets you keep a process running by other means (systemd user
units, supervisord, a process manager in the panel), start `tess` that way on
`127.0.0.1:$TESS_INTERNAL_PORT`. `config.ru` then finds it already listening,
does not start a second one, and only proxies. This is more robust, because
`tess` no longer depends on Passenger's process lifecycle.
