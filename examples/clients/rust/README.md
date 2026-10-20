# Rust client

Standalone crate (not part of the CMS workspace) using `reqwest` (rustls), `serde` and `tokio`, with
typed response structs (`Post`, `PostConnection`, `Asset`) and a `CmsError::GraphQL { code, .. }` error.
Lists published posts, searches, looks up by slug, shows error codes and, with an admin token,
creates + publishes a post and uploads a generated text file.

Prerequisites: Rust (edition 2024, i.e. 1.85+).

```sh
export TESS_URL=http://localhost:8080
cargo run
# admin part: export TESS_ADMIN_TOKEN first
```
