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

    //which invariants should the thing provide?
    //How would i batch multiple matches into a custom leaderboard.

    // Can there be multiple output nodes?
    //-> i think not
    // Only one node in the whole competition can be marked as a output.
    //-> This should then be a member of a competition intself and likely an Option<CompetitionKind>,
    //-> I need to provide an interface where every node can give back results that are ordered and probably should operate on Entities
    //-> These entities are Players or Teams.
    //->-> A leaderboard node type could be exposed to further post process these results.
    //->-> Need a interface where we can input these results and define rules on them.

    //THE INTERFACE WOULD ENSURE THAT EVERY NODE TYPE HAS ONE LEADEARBOARD ASSOCIATED WITH IT.
    output: bool,

    // We need something where we can define the range of the input.
    // Actually not only the range but rules on how to distribute something to the ndoes
    // This would be part of the functionality of competitions.
    // There we have the problem: What is the MAX thing a Competition is allowed to handle?
    // What to do if there is less than the MAX? -> How rather how to expose it to users.

    //THere need to be node templates and compeitiion templates.
    // Maybe the competition templates should handling this somehow :thinking: but that would prevent
    // users from specifing their thing in advance when the concrete stuff is not known.
    // Maybe this would be best to handle with a competitionStage thats called generate.
    //-> Generate would then prevent things that depend on generation to be run before actually knowing the specifics.
    //-> Maybe this would also be handled by the npdes themselves idk.
    input: bool,
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

            output: false,
            input: false,
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
    MonitoringV1(u32),
    ServerV1(u32),
}

// How to handle the recursion when scheduling??
// How to handle the recursion when scheduling??
