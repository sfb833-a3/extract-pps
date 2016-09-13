extern crate conllx;
extern crate extract_pps;
extern crate getopts;
extern crate petgraph;

use std::process;
use std::env::args;

use conllx::{Features, Token};
use extract_pps::{Weight, or_exit, or_stdin, sentence_to_graph};
use getopts::Options;
use petgraph::{EdgeDirection, Directed, Graph};

static PP_NOUN: &'static str = "PN";

static PP_RELATION: &'static str = "PP";

static TOPO_FIELD_FEATURE: &'static str = "tf";

static TOPO_MIDDLE_FIELD: &'static str = "MF";

macro_rules! ok_or_continue {
    ($expr:expr) => (match $expr {
        Some(val) => val,
        None => continue,
    })
}

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

fn print_ambiguous_pps(graph: &Graph<&Token, Weight, Directed>, lemma: bool) {
    'pp: for edge in graph.raw_edges() {
        // Find PPs in the graph
        if edge.weight != Weight::Relation(Some(PP_RELATION)) {
            continue;
        }

        let head = graph[edge.source()];
        let dep = graph[edge.target()];

        let pp_field = ok_or_continue!(feature_value(dep, TOPO_FIELD_FEATURE));

        if pp_field == TOPO_MIDDLE_FIELD {
            let pn_rels: Vec<_> = graph.edges_directed(edge.target(), EdgeDirection::Outgoing)
                .filter(|&(_, weight)| *weight == Weight::Relation(Some(PP_NOUN)))
                .collect();

            if pn_rels.is_empty() {
                continue;
            }

            let dep_n = graph[pn_rels[0].0];

            let head_form = ok_or_continue!(extract_form(head, lemma));
            let dep_form = ok_or_continue!(extract_form(dep, lemma));
            let dep_n_form = ok_or_continue!(extract_form(dep_n, lemma));

            let head_pos = ok_or_continue!(head.pos());
            let dep_pos = ok_or_continue!(dep.pos());

            // Find attachment candidates
            let mut current = edge.target();
            let mut candidates = Vec::new();

            loop {

                let preceding: Vec<_> = graph.edges_directed(current, EdgeDirection::Incoming)
                    .filter(|&(_, e)| *e == Weight::Precedence)
                    .map(|(idx, _)| idx)
                    .collect();

                if preceding.len() > 1 {
                    panic!("Multiple immediately preceding tokens, should not happen.")
                }

                // Reached the beginning of the sentence. We'd expect a
                // finite verb, since we are in the middle field.
                if preceding.len() == 0 {
                    break;
                }

                let token = graph[preceding[0]];

                let pos = token.pos().unwrap();
                if pos == "VAFIN" || pos == "VMFIN" {
                    // Look for non-aux.
                    let non_finites: Vec<_> =
                        graph.edges_directed(preceding[0], EdgeDirection::Outgoing)
                            .filter(|&(_, e)| *e == Weight::Relation(Some("AUX")))
                            .map(|(idx, _)| idx)
                            .collect();

                    for non_finite_idx in non_finites {
                        let non_finite = graph[non_finite_idx];
                        if non_finite != head {
                            candidates.push(non_finite);
                        }
                    }

                    break;
                } else {
                    let token_tf = ok_or_continue!(feature_value(token, TOPO_FIELD_FEATURE));
                    if token_tf == "C" {
                        continue 'pp;
                    }

                    if token != head && (pos == "NN" || pos == "VVFIN" || pos == "NE") {
                        candidates.push(token);
                    }

                    if pos == "VVFIN" {
                        break;
                    }
                }


                current = preceding[0];
            }

            // Don't print when there is no ambiguity.
            if candidates.is_empty() {
                continue;
            }

            print!("{} {} {} {}", dep_form, dep_n_form, head_form, head_pos);
            for candidate in candidates {
                print!(" {} {}",
                       ok_or_continue!(extract_form(&candidate, lemma)),
                       ok_or_continue!(candidate.pos()));
            }
            println!("");


            // let dep_form = ok_or_continue!(extract_form(dep, lemma));
            // let dep_n_form = ok_or_continue!(extract_form(dep_n, lemma));

            // let head_pos = ok_or_continue!(head.pos());
            // let dep_pos = ok_or_continue!(dep.pos());

            // let head_field = ok_or_continue!(feature_value(head, TOPO_FIELD_FEATURE));

            // println!("{} {} {} {} {} {} {}",
            //          head_form,
            //          head_pos,
            //          head_field,
            //          dep_form,
            //          dep_pos,
            //          pp_field,
            //          dep_n_form);
        }
    }
}

fn feature_value(token: &Token, feature: &str) -> Option<String> {
    token.features().map(Features::as_map).and_then(|mut f| f.remove(feature)).and_then(|v| v)
}
