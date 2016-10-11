extern crate conllx;
extern crate petgraph;

#[macro_use]
mod macros;

mod cmd;
pub use cmd::{or_exit, or_stdin, or_stdout};

mod graph;
pub use graph::{AncestorTokens, AdjacentTokens, DependencyGraph, DependencyEdge, DependencyNode,
                Direction, adjacent_tokens, ancestor_tokens, first_matching_edge,
                sentence_to_graph};
