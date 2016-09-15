extern crate conllx;

extern crate extract_pps;

extern crate getopts;

#[macro_use]
extern crate maplit;

extern crate petgraph;

#[macro_use]
extern crate lazy_static;

use std::collections::HashSet;
use std::io::Write;
use std::process;
use std::env::args;

use conllx::{Features, Token};
use extract_pps::*;
use getopts::Options;
use petgraph::EdgeDirection;
use petgraph::graph::NodeIndex;

static PREP_COMPL_RELATION: &'static str = "PN";

static PP_RELATION: &'static str = "PP";

static AUXILIARY_RELATION: &'static str = "AUX";

static TOPO_FIELD_FEATURE: &'static str = "tf";

static TOPO_MIDDLE_FIELD: &'static str = "MF";

static NAMED_ENTITY_TAG: &'static str = "NE";

static NOUN_TAG: &'static str = "NN";

static FINITE_VERB_TAG: &'static str = "VVFIN";

static FINITE_AUXILIARY_TAG: &'static str = "VAFIN";

static FINITE_MODAL_TAG: &'static str = "VMFIN";

lazy_static! {
    static ref HEAD_TAGS: HashSet<&'static str> = hashset!{
        NOUN_TAG,
        NAMED_ENTITY_TAG,
        FINITE_VERB_TAG,
        FINITE_AUXILIARY_TAG
    };

    static ref FINITE_VERB_TAGS: HashSet<&'static str> = hashset!{
        FINITE_VERB_TAG,
        FINITE_AUXILIARY_TAG,
        FINITE_MODAL_TAG
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

    let output = or_stdout(matches.free.get(1));
    let mut writer = or_exit(output.buf_write());

    for sentence in reader.sentences() {
        let sentence = or_exit(sentence);
        let graph = sentence_to_graph(&sentence, false);
        print_ambiguous_pps(&mut writer, &graph, matches.opt_present("l"))
    }
}

fn print_ambiguous_pps(writer: &mut Write, graph: &DependencyGraph, lemma: bool) {
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

        // Skip PPs that are not in the middle field.
        if pp_field != TOPO_MIDDLE_FIELD {
            continue;
        }

        let pn_rel = ok_or_continue!(first_matching_edge(graph, edge.target(),
            EdgeDirection::Outgoing, DependencyEdge::Relation(Some(PREP_COMPL_RELATION))));

        let dep_n = graph[pn_rel].token;

        let dep_form = ok_or_continue!(extract_form(pp_node.token, lemma));
        let dep_n_form = ok_or_continue!(extract_form(dep_n, lemma));

        let dep_pos = ok_or_continue!(pp_node.token.pos());
        let dep_n_pos = ok_or_continue!(dep_n.pos());

        let mut competition =
            ok_or_continue!(find_competition(graph, edge.target(), edge.source()));

        // Don't print when there is no ambiguity.
        if competition.is_empty() {
            continue;
        }

        // Add the gold annotation.
        competition.insert(0, head_node);

        // TODO: pos noun
        or_exit(write!(writer, "{} {} {} {}", dep_form, dep_pos, dep_n_form, dep_n_pos));
        for candidate in competition {
            let token = candidate.token;
            or_exit(write!(writer, " {} {} {}",
                   ok_or_continue!(extract_form(&token, lemma)),
                   ok_or_continue!(token.pos()),
                   candidate.offset as isize - pp_node.offset as isize));
        }

        or_exit(writeln!(writer, ""));
    }
}

fn find_competition<'a>(graph: &'a DependencyGraph<'a>,
                        p_idx: NodeIndex,
                        head_idx: NodeIndex)
                        -> Option<Vec<&'a DependencyNode<'a>>> {
    let mut candidates = Vec::new();

    for idx in preceding_tokens(graph, p_idx) {
        let node = &graph[idx];
        let pos = node.token.pos().unwrap();

        if FINITE_VERB_TAGS.contains(pos) {
            let verb_idx = resolve_verb(graph, idx);

            if verb_idx != head_idx {
                candidates.push(&graph[verb_idx]);
            }

            return Some(candidates);

        } else {
            let token_tf = ok_or_continue!(feature_value(node.token, TOPO_FIELD_FEATURE));

            // Bail out if we have a C-feld.
            if token_tf == "C" {
                return None;
            }

            if idx != head_idx && HEAD_TAGS.contains(pos) {
                candidates.push(node);
            }
        }
    }

    None
}

fn resolve_verb(graph: &DependencyGraph, verb: NodeIndex) -> NodeIndex {
    // Look for non-aux.
    match first_matching_edge(graph,
                              verb,
                              EdgeDirection::Outgoing,
                              DependencyEdge::Relation(Some(AUXILIARY_RELATION))) {
        Some(idx) => resolve_verb(graph, idx),
        None => verb,
    }
}

fn feature_value(token: &Token, feature: &str) -> Option<String> {
    token.features().map(Features::as_map).and_then(|mut f| f.remove(feature)).and_then(|v| v)
}
