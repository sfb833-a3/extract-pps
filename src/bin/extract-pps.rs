extern crate conllx;

#[macro_use]
extern crate syncooc;

extern crate getopts;

extern crate petgraph;

use std::io::Write;
use std::process;
use std::env::args;

use conllx::{Features, Token};
use syncooc::*;
use getopts::Options;
use petgraph::EdgeDirection;

static PP_NOUN: &'static str = "PN";

static PP_RELATION: &'static str = "PP";

static TOPO_FIELD_FEATURE: &'static str = "tf";

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

    for sentence in reader {
        let sentence = or_exit(sentence);
        let graph = sentence_to_graph(&sentence, false);
        print_pps(&mut writer, &graph, matches.opt_present("l"))
    }
}

fn print_pps(writer: &mut Write, graph: &DependencyGraph, lemma: bool) {
    for edge in graph.raw_edges() {
        if edge.weight == DependencyEdge::Relation(Some(PP_RELATION)) {
            let head = graph[edge.source()].token;
            let dep = graph[edge.target()].token;

            let preceding_tag =
                match adjacent_tokens(graph, edge.target(), Direction::Preceeding).next() {
                    Some(idx) => graph[idx].token.pos().unwrap_or("NONE"),
                    None => "NONE",
                };

            let pn_rel = ok_or_continue!(first_matching_edge(
                graph,
                edge.target(),
                EdgeDirection::Outgoing,
                |e| *e == DependencyEdge::Relation(Some(PP_NOUN)),
            ));

            let dep_n = graph[pn_rel].token;

            let head_form = ok_or_continue!(extract_form(head, lemma));
            let dep_form = ok_or_continue!(extract_form(dep, lemma));
            let dep_n_form = ok_or_continue!(extract_form(dep_n, lemma));

            let head_pos = ok_or_continue!(head.pos());
            let dep_pos = ok_or_continue!(dep.pos());

            let head_field = ok_or_continue!(feature_value(head, TOPO_FIELD_FEATURE));
            let pp_field = ok_or_continue!(feature_value(dep, TOPO_FIELD_FEATURE));

            or_exit(writeln!(
                writer,
                "{} {} {} {} {} {} {} {}",
                head_form,
                head_pos,
                head_field,
                dep_form,
                dep_pos,
                pp_field,
                dep_n_form,
                preceding_tag
            ));
        }
    }
}

fn feature_value(token: &Token, feature: &str) -> Option<String> {
    token
        .features()
        .map(Features::as_map)
        .and_then(|f| f.get(feature))
        .and_then(|v| v.clone())
}
