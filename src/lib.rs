extern crate conllx;
extern crate petgraph;

mod cmd;
pub use cmd::{or_exit, or_stdin};

mod graph;
pub use graph::{DependencyGraph, DependencyEdge, DependencyNode, PrecedingTokens,
                first_matching_edge, preceding_tokens, sentence_to_graph};
