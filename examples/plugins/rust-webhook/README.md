# rust-webhook (Rust, after_write + HTTP)

Sends `{"event": "entry.publish", "contentType", "ref", "updatedAt", "publishedAt"}`
to a URL after changes — e.g. to trigger a static-site rebuild on publish.

```sh
cargo build --release --target wasm32-unknown-unknown
# -> target/wasm32-unknown-unknown/release/webhook_plugin.wasm
```

Config: `{"url": "https://hooks.example.com/cms", "actions": ["publish", "unpublish"]}`.
Add the webhook host (`hooks.example.com`) to the plugin's **allowed hosts**:
plugins have no network access otherwise. Failures never block the write;
they show up as the plugin's last error in the admin.
