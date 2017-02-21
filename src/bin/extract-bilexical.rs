extern crate conllx;

#[macro_use]
extern crate extract_pps;

extern crate getopts;

extern crate petgraph;

use std::io::Write;
use std::process;
use std::env::args;

use conllx::Token;
use extract_pps::*;
use getopts::Options;

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options] REL [INPUT_FILE] [OUTPUT_FILE]",
                        program);
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

    if matches.free.len() == 0 || matches.free.len() > 3 {
        print_usage(&program, opts);
        process::exit(1);
    }

    let relation = &matches.free[0];

    // Read CoNNL-X from stdin or file.
    let input = or_stdin(matches.free.get(1));
    let reader = conllx::Reader::new(or_exit(input.buf_read()));

    let output = or_stdout(matches.free.get(2));
    let mut writer = or_exit(output.buf_write());

    for sentence in reader.sentences() {
        let sentence = or_exit(sentence);
        let graph = sentence_to_graph(&sentence, false);
        print_bilex(&mut writer, &graph, relation, matches.opt_present("l"))
    }
}

fn print_bilex(writer: &mut Write, graph: &DependencyGraph, relation: &str, lemma: bool) {
    for edge in graph.raw_edges() {
        if edge.weight == DependencyEdge::Relation(Some(relation)) {
            let head = graph[edge.source()].token;
            let dep = graph[edge.target()].token;

            let head_form = ok_or_continue!(extract_form(head, lemma));
            let dep_form = ok_or_continue!(extract_form(dep, lemma));

            let head_pos = ok_or_continue!(head.pos());
            let dep_pos = ok_or_continue!(dep.pos());

            or_exit(writeln!(writer,
                             "{} {} {} {}",
                             head_form,
                             head_pos,
                             dep_form,
                             dep_pos));
        }
    }
}
