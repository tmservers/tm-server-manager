spacetime delete tm-tourney-manager
spacetime publish -p tm-tourney-manager tm-tourney-manager
spacetime generate --lang rust --out-dir tm-tourney-manager-api-rs/src/generated --project-path tm-tourney-manager
spacetime generate --lang typescript --out-dir tm-tourney-manager-api-ts/src/gen --project-path tm-tourney-manager
spacetime call tm-tourney-manager create_tournament TestTourney
spacetime call tm-tourney-manager add_event "Discovery#1" [1759572000000000] 1 null
spacetime call tm-tourney-manager add_stage "TimeAttack" 1 null 
spacetime call tm-tourney-manager provision_match 1 null false
spacetime sql tm-tourney-manager "SELECT * FROM tournament"
spacetime sql tm-tourney-manager "SELECT * FROM tournament_event"
spacetime sql tm-tourney-manager "SELECT * FROM event_stage"
spacetime sql tm-tourney-manager "SELECT * FROM stage_match"