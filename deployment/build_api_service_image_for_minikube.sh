#!/usr/bin/env bash
set -euo pipefail

IMAGE_NAME="api-service"
IMAGE_TAG="latest"

# Resolve the repo root relative to this script's location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

echo "III Pointing Docker to Minikube's container runtime"
eval "$(minikube docker-env)"

echo "III Building image ${IMAGE_NAME}:${IMAGE_TAG}..."
docker build \
  -f "${REPO_ROOT}/api-service-emulator/Dockerfile.api-service" \
  -t "${IMAGE_NAME}:${IMAGE_TAG}" \
  "${REPO_ROOT}/api-service-emulator"

echo "III Done. Image '${IMAGE_NAME}:${IMAGE_TAG}' is available inside Minikube."
echo ""
echo "Make sure your deployment yaml has imagePullPolicy: Never"
