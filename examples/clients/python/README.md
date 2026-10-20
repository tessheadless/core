# Python client

`requests`-based script: list published posts, full-text search (`searchPosts`), lookup by slug,
GraphQL error codes, and with an admin token: create a draft post, publish it and upload a small
generated text file to `/media/upload`.

Prerequisites: Python 3.9+, `pip install -r requirements.txt`.

```sh
export TESS_URL=http://localhost:8080
python3 cms_client.py                 # read-only part
TESS_ADMIN_TOKEN=... python3 cms_client.py   # or: source your dev env file first
```
