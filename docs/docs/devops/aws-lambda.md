---
sidebar_position: 7
title: AWS Lambda
---

# AWS Lambda

The same `tess` binary runs as a Lambda function when it detects the Lambda
runtime. The SAM template `deploy/aws/template.yaml` deploys:

- the function: `provided.al2023`, arm64, built with cargo-lambda, exposed
  through a **Function URL**;
- a **DynamoDB** table (`pk`/`sk`, on-demand, point-in-time recovery,
  encryption);
- a private **S3** bucket for media, with a CORS rule for browser uploads;
- environment variables resolved from **AWS Secrets Manager**.

## Deploy

1. Create the secrets (one random value each):

   ```bash
   aws secretsmanager create-secret --name tess/secrets --secret-string \
     '{"adminToken":"…","idSecret":"…","refSecret":"…","sessionSecret":"…","typesenseApiKey":""}'
   ```

2. Build and deploy (needs the SAM CLI and cargo-lambda):

   ```bash
   cd deploy/aws
   sam build --beta-features
   sam deploy --guided
   ```

   Parameters: `SecretName` (default `tess/secrets`), `TypesenseUrl` (empty
   disables search) and `CorsOrigins`.

3. The `ApiUrl` output is your endpoint. Create the first Owner with the
   admin token (see [Quick start](../getting-started/quick-start.md)), then
   remove `adminToken` from the secret and redeploy.

## Building by hand

```bash
cargo lambda build --release --arm64 -p tess-server \
  --no-default-features --features dynamodb,sql,lambda,s3
# output: target/lambda/tess/bootstrap
```

:::warning arm64 cross-compilation
Some zig versions fail to link aarch64 with `unsupported linker arg:
--fix-cortex-a53-843419`. Use a zig release that supports the flag, or build
with `--x86-64` and set `Architectures: [x86_64]` in the template.
:::

## How it behaves on Lambda

- **Startup**: on a cold start, Tess runs migrations, loads the content
  model and the enabled plugins, and builds the GraphQL schema. Warm
  invocations reuse them, and changes made elsewhere are picked up within
  `TESS_SCHEMA_REFRESH_SECS`.
- **Backends**: RocksDB, SQLite and the filesystem media store are refused,
  because nothing on the Lambda filesystem persists. PostgreSQL works too
  (RDS, Aurora, Neon); the pool defaults to 2 connections per execution
  environment.
- **Uploads**: requests are limited to 6 MB by Lambda, so use direct uploads
  (`createAssetUpload`) for media; see [Media storage](media-storage.md).
- **Logs** are JSON, in CloudWatch.

:::danger Secrets in environment variables
The template resolves secrets into environment variables at deploy time.
Anyone allowed to call `lambda:GetFunctionConfiguration` can read them, so
restrict that permission.
:::
