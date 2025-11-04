# Trackmania Tournament Manager (WIP)
The Goal of this project is to provide an unified backend for organizing all sort of trackmania tournaments.
Concretly it is implemented as a spacetimedb module allowing self-hosting or relying on a centrally hosted instance on spacetimes "maincloud".
This has a few advantages:
1. Unique identities for users and servers through trackmanias authentication.
2. Ability to generate a typed interface for multiple languages through spacetime.
3. Everything happening in matches gets recorded automatically and can be reconstructed.
4. Live updating weboscket based api for custom tournament frontends. 

## Project Structure
- `tm-server-types`: Provides type abstractions over GBX Remote 2 for use by all other crates or standalone.
- `tm-server-client`: General purpose GBX Remote 2 protocol iplementation. Used to interact with a Trackmania server over xml-rpc.
- `tm-server-bridge`: Implements a so called "sidecar" for spacetimedb taking the role of a "trackmania server as a db client". That means it subscribes to events from the tourney manager instance to synchronize the state and control the associated tm-server.
- `tm-tourney-manager`: SpacetimeDB module to host and configure Trackmania tournaments in a flexible and as unopinionated interface as possible. 
- `tm-tourney-manager-api-{ts|rs|cs}`: Houses the generated types from tm-tourney-manager in its own package to have a strong versioned dependency for clients developed for the project.

# Contributing and project Governance 
Contributions are very welcome but features are best discussed in a issue beforehand to avoid dissapointment in case the design is not ideal.
The project also wont babysit contributions or tolerate obvious AI slop so please be concious about that before submitting a pull request. 
If you feel like the project should be in it's own organization, want to help maintain it or a crate in this project would be better off standalone please write me on discord or create a issue.
Im generally open for it but only if there is any demand :>   