#!/bin/bash

set -e

CONTAINER_ID=$(docker create $1:latest)
docker cp "$CONTAINER_ID:/nitro.eif" .
docker cp "$CONTAINER_ID:/nitro.pcrs" .
docker rm "$CONTAINER_ID"
