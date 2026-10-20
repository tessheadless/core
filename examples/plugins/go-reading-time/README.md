# go-reading-time (Go)

Computes `ceil(words / wordsPerMinute)` of a text field and stores it in an
Int field on create/update.

```sh
GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o reading-time.wasm .
```

Config: `{"sourceField": "body", "targetField": "readingTime", "wordsPerMinute": 200}`.
The target field must exist on the content type (e.g. `readingTime: INT`);
scope the plugin to those content types in the admin.
