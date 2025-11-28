spacetime publish --break-clients -c -y -p tm-tourney-manager tm-tourney-manager
spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/src/gen --project-path tm-tourney-manager
spacetime call tm-tourney-manager create_tournament TestTourneyy
spacetime call tm-tourney-manager create_match 1 1 null