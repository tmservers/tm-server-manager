# Connections

Connections allow definig relationships between nodes.
Like nodes they also have a configuring state.

A node can have multiple input and output connections but the graph always stays acyclic.

Multiple types of connections exist:
- Wait: 
- Data
- Action

## Automatic (Implicit) start.
If all incoming wait and data connections of a node are ready, the node does an _implicit_ action depending on the type:
- Match: Open up server with the pre match config.
- Registration: Open up registration.

## Explicit start.
If you need to manually control the status of a node you can use a action connection.
This allows you to define an action which gets executed as soon as the origin node resolves.

