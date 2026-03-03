spacetime publish --break-clients -c -y -p tm-tourney-manager tm-tourney-manager
cargo r --manifest-path ../SpacetimeDB\Cargo.toml -p spacetimedb-cli generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --module-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/tourney-manager --module-path tm-tourney-manager

spacetime call tm-tourney-manager create_project "My project" "This is a beautiful project" "{""__timestamp_micros_since_unix_epoch__"": 1767132984000000 }" "{""__timestamp_micros_since_unix_epoch__"": 1767233084000000 }"

spacetime call tm-tourney-manager match_create "" 1 null
spacetime call tm-tourney-manager match_create "" 1 null

spacetime call tm-tourney-manager competition_create "Division 1" 1 0

spacetime call tm-tourney-manager competition_create "League Phase" 2 0
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null

spacetime call tm-tourney-manager competition_create "Playoffs" 2 0
spacetime call tm-tourney-manager match_create "" 4 null
spacetime call tm-tourney-manager match_create "" 4 null

spacetime call tm-tourney-manager competition_create "Division 2" 1 0

spacetime call tm-tourney-manager competition_create "League Phase" 5 0
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null

spacetime call tm-tourney-manager competition_create "Playoffs" 5 0
spacetime call tm-tourney-manager match_create "" 7 null
spacetime call tm-tourney-manager match_create "" 7 null

spacetime call tm-tourney-manager create_connection "{""MatchV1"": 1 }" "{""MatchV1"": 2 }" "{""Waiting"": {}}"