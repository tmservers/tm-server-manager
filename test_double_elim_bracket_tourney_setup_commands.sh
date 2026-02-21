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
spacetime call tm-tourney-manager create_tournament "Double Elimination" "This is a beautiful tournament" '{"__timestamp_micros_since_unix_epoch__": 1777132984000000}' '{"__timestamp_micros_since_unix_epoch__": 1777233084000000}'

# Double elimination bracket
spacetime call tm-tourney-manager create_competition "Double Elimination Bracket" 1 null

# Upper R1
# 8 matches, Match IDs 1-8
for i in {1..8}; do
  spacetime call tm-tourney-manager match_create "Upper R1 M$i" 2 null
done

# Upper R2
# 4 matches, Match IDs 9-12
for i in {1..4}; do
  spacetime call tm-tourney-manager match_create "Upper R2 M$i" 2 null
done

# Upper R2 connections
# Winners of Upper R1 to Upper R2
# Upper R2 M1 gets winners of Upper R1 matches 1, 2, 7, 8
# Upper R2 M2 gets winners of Upper R1 matches 3, 4, 5, 6
# Upper R2 M3 gets winners of Upper R1 matches 3, 4, 5, 6
# Upper R2 M4 gets winners of Upper R1 matches 1, 2, 7, 8
for match in {9..12}; do
  if [ $match -eq 9 ] || [ $match -eq 12 ]; then
    winners=(1 2 7 8)
  else
    winners=(3 4 5 6)
  fi

  for target in {1..8}; do
    # If target is one of the winners for this match, create Data connection
    if [[ " ${winners[@]} " =~ " ${target} " ]]; then
      spacetime call tm-tourney-manager create_connection \
        "{\"MatchV1\": $match }" \
        "{\"MatchV1\": $target }" \
        '{"Data": {}}'
    # Otherwise, create Waiting connection
    else
      spacetime call tm-tourney-manager create_connection \
        "{\"MatchV1\": $match }" \
        "{\"MatchV1\": $target }" \
        '{"Waiting": {}}'
    fi
  done
done

# Lower R1
# 4 matches, Match IDs 13-16
for i in {1..4}; do
  spacetime call tm-tourney-manager match_create "Lower R1 M$i" 2 null
done

# Lower R1 connections
# Losers of Upper R1 to Lower R1
# Lower R1 M1 gets losers of Upper R1 matches 1, 2, 7, 8
# Lower R1 M2 match gets losers of Upper R1 matches 3, 4, 5, 6
# Lower R1 M3 match gets losers of Upper R1 matches 3, 4, 5, 6
# Lower R1 M4 match gets losers of Upper R1 matches 1, 2, 7, 8
for match in {13..16}; do
  if [ $match -eq 13 ] || [ $match -eq 16 ]; then
    losers=(1 2 7 8)
  else
    losers=(3 4 5 6)
  fi

  for target in {1..8}; do
    # If target is one of the losers for this match, create Data connection
    if [[ " ${losers[@]} " =~ " ${target} " ]]; then
      spacetime call tm-tourney-manager create_connection \
        "{\"MatchV1\": $match }" \
        "{\"MatchV1\": $target }" \
        '{"Data": {}}'
    # Otherwise, create Waiting connection
    else
      spacetime call tm-tourney-manager create_connection \
        "{\"MatchV1\": $match }" \
        "{\"MatchV1\": $target }" \
        '{"Waiting": {}}'
    fi

  done
done

# Lower R2
# 4 matches, Match IDs 17-20
for i in {1..4}; do
  spacetime call tm-tourney-manager match_create "Lower R2 M$i" 2 null
done

# Lower R2 connections
# Winners of Lower R1 to Lower R2, Losers of Upper R2 to Lower R2
# Lower R2 M1 gets winners of Lower R1 matches 1 and 2, and losers of Upper R2 matches 3 and 4
# Lower R2 M2 gets winners of Lower R1 matches 1 and 2, and losers of Upper R2 matches 3 and 4
# Lower R2 M3 gets winners of Lower R1 matches 3 and 4, and losers of Upper R2 matches 1 and 2
# Lower R2 M4 gets winners of Lower R1 matches 3 and 4, and losers of Upper R2 matches 1 and 2
for match in {17..20}; do
  if [ $match -eq 17 ] || [ $match -eq 18 ]; then
    lower_winners=(13 14)
    upper_losers=(11 12)
  else
    lower_winners=(15 16)
    upper_losers=(9 10)
  fi

  for target in {9..16}; do
    # If target is one of the winners or losers for this match, create Data connection
    if [[ " ${lower_winners[@]} " =~ " ${target} " ]] || [[ " ${upper_losers[@]} " =~ " ${target} " ]]; then
      spacetime call tm-tourney-manager create_connection \
        "{\"MatchV1\": $match }" \
        "{\"MatchV1\": $target }" \
        '{"Data": {}}'
    # Otherwise, create Waiting connection
    else
      spacetime call tm-tourney-manager create_connection \
        "{\"MatchV1\": $match }" \
        "{\"MatchV1\": $target }" \
        '{"Waiting": {}}'
    fi

  done
done

# Upper R3
# 2 matches, Match IDs 21-22
for i in {1..2}; do
  spacetime call tm-tourney-manager match_create "Upper R3 M$i" 2 null
done

# Upper R3 connections
# Winners of Upper R2 to Upper R3
# Upper R3 M1 gets winners of Upper R2 matches 1, 2, 3, 4
# Upper R3 M2 gets winners of Upper R2 matches 1, 2, 3, 4
for match in {21..22}; do
  for target in {9..12}; do
    spacetime call tm-tourney-manager create_connection \
      "{\"MatchV1\": $match }" \
      "{\"MatchV1\": $target }" \
      '{"Data": {}}'
  done
done

# Upper round 3 also has to wait on Lower R2
for match in {21..22}; do
  for target in {17..20}; do
    spacetime call tm-tourney-manager create_connection \
      "{\"MatchV1\": $match }" \
      "{\"MatchV1\": $target }" \
      '{"Waiting": {}}'
  done
done

# Lower R3
# 2 matches, Match IDs 23-24
for i in {1..2}; do
  spacetime call tm-tourney-manager match_create "Lower R3 M$i" 2 null
done

# Lower R3 connections
# Winners of Lower R2 to Lower R3
# Lower R3 M1 gets winners of Lower R2 matches 1, 2, 3, 4
# Lower R3 M2 gets winners of Lower R2 matches 1, 2, 3, 4
for match in {23..24}; do
  for target in {17..20}; do
    spacetime call tm-tourney-manager create_connection \
      "{\"MatchV1\": $match }" \
      "{\"MatchV1\": $target }" \
      '{"Data": {}}'
  done
done

# Lower R4
# 2 matches, Match IDs 25-26
for i in {1..2}; do
  spacetime call tm-tourney-manager match_create "Lower R4 M$i" 2 null
done

# Lower R4 connections
# Winners of Lower R3 to Lower R4, Losers of Upper R3 to Lower R4
# Lower R4 M1 gets winners of Lower R3 match 1, 2 and losers of Upper R3 match 1, 2
# Lower R4 M2 gets winners of Lower R3 match 1, 2 and losers of Upper R3 match 1, 2
for match in {25..26}; do
  for target in {21..24}; do
    spacetime call tm-tourney-manager create_connection \
      "{\"MatchV1\": $match }" \
      "{\"MatchV1\": $target }" \
      '{"Data": {}}'
  done
done

# Upper Final
# 1 match, Match ID 27
spacetime call tm-tourney-manager match_create "Upper Final" 2 null

# Upper Final connections
# Winners of Upper R3 to Upper Final
# Upper Final gets winners of Upper R3 match 1, 2
for target in {21..22}; do
  spacetime call tm-tourney-manager create_connection \
    '{"MatchV1": 27 }' \
    "{\"MatchV1\": $target }" \
    '{"Data": {}}'
done

# Upper Final also has to wait on Lower R4
for target in {25..26}; do
  spacetime call tm-tourney-manager create_connection \
    '{"MatchV1": 27 }' \
    "{\"MatchV1\": $target }" \
    '{"Waiting": {}}'
done

# Lower Final
# 1 match, Match ID 28
spacetime call tm-tourney-manager match_create "Lower Final" 2 null

# Lower Final connections
# Winners of Lower R4 to Lower Final
# Lower Final gets winners of Lower R4 match 1, 2
for target in {25..26}; do
  spacetime call tm-tourney-manager create_connection \
    '{"MatchV1": 28 }' \
    "{\"MatchV1\": $target }" \
    '{"Data": {}}'
done

# Consolidation Final
# 1 match, Match ID 29
spacetime call tm-tourney-manager match_create "Consolidation Final" 2 null

# Consolidation Final connections
# Losers of Upper Final and Winners of Lower Final to Consolidation Final
# Consolidation Final gets loser of Upper Final and winner of Lower Final
for target in {27,28}; do
  spacetime call tm-tourney-manager create_connection \
    '{"MatchV1": 29 }' \
    "{\"MatchV1\": $target }" \
    '{"Data": {}}'
done

# Grand Final
# 1 match, Match ID 30
spacetime call tm-tourney-manager match_create "Grand Final" 2 null

# Grand Final connections
# Winners of Upper Final and Winners of Consolidation Final to Grand Final
# Grand Final gets winner of Upper Final and winner of Consolidation Final
for target in {27,29}; do
  spacetime call tm-tourney-manager create_connection \
    '{"MatchV1": 30 }' \
    "{\"MatchV1\": $target }" \
    '{"Data": {}}'
done

# Create match template
spacetime call tm-tourney-manager match_create_template "Rounds Template" '{ "options": {}, "common": { "chat_time": 10, "respawn_behaviour": { "Default": {} }, "delay_before_next_map": 2000, "synchronize_players_at_map_start": true, "synchronize_players_at_round_start": true, "trust_client_simulation": true, "use_crude_extrapolation": true, "warmup_duration": { "BasedOnMedal": {} }, "warmup_timeout": { "BasedOnMedal": {} }, "warmup_number": 0, "deco_image_url_checkpoint": "", "deco_image_url_decal_sponsor_4x1": "", "deco_image_url_screen_16x1": "", "deco_image_url_screen_16x9": "", "deco_image_url_screen_8x1": "", "deco_image_url_who_am_i_url": "", "force_laps_number": { "Validation": {} }}, "mode": { "Rounds": { "finish_timeout": { "BasedOnMedal": {} }, "maps_per_match": { "One": {} }, "points_limit": { "PointsLimit": 50 }, "use_custom_points_repartition": false, "points_repartition": [10, 6, 4, 3, 2, 1], "rounds_per_map": { "Unlimited": {} }, "use_tie_breaker": true }}, "maps": { "start": 0, "map_uids": ["olsKnq_qAghcVAnEkoeUnVHFZei"] }}'\
