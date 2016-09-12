use conllx::{Sentence, Token};

use petgraph::{Directed, Graph};

pub fn sentence_to_graph(sentence: &Sentence,
                         projective: bool)
                         -> Graph<&Token, Option<&str>, Directed> {
    let mut g = Graph::new();

    let nodes: Vec<_> = sentence.iter().map(|token| g.add_node(token)).collect();

    for (idx, token) in sentence.iter().enumerate() {
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
                g.add_edge(nodes[head - 1], nodes[idx], rel);
            }
        }
    }

    g
}
