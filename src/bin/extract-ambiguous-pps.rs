extern crate conllx;
extern crate extract_pps;
extern crate getopts;
extern crate petgraph;
#[macro_use]
extern crate lazy_static;

use std::collections::HashSet;
use std::process;
use std::env::args;

use conllx::{Features, Token};
use extract_pps::{DependencyGraph, DependencyEdge, DependencyNode, or_exit, or_stdin, sentence_to_graph};
use getopts::Options;
use petgraph::EdgeDirection;
use petgraph::graph::NodeIndex;

static PP_NOUN: &'static str = "PN";

static PP_RELATION: &'static str = "PP";

static AUXILIARY_RELATION: &'static str = "AUX";

static TOPO_FIELD_FEATURE: &'static str = "tf";

static TOPO_MIDDLE_FIELD: &'static str = "MF";

static FINITE_VERB: &'static str = "VVFIN";

static FINITE_AUXILIARY: &'static str = "VAFIN";

static FINITE_MODAL: &'static str = "VMFIN";

lazy_static! {
    static ref HEAD_TAGS: HashSet<&'static str> = {
        let mut tags = HashSet::new();
        tags.insert("NN");
        tags.insert("NE");
        tags.insert(FINITE_VERB);
        tags.insert(FINITE_AUXILIARY);
        tags
    };

    static ref FINITE_VERB_TAGS: HashSet<&'static str> = {
        let mut tags = HashSet::new();
        tags.insert(FINITE_VERB);
        tags.insert(FINITE_AUXILIARY);
        tags.insert(FINITE_MODAL);
        tags
    };
}

macro_rules! ok_or_continue {
    ($expr:expr) => (match $expr {
        Some(val) => val,
        None => continue,
    })
}

macro_rules! stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options] [INPUT_FILE] [OUTPUT_FILE]", program);
    print!("{}", opts.usage(&brief));
}

fn extract_form(token: &Token, lemma: bool) -> Option<&str> {
    if lemma { token.lemma() } else { token.form() }
}

fn main() {
    let args: Vec<String> = args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("h", "help", "print this help menu");
    opts.optflag("l", "lemma", "use lemmas instead of forms");
    let matches = or_exit(opts.parse(&args[1..]));

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }

    if matches.free.len() > 2 {
        print_usage(&program, opts);
        process::exit(1);
    }

    // Read CoNNL-X from stdin or file.
    let input = or_stdin(matches.free.get(0));
    let reader = conllx::Reader::new(or_exit(input.buf_read()));

    for sentence in reader.sentences() {
        let sentence = or_exit(sentence);
        let graph = sentence_to_graph(&sentence, false);
        print_ambiguous_pps(&graph, matches.opt_present("l"))
    }
}

fn print_ambiguous_pps(graph: &DependencyGraph, lemma: bool) {
    'pp: for edge in graph.raw_edges() {
        // Find PPs in the graph
        if edge.weight != DependencyEdge::Relation(Some(PP_RELATION)) {
            continue;
        }

        let head_node = &graph[edge.source()];
        let head = graph[edge.source()].token;
        let head_pos = ok_or_continue!(head.pos());
        
        // Skip PPs with heads that we are not interested in
        if !HEAD_TAGS.contains(head_pos) {
            continue;
        }

        let pp_node = &graph[edge.target()];

        let pp_field = ok_or_continue!(feature_value(pp_node.token, TOPO_FIELD_FEATURE));

        if pp_field != TOPO_MIDDLE_FIELD {
            continue;
        }

        let pn_rels = find_matching_edges(graph, edge.target(), EdgeDirection::Outgoing,
            DependencyEdge::Relation(Some(PP_NOUN)));

        if pn_rels.is_empty() {
            // sind davon überzeugt
            continue;
        }

        let dep_n = graph[pn_rels[0]].token;

        let dep_form = ok_or_continue!(extract_form(pp_node.token, lemma));
        let dep_n_form = ok_or_continue!(extract_form(dep_n, lemma));

        let dep_pos = ok_or_continue!(pp_node.token.pos());
        let dep_n_pos = ok_or_continue!(dep_n.pos());

        let mut competition = ok_or_continue!(find_competition(graph, edge.target(), edge.source()));

        // Don't print when there is no ambiguity.
        if competition.is_empty() {
            continue;
        }

        // Add the gold annotation.
        competition.insert(0, head_node);

        // TODO: pos noun
        print!("{} {} {} {}", dep_form, dep_pos, dep_n_form, dep_n_pos);
        for candidate in competition {
            let token = candidate.token;
            print!(" {} {} {}",
                   ok_or_continue!(extract_form(&token, lemma)),
                   ok_or_continue!(token.pos()),
                   candidate.offset as isize - pp_node.offset as isize);
        }

        println!("");
    }
}

fn find_competition<'a>(graph: &'a DependencyGraph<'a>, p_idx: NodeIndex, head_idx: NodeIndex) -> Option<Vec<&'a DependencyNode<'a>>> {
    let mut candidates = Vec::new();
    let mut current = p_idx;
        loop {

            let preceding = find_matching_edges(graph, current, EdgeDirection::Incoming, DependencyEdge::Precedence);

            if preceding.len() > 1 {
                panic!("Multiple immediately preceding tokens, should not happen.")
            }

            // When there is no left bracket, skip this PP.
            // E.g.: Die gefahr für eine Trinkerin , vom partner Verlassen zu werden , [...]
            if preceding.len() == 0 {
                return None
            }

            let preceding = preceding[0];

            let node = &graph[preceding];

            let pos = node.token.pos().unwrap();
            if FINITE_VERB_TAGS.contains(pos) {
                let verb_idx = resolve_verb(graph, preceding);
                
                if verb_idx != head_idx {
                    candidates.push(&graph[verb_idx]);
                }

                // We should be in the left bracket now...
                break;
            } else {
                let token_tf = ok_or_continue!(feature_value(node.token, TOPO_FIELD_FEATURE));

                // Bail out if we have a C-feld.
                if token_tf == "C" {
                    return None
                }

                if preceding != head_idx && HEAD_TAGS.contains(pos) {
                    candidates.push(node);
                }

                // TODO: change to: if field is LK, break.
                if pos == "VVFIN" {
                    break;
                }
            }


            current = preceding;
        }

        Some(candidates)
}

fn resolve_verb(graph: &DependencyGraph, verb: NodeIndex) -> NodeIndex {
    // Look for non-aux.
    let non_finites = find_matching_edges(graph, verb, EdgeDirection::Outgoing,
        DependencyEdge::Relation(Some(AUXILIARY_RELATION)));
    
    if non_finites.len() == 0 {
        return verb
    }

    resolve_verb(graph, non_finites[0])
}

fn find_matching_edges(graph: &DependencyGraph,
                       index: NodeIndex,
                       direction: EdgeDirection,
                       weight: DependencyEdge)
                       -> Vec<NodeIndex> {
    graph.edges_directed(index, direction)
        .filter(|&(_, e)| *e == weight)
        .map(|(idx, _)| idx)
        .collect()

}

fn feature_value(token: &Token, feature: &str) -> Option<String> {
    token.features().map(Features::as_map).and_then(|mut f| f.remove(feature)).and_then(|v| v)
}
