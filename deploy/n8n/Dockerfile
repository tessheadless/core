# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# n8n with the Tess nodes (Tess Trigger, Tess) preinstalled. Build from the
# repository root:  docker build -f deploy/n8n/Dockerfile -t tess-n8n .
ARG N8N_VERSION=2.43.8

FROM node:24-alpine AS nodes
WORKDIR /src
COPY integrations/n8n-nodes-tess/package.json integrations/n8n-nodes-tess/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY integrations/n8n-nodes-tess/ ./
RUN npm run build

FROM n8nio/n8n:${N8N_VERSION}
# Outside /home/node/.n8n, which is a volume and would hide the files.
COPY --from=nodes --chown=node:node /src/package.json /opt/n8n-nodes-tess/package.json
COPY --from=nodes --chown=node:node /src/dist /opt/n8n-nodes-tess/dist
ENV N8N_CUSTOM_EXTENSIONS=/opt/n8n-nodes-tess
