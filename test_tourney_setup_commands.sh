#!/bin/bash
set -e  # stop on errors (except where overridden)

# Try to delete DB if it exists (ignore errors if not found)
spacetime delete tm-tourney-manager || true

# Publish module as "tourney-manager"
spacetime publish --break-clients --delete-data=on-conflict -y -p tm-tourney-manager tm-tourney-manager

# Generate Rust and TS client APIs
spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/tourney-manager --project-path tm-tourney-manager

# Create tournament
spacetime call tm-tourney-manager create_tournament "My Tournament" "This is a beautiful tournament" '{"__timestamp_micros_since_unix_epoch__": 1767132984000000}' '{"__timestamp_micros_since_unix_epoch__": 1767233084000000}'

# Qualifier matches
spacetime call tm-tourney-manager create_match 1 null
spacetime call tm-tourney-manager create_match 1 null

# Division 1
spacetime call tm-tourney-manager create_competition "Division 1" 1 null

# League phase matches for Division 1
spacetime call tm-tourney-manager create_competition "League Phase" 2 null
spacetime call tm-tourney-manager create_match 3 null
spacetime call tm-tourney-manager create_match 3 null
spacetime call tm-tourney-manager create_match 3 null
spacetime call tm-tourney-manager create_match 3 null

# Playoffs for Division 1
spacetime call tm-tourney-manager create_competition "Playoffs" 2 null
spacetime call tm-tourney-manager create_match 4 null
spacetime call tm-tourney-manager create_match 4 null

# Division 2
spacetime call tm-tourney-manager create_competition "Division 2" 1 null

# League phase matches for Division 2
spacetime call tm-tourney-manager create_competition "League Phase" 5 null
spacetime call tm-tourney-manager create_match 6 null
spacetime call tm-tourney-manager create_match 6 null
spacetime call tm-tourney-manager create_match 6 null
spacetime call tm-tourney-manager create_match 6 null

# Playoffs for Division 2
spacetime call tm-tourney-manager create_competition "Playoffs" 5 null
spacetime call tm-tourney-manager create_match 7 null
spacetime call tm-tourney-manager create_match 7 null

curl -X POST http://localhost:1234/v1/database/tm-tourney-manager/call/create_connection \
     -H "Content-Type: application/json" \
     -d '[{"MatchV1": 2},{"MatchV1": 1}, {"Waiting": {}}]'