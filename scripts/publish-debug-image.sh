#!/usr/bin/env bash

# fail asap
set -e

# Check if an argument was provided
if [ $# -eq 0 ]; then
    echo "No arguments provided"
    echo "Usage: scripts/publish-debug-image.sh 20230826-1 true"
    echo ""
    echo "Last argument specifies whether we should have a debug build as opposed to release build."
    exit 1
fi

DEBUG=$2
if [ "$DEBUG" = "true" ]; then
  echo "[profile.release]" >> Cargo.toml
  echo "debug = true" >> Cargo.toml
fi

TAG=$1
echo "Building images, will tag for ghcr.io with $TAG!"
docker build -t ghcr.io/chalkpe/stoat-backend-base:latest -f Dockerfile.useCurrentArch .
docker build -t ghcr.io/chalkpe/stoat-backend-server:$TAG - < crates/delta/Dockerfile
docker build -t ghcr.io/chalkpe/stoat-backend-bonfire:$TAG - < crates/bonfire/Dockerfile
docker build -t ghcr.io/chalkpe/stoat-backend-autumn:$TAG - < crates/services/autumn/Dockerfile
docker build -t ghcr.io/chalkpe/stoat-backend-january:$TAG - < crates/services/january/Dockerfile
docker build -t ghcr.io/chalkpe/stoat-backend-gifbox:$TAG - < crates/services/gifbox/Dockerfile
docker build -t ghcr.io/chalkpe/stoat-backend-crond:$TAG - < crates/daemons/crond/Dockerfile
docker build -t ghcr.io/chalkpe/stoat-backend-pushd:$TAG - < crates/daemons/pushd/Dockerfile

if [ "$DEBUG" = "true" ]; then
  git restore Cargo.toml
fi

docker push ghcr.io/chalkpe/stoat-backend-server:$TAG
docker push ghcr.io/chalkpe/stoat-backend-bonfire:$TAG
docker push ghcr.io/chalkpe/stoat-backend-autumn:$TAG
docker push ghcr.io/chalkpe/stoat-backend-january:$TAG
docker push ghcr.io/chalkpe/stoat-backend-gifbox:$TAG
docker push ghcr.io/chalkpe/stoat-backend-crond:$TAG
docker push ghcr.io/chalkpe/stoat-backend-pushd:$TAG
