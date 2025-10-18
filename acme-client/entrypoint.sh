#!/bin/sh
set -e

echo "========================================="
echo "dstack ACME Client"
echo "========================================="
echo "Starting nginx on port 80..."
echo "ACME challenges will be served from /var/www/certbot/.well-known/acme-challenge/"
echo ""
echo "Endpoints:"
echo "  - /.well-known/acme-challenge/ - ACME challenge endpoint"
echo "  - /health - Health check"
echo "  - / - File browser"
echo ""
echo "To manually create a test challenge:"
echo "  echo 'test-response' > /var/www/certbot/.well-known/acme-challenge/test-token"
echo ""
echo "To request a certificate:"
echo "  certbot certonly --webroot -w /var/www/certbot -d your-domain.com"
echo "========================================="

# Start nginx in the foreground
exec nginx -c /etc/nginx/nginx.conf
