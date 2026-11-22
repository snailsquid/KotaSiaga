# Gateway image: nginx with the repo's config and the API docs baked in. Built and pushed by CI
# (amd64), so the deploy host only pulls. Coolify cannot bind-mount ./gateway/nginx.conf: in a
# git-based compose deployment relative paths resolve under /data/coolify/applications/<uuid>/.
#   docker build -t kotasiaga-gateway gateway
# docs/openapi.yaml is a copy of the repo-root openapi.yaml (scripts/sync_openapi.sh; a
# core-service test fails when they differ), because the build context is this folder.
FROM nginx:1.28-alpine
COPY nginx.conf /etc/nginx/nginx.conf
COPY docs /usr/share/nginx/html/docs
