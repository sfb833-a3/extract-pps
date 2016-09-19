extern crate conllx;
extern crate petgraph;

mod cmd;
pub use cmd::{or_exit, or_stdin, or_stdout};

mod graph;
pub use graph::{AncestorTokens, DependencyGraph, DependencyEdge, DependencyNode, PrecedingTokens,
                ancestor_tokens, first_matching_edge, preceding_tokens, sentence_to_graph};

mod macros;
