spacetime publish --break-clients -c -y -p tm-tourney-manager tm-tourney-manager
cargo r --manifest-path ../SpacetimeDB\Cargo.toml -p spacetimedb-cli generate --yes --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --module-path tm-tourney-manager
spacetime generate --yes --lang typescript --out-dir tm-tourney-manager-api-ts/tourney-manager --module-path tm-tourney-manager

spacetime call tm-tourney-manager create_project "Dirt Weeks 26'" "The Dirt Weeks are nice." "{""__timestamp_micros_since_unix_epoch__"": 1767132984000000 }" "{""__timestamp_micros_since_unix_epoch__"": 1767233084000000 }"

:: Template for a Discovery
spacetime call tm-tourney-manager competition_template_create "Discovery Template" 1 0
spacetime call tm-tourney-manager match_template_create "Time Attack" 2 0
spacetime call tm-tourney-manager match_template_create "Rounds" 2 0
spacetime call tm-tourney-manager match_template_create "Playoff" 2 0
spacetime call tm-tourney-manager connection_create "{""CompetitionV1"": 2 }" "{""MatchV1"": 1 }" "{""Wait"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 1 }" "{""MatchV1"": 2 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 2 }" "{""MatchV1"": 3 }" "{""Data"": {}}"


:: Template for a Match Format
spacetime call tm-tourney-manager competition_template_create "Match Template" 1 0
spacetime call tm-tourney-manager registration_template_create "Registration" 3 0
spacetime call tm-tourney-manager match_template_create "Seeding" 3 0
spacetime call tm-tourney-manager connection_create "{""RegistrationV1"": 1 }" "{""MatchV1"": 4 }" "{""Data"": {}}"
    :: Template for a Division Format
spacetime call tm-tourney-manager competition_template_create "Division Template" 3 0
spacetime call tm-tourney-manager match_template_create "Round 1: Match 1" 4 0
spacetime call tm-tourney-manager match_template_create "Round 1: Match 2" 4 0
spacetime call tm-tourney-manager match_template_create "Round 1: Match 3" 4 0
spacetime call tm-tourney-manager match_template_create "Round 1: Match 4" 4 0
spacetime call tm-tourney-manager connection_create "{""CompetitionV1"": 4 }" "{""MatchV1"": 5 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""CompetitionV1"": 4 }" "{""MatchV1"": 6 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""CompetitionV1"": 4 }" "{""MatchV1"": 7 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""CompetitionV1"": 4 }" "{""MatchV1"": 8 }" "{""Data"": {}}"
spacetime call tm-tourney-manager match_template_create "Round 2: Match 1" 4 0
spacetime call tm-tourney-manager match_template_create "Round 2: Match 2" 4 0
spacetime call tm-tourney-manager match_template_create "Round 2: Match 3" 4 0
spacetime call tm-tourney-manager match_template_create "Round 2: Match 4" 4 0
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 5 }" "{""MatchV1"": 9 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 5 }" "{""MatchV1"": 10 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 5 }" "{""MatchV1"": 11 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 5 }" "{""MatchV1"": 12 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 6 }" "{""MatchV1"": 9 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 6 }" "{""MatchV1"": 10 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 6 }" "{""MatchV1"": 11 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 6 }" "{""MatchV1"": 12 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 7 }" "{""MatchV1"": 9 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 7 }" "{""MatchV1"": 10 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 7 }" "{""MatchV1"": 11 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 7 }" "{""MatchV1"": 12 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 8 }" "{""MatchV1"": 9 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 8 }" "{""MatchV1"": 10 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 8 }" "{""MatchV1"": 11 }" "{""Data"": {}}"
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 8 }" "{""MatchV1"": 12 }" "{""Data"": {}}"

    :: Template for a Division Format Finished
spacetime call tm-tourney-manager competition_template_create "Division 1" 3 4
spacetime call tm-tourney-manager competition_template_create "Division 2" 3 4
spacetime call tm-tourney-manager competition_template_create "Division 3" 3 4
spacetime call tm-tourney-manager competition_template_create "Division 4" 3 4
:: Yoink first 16
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 4 }" "{""CompetitionV1"": 5 }" "{""Data"": {}}"
:: Yoink second 16
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 4 }" "{""CompetitionV1"": 6 }" "{""Data"": {}}"
:: Yoink third 16
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 4 }" "{""CompetitionV1"": 7 }" "{""Data"": {}}"
:: Yoink fourth 16
spacetime call tm-tourney-manager connection_create "{""MatchV1"": 4 }" "{""CompetitionV1"": 8 }" "{""Data"": {}}"

:: Template end

:: Make the actual format.
spacetime call tm-tourney-manager schedule_create "Start Discovery 1" 1 0
spacetime call tm-tourney-manager competition_create "Discovery 1" 1 2
spacetime call tm-tourney-manager competition_create "Discovery 2" 1 2
spacetime call tm-tourney-manager competition_create "Matches: Week 2" 1 3
spacetime call tm-tourney-manager competition_create "Discovery 3" 1 2
