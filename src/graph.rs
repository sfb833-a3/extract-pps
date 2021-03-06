use conllx::{Sentence, Token};

use petgraph::{Directed, EdgeDirection, Graph};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;

#[derive(Debug, Eq, PartialEq)]
pub enum DependencyEdge<'a> {
    Relation(Option<&'a str>),
    Precedence,
}

#[derive(Debug)]
pub struct DependencyNode<'a> {
    pub token: &'a Token,
    pub offset: usize,
}

pub type DependencyGraph<'a> = Graph<DependencyNode<'a>, DependencyEdge<'a>, Directed>;

pub fn sentence_to_graph(sentence: &Sentence, projective: bool) -> DependencyGraph {
    let mut g = Graph::new();

    let nodes: Vec<_> = sentence
        .iter()
        .enumerate()
        .map(|(offset, token)| {
            g.add_node(DependencyNode {
                token: token,
                offset: offset,
            })
        })
        .collect();

    for (idx, token) in sentence.iter().enumerate() {
        if idx > 0 {
            g.add_edge(nodes[idx - 1], nodes[idx], DependencyEdge::Precedence);
        }

        let head = if projective {
            token.p_head()
        } else {
            token.head()
        };

        let rel = if projective {
            token.p_head_rel()
        } else {
            token.head_rel()
        };

        if let Some(head) = head {
            if head != 0 {
                g.add_edge(nodes[head - 1], nodes[idx], DependencyEdge::Relation(rel));
            }
        }
    }

    g
}

pub fn first_matching_edge<F>(
    graph: &DependencyGraph,
    index: NodeIndex,
    direction: EdgeDirection,
    predicate: F,
) -> Option<NodeIndex>
where
    F: Fn(&DependencyEdge) -> bool,
{
    graph
        .edges_directed(index, direction)
        .find(|edge_ref| predicate(edge_ref.weight()))
        .map(|edge_ref| edge_ref.target())
}

pub enum Direction {
    Preceeding,
    Succeeding,
}

pub fn adjacent_tokens<'a>(
    graph: &'a DependencyGraph<'a>,
    index: NodeIndex,
    direction: Direction,
) -> AdjacentTokens<'a> {
    AdjacentTokens {
        graph: graph,
        current: index,
        direction: direction,
    }
}

pub struct AdjacentTokens<'a> {
    graph: &'a DependencyGraph<'a>,
    current: NodeIndex,
    direction: Direction,
}

impl<'a> Iterator for AdjacentTokens<'a> {
    type Item = NodeIndex;

    fn next(&mut self) -> Option<Self::Item> {
        let direction = match self.direction {
            Direction::Preceeding => EdgeDirection::Incoming,
            Direction::Succeeding => EdgeDirection::Outgoing,
        };

        match first_matching_edge(self.graph, self.current, direction, |e| {
            *e == DependencyEdge::Precedence
        }) {
            Some(idx) => {
                self.current = idx;
                Some(idx)
            }
            None => None,
        }
    }
}

pub fn ancestor_tokens<'a>(graph: &'a DependencyGraph<'a>, index: NodeIndex) -> AncestorTokens<'a> {
    AncestorTokens {
        graph: graph,
        current: index,
    }
}

pub struct AncestorTokens<'a> {
    graph: &'a DependencyGraph<'a>,
    current: NodeIndex,
}

impl<'a> Iterator for AncestorTokens<'a> {
    type Item = NodeIndex;

    fn next(&mut self) -> Option<Self::Item> {
        match first_matching_edge(
            self.graph,
            self.current,
            EdgeDirection::Incoming,
            is_relation,
        ) {
            Some(idx) => {
                self.current = idx;
                Some(idx)
            }
            None => None,
        }
    }
}

fn is_relation(e: &DependencyEdge) -> bool {
    match *e {
        DependencyEdge::Relation(_) => true,
        DependencyEdge::Precedence => false,
    }
}
