extern crate conllx;
extern crate petgraph;

mod cmd;
pub use cmd::{or_exit, or_stdin};

mod graph;
pub use graph::{DependencyGraph, DependencyEdge, DependencyNode, sentence_to_graph};
