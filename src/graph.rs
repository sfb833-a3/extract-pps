use conllx::{Sentence, Token};

use petgraph::{Directed, Graph};

#[derive(Debug, Eq, PartialEq)]
pub enum Weight<'a> {
    Relation(Option<&'a str>),
    Precedence,
}

pub fn sentence_to_graph(sentence: &Sentence, projective: bool) -> Graph<&Token, Weight, Directed> {
    let mut g = Graph::new();

    let nodes: Vec<_> = sentence.iter().map(|token| g.add_node(token)).collect();

    for (idx, token) in sentence.iter().enumerate() {
        if idx > 0 {
            g.add_edge(nodes[idx - 1], nodes[idx], Weight::Precedence);
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
                g.add_edge(nodes[head - 1], nodes[idx], Weight::Relation(rel));
            }
        }
    }

    g
}
