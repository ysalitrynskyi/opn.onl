#!/bin/bash
# Build and push Docker images to GitHub Container Registry
# Usage: ./scripts/build-and-push.sh [REGISTRY_USERNAME]
#
# Before running:
# 1. Create a GitHub Personal Access Token with 'write:packages' scope
# 2. Login: echo $GITHUB_TOKEN | docker login ghcr.io -u YOUR_USERNAME --password-stdin

set -e

# Configuration
REGISTRY="ghcr.io"
USERNAME="${1:-opn-onl}"
BACKEND_IMAGE="$REGISTRY/$USERNAME/opn-backend"
FRONTEND_IMAGE="$REGISTRY/$USERNAME/opn-frontend"
VERSION="${2:-latest}"

# Build args for frontend (update these for your domain)
VITE_API_URL="${VITE_API_URL:-https://api.opn.onl}"
VITE_FRONTEND_URL="${VITE_FRONTEND_URL:-https://opn.onl}"

echo "=========================================="
echo "Building opn.onl Docker images"
echo "=========================================="
echo "Registry: $REGISTRY"
echo "Username: $USERNAME"
echo "Version:  $VERSION"
echo "API URL:  $VITE_API_URL"
echo "=========================================="

# Check if logged in
if ! docker info 2>/dev/null | grep -q "Username"; then
    echo ""
    echo "WARNING: You may not be logged into the registry."
    echo "Run: echo \$GITHUB_TOKEN | docker login ghcr.io -u $USERNAME --password-stdin"
    echo ""
fi

# Build backend
echo ""
echo ">>> Building backend..."
docker build \
    --platform linux/amd64 \
    -t "$BACKEND_IMAGE:$VERSION" \
    -t "$BACKEND_IMAGE:latest" \
    ./backend

# Build frontend
echo ""
echo ">>> Building frontend..."
docker build \
    --platform linux/amd64 \
    --build-arg VITE_API_URL="$VITE_API_URL" \
    --build-arg VITE_FRONTEND_URL="$VITE_FRONTEND_URL" \
    -t "$FRONTEND_IMAGE:$VERSION" \
    -t "$FRONTEND_IMAGE:latest" \
    ./frontend

# Push images
echo ""
echo ">>> Pushing images..."
docker push "$BACKEND_IMAGE:$VERSION"
docker push "$BACKEND_IMAGE:latest"
docker push "$FRONTEND_IMAGE:$VERSION"
docker push "$FRONTEND_IMAGE:latest"

echo ""
echo "=========================================="
echo "Done! Images pushed:"
echo "  - $BACKEND_IMAGE:$VERSION"
echo "  - $FRONTEND_IMAGE:$VERSION"
echo "=========================================="
echo ""
echo "Update your .env for Portainer:"
echo "  BACKEND_IMAGE=$BACKEND_IMAGE:$VERSION"
echo "  FRONTEND_IMAGE=$FRONTEND_IMAGE:$VERSION"




