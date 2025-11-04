#!/bin/bash
set -e  # stop on errors (except where overridden)

# Try to delete DB if it exists (ignore errors if not found)
spacetime delete tm-tourney-manager || true

# Publish module as "tourney-manager"
spacetime publish -c -y -p tm-tourney-manager tm-tourney-manager

# Generate Rust and TS client APIs
spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/src/gen --project-path tm-tourney-manager

spacetime call tm-tourney-manager create_tournament TestTourney
spacetime call tm-tourney-manager create_competition "Discovery#1" [1759572000000000] 1 1 null
spacetime call tm-tourney-manager create_match 1 1 null false
spacetime sql tm-tourney-manager "SELECT * FROM tournament"
spacetime sql tm-tourney-manager "SELECT * FROM competition"
spacetime sql tm-tourney-manager "SELECT * FROM tm_match"