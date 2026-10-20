# rust-slugify (Rust, before_write)

Fills a slug field from a source field (`"Été à Paris !"` → `ete-a-paris`)
when it is empty, and rejects entries containing forbidden words.

```sh
cargo build --release --target wasm32-unknown-unknown
# -> target/wasm32-unknown-unknown/release/slugify_plugin.wasm
```

Config: `{"sourceField": "title", "slugField": "slug", "forbiddenWords": ["casino"]}`.
Scope it to content types that have both fields.
