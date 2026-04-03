spacetime publish --break-clients -c -y -p tm-server-manager tm-server-manager
cargo r --manifest-path ../SpacetimeDB\Cargo.toml -p spacetimedb-cli generate --yes --lang rust --out-dir tm-server-manager-api-rs/src/generated --module-path tm-server-manager
spacetime generate --yes --lang typescript --out-dir tm-server-manager-api-ts/server-manager --module-path tm-server-manager

spacetime call tm-server-manager create_project "My project" "This is a beautiful project" "{""Tournament"": {}}" "{""__timestamp_micros_since_unix_epoch__"": 1767132984000000 }" "{""__timestamp_micros_since_unix_epoch__"": 1767233084000000 }"

spacetime call tm-server-manager match_create "" 1 0
spacetime call tm-server-manager match_create "" 1 0

spacetime call tm-server-manager competition_create "Division 1" 1 0

spacetime call tm-server-manager competition_create "League Phase" 2 0
spacetime call tm-server-manager match_create "" 3 0
spacetime call tm-server-manager match_create "" 3 0
spacetime call tm-server-manager match_create "" 3 0
spacetime call tm-server-manager match_create "" 3 0

spacetime call tm-server-manager competition_create "Playoffs" 2 0
spacetime call tm-server-manager match_create "" 4 0
spacetime call tm-server-manager match_create "" 4 0

spacetime call tm-server-manager competition_create "Division 2" 1 0

spacetime call tm-server-manager competition_create "League Phase" 5 0
spacetime call tm-server-manager match_create "" 6 0
spacetime call tm-server-manager match_create "" 6 0
spacetime call tm-server-manager match_create "" 6 0
spacetime call tm-server-manager match_create "" 6 0

spacetime call tm-server-manager competition_create "Playoffs" 5 0
spacetime call tm-server-manager match_create "" 7 0
spacetime call tm-server-manager match_create "" 7 0

spacetime call tm-server-manager connection_create "{""MatchV1"": 1 }" "{""MatchV1"": 2 }" "{""Wait"": {}}"