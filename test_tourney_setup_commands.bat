spacetime publish --break-clients --delete-data=on-conflict -y -p tm-tourney-manager tm-tourney-manager
spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/tourney-manager --project-path tm-tourney-manager

spacetime publish --break-clients --delete-data=on-conflict -y -p tm-tourney-manager tm-tourney-manager

spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/tourney-manager --project-path tm-tourney-manager

spacetime call tm-tourney-manager create_tournament "My Tournament"

spacetime call tm-tourney-manager create_match 1 null
spacetime call tm-tourney-manager create_match 1 null

spacetime call tm-tourney-manager create_competition "Division 1" 1 null

spacetime call tm-tourney-manager create_competition "League Phase" 2 null
spacetime call tm-tourney-manager create_match 3 null
spacetime call tm-tourney-manager create_match 3 null
spacetime call tm-tourney-manager create_match 3 null
spacetime call tm-tourney-manager create_match 3 null

spacetime call tm-tourney-manager create_competition "Playoffs" 2 null
spacetime call tm-tourney-manager create_match 4 null
spacetime call tm-tourney-manager create_match 4 null

spacetime call tm-tourney-manager create_competition "Division 2" 1 null

spacetime call tm-tourney-manager create_competition "League Phase" 5 null
spacetime call tm-tourney-manager create_match 6 null
spacetime call tm-tourney-manager create_match 6 null
spacetime call tm-tourney-manager create_match 6 null
spacetime call tm-tourney-manager create_match 6 null

spacetime call tm-tourney-manager create_competition "Playoffs" 5 null
spacetime call tm-tourney-manager create_match 7 null
spacetime call tm-tourney-manager create_match 7 null