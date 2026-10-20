# Lua client

Lua 5.1–5.4 / LuaJIT with `luasocket` and `dkjson`: list published posts, full-text search, lookup by slug,
error codes (errors are raised as `{ code, message }` tables), and with an admin token: create + publish a
post and upload a generated text file.

```sh
luarocks install luasocket
luarocks install dkjson
export TESS_URL=http://localhost:8080
lua cms_client.lua
# admin part: export TESS_ADMIN_TOKEN first
```

`socket.http` only speaks plain HTTP; for an `https://` CMS install `luasec` and use `ssl.https`
(same `request{...}` API).
