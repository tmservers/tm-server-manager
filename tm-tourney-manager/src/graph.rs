/* #[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct StableGraph<N, E, Ix = DefaultIx> {
    g: Graph<Option<N>, Option<E>, Ix>,
    node_count: u32,
    edge_count: u32,

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

pub type NodeIndex = u32;
pub type EdgeIndex = u32;

/// The error type for fallible `Graph` & `StableGraph` operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphError {
    /// The Graph is at the maximum number of nodes for its index.
    NodeIxLimit,

    /// The Graph is at the maximum number of edges for its index.
    EdgeIxLimit,

    /// The node with the specified index is missing from the graph.
    NodeMissed(usize),

    /// Node indices out of bounds.
    NodeOutBounds,
}

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
#[derive(Debug, Default)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct Competitions {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}
impl Competitions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_add_competition(&mut self, kind: CompetitionKind) -> NodeIndex {
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            weight: kind,
            next: StartEnd {
                start: u32::MAX,
                end: u32::MAX,
            },
        });
        id
    }

    pub fn try_add_dependency(
        &mut self,
        start: NodeIndex,
        end: NodeIndex,
    ) -> Result<EdgeIndex, GraphError> {
        let edge_idx = self.edges.len() as u32;
        /*  if !(EdgeIndex::MAX.index() == !0 || EdgeIndex::MAX != edge_idx) {
            return Err(GraphError::EdgeIxLimit);
        } */

        let mut edge = Edge {
            node: StartEnd { start, end },
            next: StartEnd {
                start: EdgeIndex::MAX,
                end: EdgeIndex::MAX,
            },
        };
        /* match index_twice(&mut self.nodes, start.index(), end.index()) {
            Pair::None => return Err(GraphError::NodeOutBounds),
            Pair::One(an) => {
                edge.next = an.next;
                an.next[0] = edge_idx;
                an.next[1] = edge_idx;
            }
            Pair::Both(an, bn) => {
                // a and b are different indices
                edge.next = [an.next[0], bn.next[1]];
                an.next[0] = edge_idx;
                bn.next[1] = edge_idx;
            }
        } */
        self.edges.push(edge);
        Ok(edge_idx)
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum CompetitionKind {
    MatchV1(u32),
    CompetitionV1(u32),
    MapMonitorV1(u32),
    ServerV1(u32),
}

// How to handle the recursion when scheduling??
