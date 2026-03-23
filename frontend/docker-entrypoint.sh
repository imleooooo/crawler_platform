#!/bin/sh
set -e
# Substitute only ${API_KEY} — all other $nginx_variables are left untouched.
envsubst '$API_KEY' \
    < /etc/nginx/templates/default.conf.template \
    > /etc/nginx/conf.d/default.conf
exec "$@"
