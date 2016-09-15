use conllx::{Sentence, Token};

use petgraph::{Directed, EdgeDirection, Graph};
use petgraph::graph::NodeIndex;

#[derive(Debug, Eq, PartialEq)]
pub enum DependencyEdge<'a> {
    Relation(Option<&'a str>),
    Precedence,
}

pub struct DependencyNode<'a> {
    pub token: &'a Token,
    pub offset: usize,
}

pub type DependencyGraph<'a> = Graph<DependencyNode<'a>, DependencyEdge<'a>, Directed>;

pub fn sentence_to_graph(sentence: &Sentence, projective: bool) -> DependencyGraph {
    let mut g = Graph::new();

    let nodes: Vec<_> = sentence.iter()
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

pub fn first_matching_edge<F>(graph: &DependencyGraph,
                           index: NodeIndex,
                           direction: EdgeDirection,
                           predicate: F)
                           -> Option<NodeIndex>
                           where F: Fn(&DependencyEdge) -> bool {
    graph.edges_directed(index, direction)
        .find(|&(_, e)| predicate(e))
        .map(|(idx, _)| idx)
}


pub fn preceding_tokens<'a>(graph: &'a DependencyGraph<'a>,
                            index: NodeIndex)
                            -> PrecedingTokens<'a> {
    PrecedingTokens {
        graph: graph,
        current: index,
    }
}

pub struct PrecedingTokens<'a> {
    graph: &'a DependencyGraph<'a>,
    current: NodeIndex,
}

impl<'a> Iterator for PrecedingTokens<'a> {
    type Item = NodeIndex;

    fn next(&mut self) -> Option<Self::Item> {
        match first_matching_edge(self.graph,
                                  self.current,
                                  EdgeDirection::Incoming,
                                  |e| *e == DependencyEdge::Precedence) {
            Some(idx) => {
                self.current = idx;
                Some(idx)
            }
            None => None,
        }
    }
}
