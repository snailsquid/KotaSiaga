#!/bin/sh
# Runs before nginx starts (nginx image /docker-entrypoint.d). Writes the set_real_ip_from list:
# CF-Connecting-IP is trusted only when the request comes from a container on one of the
# gateway's own Docker networks (the cloudflared tunnel on an STB, Coolify's Traefik on volta),
# never from any private address in general. Extra proxies: TRUSTED_PROXY_CIDRS="a/b c/d".
# If nothing can be detected the file stays empty: every client is then rate-limited by its own
# peer address, which is the safe failure.
set -eu
out=/etc/nginx/trusted_proxies.conf
: > "$out"
for cidr in $(ip -o -4 addr show scope global 2>/dev/null | awk '{print $4}'); do
  net="$(ipcalc -n "$cidr" 2>/dev/null | sed -n 's/^NETWORK=//p')"
  [ -n "$net" ] && echo "set_real_ip_from $net/${cidr#*/};" >> "$out"
done
for cidr in ${TRUSTED_PROXY_CIDRS:-}; do
  echo "set_real_ip_from $cidr;" >> "$out"
done
echo "trusted proxies: $(tr '\n' ' ' < "$out")"
