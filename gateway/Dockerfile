# Gateway image: nginx with the repo's config baked in. Built and pushed by CI (amd64), so the
# deploy host only pulls. Coolify cannot bind-mount ./gateway/nginx.conf: in a git-based compose
# deployment relative paths resolve under /data/coolify/applications/<uuid>/, not the checkout.
#   docker build -t kotasiaga-gateway gateway
FROM nginx:1.28-alpine
COPY nginx.conf /etc/nginx/nginx.conf
