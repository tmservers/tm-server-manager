# Trackmania Tournament Manager (WIP)
The Goal of this project is to provide an unified backend for organizing all sort of trackmania tournaments.
Concretly it is implemented as a spacetimedb module allowing self-hosting or relying on a centrally hosted instance on spacetimes "maincloud".
This has a few advantages:
1. Unique identities for users and servers through trackmanias authentication.
2. Ability to generate a typed interface for multiple languages through spacetime.
3. Everything happening in matches gets recorded automatically and can be reconstructed.
4. Live updating weboscket based api for custom tournament frontends. 

## Architecture
First let's discuss the architecture of the backend/database to give you an idea whats possible. 
At the core the tourney-manager allows you to define 4 things:
- Tournament: Self explaining xdd. Has multiple events an owner and optionally additional organizers.
- Event: Something which can be considered as one unit/playday and has dependencies and rules for advancing e.g. a eliminationi bracket. Consists of multiple stages. 
- Stage: A parallelizable point in time at the event with multiple matches.
- Match: A concrete instantiation where players compete.

## Project Structure
- `tm-server-types`: Provides type abstractions over GBX Remote 2 for use by all other crates or standalone.
- `tm-server-client`: General purpose GBX Remote 2 protocol iplementation. Used to interact with a Trackmania server over xml-rpc.
- `tm-server-bridge`: Implements a so called "sidecar" for spacetimedb taking the role of a "trackmania server as a db client". That means it subscribes to events from the tourney manager instance to synchronize the state and control the associated tm-server.
- `tm-tourney-manager`: SpacetimeDB module to host and configure Trackmania tournaments in a flexible and as unopinionated interface as possible. 
- `tm-tourney-manager-api-{ts|rs|cs}`: Houses the generated types from tm-tourney-manager in its own package to have a strong versioned dependency for clients developed for the project.