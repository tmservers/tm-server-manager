/* #[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct StableGraph<N, E, Ix = DefaultIx> {
    g: Graph<Option<N>, Option<E>, Ix>,
    node_count: u64,
    edge_count: u64,

    // node and edge free lists (both work the same way)
    //
    // free_node, if not NodeIndex::end(), points to a node index
    // that is vacant (after a deletion).
    // The free nodes form a doubly linked list using the fields Node.next[0]
    // for forward references and Node.next[1] for backwards ones.
    // The nodes are stored as EdgeIndex, and the _into_edge()/_into_node()
    // methods convert.
    // free_edge, if not EdgeIndex::end(), points to a free edge.
    // The edges only form a singly linked list using Edge.next[0] to store
    // the forward reference.
    free_node: NodeIndex,
    free_edge: EdgeIndex,
} */

/* #[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct Graph<N, E> {
    nodes: Vec<Node<N>>,
    edges: Vec<Edge<E>>,
    //ty: PhantomData<Ty>,
} */

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct Node {
    /// Associated node data.
    weight: CompetitionKind,
    /// Next edge in outgoing and incoming edge lists.
    next: StartEnd,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct Edge {
    /// Next edge in outgoing and incoming edge lists.
    next: StartEnd,
    /// Start and End node index
    node: StartEnd,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct StartEnd {
    start: u32,
    end: u32,
}

/// A Directed Acyclic Graph.
#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct Competitions {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}
impl Competitions {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum CompetitionKind {
    Match(u64),
    Competition(u64),
}
