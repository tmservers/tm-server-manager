#!/bin/bash
set -e  # stop on errors (except where overridden)

# Try to delete DB if it exists (ignore errors if not found)
spacetime delete tm-tourney-manager || true

# Publish module as "tourney-manager"
spacetime publish --break-clients --delete-data=on-conflict -y -p tm-tourney-manager tm-tourney-manager

# Generate Rust and TS client APIs
spacetime generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --module-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/tourney-manager --module-path tm-tourney-manager

# Create tournament
spacetime call tm-tourney-manager create_project "My Tournament" "This is a beautiful tournament" '{"__timestamp_micros_since_unix_epoch__": 1777132984000000}' '{"__timestamp_micros_since_unix_epoch__": 1777233084000000}'

# Qualifier matches
spacetime call tm-tourney-manager match_create "" 1 null
spacetime call tm-tourney-manager match_create "" 1 null

# Division 1
spacetime call tm-tourney-manager create_competition "Division 1" 1 null

# League phase matches for Division 1
spacetime call tm-tourney-manager create_competition "League Phase" 2 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null
spacetime call tm-tourney-manager match_create "" 3 null

spacetime call tm-tourney-manager create_connection '{"MatchV1": 5 }' '{"MatchV1": 3 }' '{"Waiting": {}}'
spacetime call tm-tourney-manager create_connection '{"MatchV1": 5 }' '{"MatchV1": 4 }' '{"Waiting": {}}'
# spacetime call tm-tourney-manager create_connection '{"MatchV1": 6 }' '{"MatchV1": 4 }' '{"Waiting": {}}'
# spacetime call tm-tourney-manager create_connection '{"MatchV1": 6 }' '{"MatchV1": 3 }' '{"Waiting": {}}'

# Playoffs for Division 1
spacetime call tm-tourney-manager create_competition "Playoffs" 2 null
spacetime call tm-tourney-manager match_create "" 4 null
spacetime call tm-tourney-manager match_create "" 4 null

# Division 2
spacetime call tm-tourney-manager create_competition "Division 2" 1 null

# League phase matches for Division 2
spacetime call tm-tourney-manager create_competition "League Phase" 5 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null
spacetime call tm-tourney-manager match_create "" 6 null

# Playoffs for Division 2
spacetime call tm-tourney-manager create_competition "Playoffs" 5 null
spacetime call tm-tourney-manager match_create "" 7 null
spacetime call tm-tourney-manager match_create "" 7 null

spacetime call tm-tourney-manager match_create_template "Rounds Template" '{ "options": {}, "common": { "chat_time": 10, "respawn_behaviour": { "Default": {} }, "delay_before_next_map": 2000, "synchronize_players_at_map_start": true, "synchronize_players_at_round_start": true, "trust_client_simulation": true, "use_crude_extrapolation": true, "warmup_duration": { "BasedOnMedal": {} }, "warmup_timeout": { "BasedOnMedal": {} }, "warmup_number": 0, "deco_image_url_checkpoint": "", "deco_image_url_decal_sponsor_4x1": "", "deco_image_url_screen_16x1": "", "deco_image_url_screen_16x9": "", "deco_image_url_screen_8x1": "", "deco_image_url_who_am_i_url": "", "force_laps_number": { "Validation": {} }}, "mode": { "Rounds": { "finish_timeout": { "BasedOnMedal": {} }, "maps_per_match": { "One": {} }, "points_limit": { "PointsLimit": 50 }, "use_custom_points_repartition": false, "points_repartition": [10, 6, 4, 3, 2, 1], "rounds_per_map": { "Unlimited": {} }, "use_tie_breaker": true }}, "maps": { "start": 0, "map_uids": ["olsKnq_qAghcVAnEkoeUnVHFZei"] }}'\
