spacetime publish --break-clients -c -y -p tm-tourney-manager tm-tourney-manager
spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/tourney-manager --project-path tm-tourney-manager

spacetime call tm-tourney-manager create_tournament "My Tournament" "This is a beautiful tournament" "{""__timestamp_micros_since_unix_epoch__"": 1767132984000000 }" "{""__timestamp_micros_since_unix_epoch__"": 1767233084000000 }"

spacetime call tm-tourney-manager match_create "" 1 null
spacetime call tm-tourney-manager match_create "" 1 null

spacetime call tm-tourney-manager create_competition "Division 1" 1 null

spacetime call tm-tourney-manager create_competition "League Phase" 2 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null

spacetime call tm-tourney-manager create_competition "Playoffs" 2 null
spacetime call tm-tourney-manager match_create "" 4 null
spacetime call tm-tourney-manager match_create "" 4 null

spacetime call tm-tourney-manager create_competition "Division 2" 1 null

spacetime call tm-tourney-manager create_competition "League Phase" 5 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null

spacetime call tm-tourney-manager create_competition "Playoffs" 5 null
spacetime call tm-tourney-manager match_create "" 7 null
spacetime call tm-tourney-manager match_create "" 7 null

spacetime call tm-tourney-manager create_connection "{""MatchV1"": 1 }" "{""MatchV1"": 2 }" "{""Waiting"": {}}"