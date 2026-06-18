#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UI_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_DIR="$(cd "${UI_DIR}/.." && pwd)"

if ! command -v docker >/dev/null 2>&1; then
  printf 'Docker is required to generate CI-compatible visual snapshots.\n' >&2
  exit 1
fi

IMAGE="lv1-scene-fade-ui-visual:latest"
NODE_MODULES_VOLUME="lv1-scene-fade-ui-node-modules"
PLATFORM="linux/amd64"
VISUAL_TEST_SCRIPT="${VISUAL_TEST_SCRIPT:-test:visual:update}"
PLAYWRIGHT_VERSION="$(node -p "require('${UI_DIR}/package-lock.json').packages['node_modules/playwright'].version")"

docker build \
  --platform "${PLATFORM}" \
  --build-arg "PLAYWRIGHT_VERSION=${PLAYWRIGHT_VERSION}" \
  --file "${UI_DIR}/Dockerfile.visual" \
  --tag "${IMAGE}" \
  "${UI_DIR}"

docker run --rm \
  --platform "${PLATFORM}" \
  --env HOME=/tmp/playwright-home \
  --env npm_config_cache=/tmp/npm-cache \
  --env PLAYWRIGHT_BROWSERS_PATH=/ms-playwright \
  --env "VISUAL_TEST_SCRIPT=${VISUAL_TEST_SCRIPT}" \
  --workdir /work/ui \
  --volume "${REPO_DIR}:/work" \
  --volume "${NODE_MODULES_VOLUME}:/work/ui/node_modules" \
  "${IMAGE}" \
  bash -lc 'npm ci && npm run "$VISUAL_TEST_SCRIPT" -- --timeout=120000'
