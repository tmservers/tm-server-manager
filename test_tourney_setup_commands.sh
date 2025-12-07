#!/bin/bash
set -e  # stop on errors (except where overridden)

# Try to delete DB if it exists (ignore errors if not found)
spacetime delete tm-tourney-manager || true

# Publish module as "tourney-manager"
spacetime publish --break-clients --delete-data=on-conflict -y -p tm-tourney-manager tm-tourney-manager

# Generate Rust and TS client APIs
spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/src --project-path tm-tourney-manager

spacetime call tm-tourney-manager create_tournament TestTourneyy
spacetime call tm-tourney-manager create_match 1 null
spacetime call tm-tourney-manager create_competition "whatever" 1 null