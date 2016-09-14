use conllx::{Sentence, Token};

use petgraph::{Directed, Graph};

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
