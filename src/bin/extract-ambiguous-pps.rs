extern crate conllx;

#[macro_use]
extern crate extract_pps;

extern crate getopts;

#[macro_use]
extern crate maplit;

extern crate petgraph;

#[macro_use]
extern crate lazy_static;

use std::collections::HashSet;
use std::collections::HashMap;
use std::io::Write;
use std::process;
use std::env::args;

use conllx::{Features, Token};
use extract_pps::*;
use getopts::Options;
use petgraph::EdgeDirection;
use petgraph::graph::NodeIndex;

#[derive(Clone,Copy)]
enum Field {
    VF,
    MF,
    NF,
}

impl Field {
    fn string_value(&self) -> &'static str {
        match *self {
            Field::VF => "VF",
            Field::MF => "MF",
            Field::NF => panic!("No NF treatment yet"),
        }
    }
}

static PREP_COMPL_RELATION: &'static str = "PN";

static PP_RELATION: &'static str = "PP";

static AUXILIARY_RELATION: &'static str = "AUX";

static TOPO_FIELD_FEATURE: &'static str = "tf";

static TOPO_RK_FIELD: &'static str = "VC";

static TOPO_C_FIELD: &'static str = "C";

static FINITE_VERB_TAG: &'static str = "VVFIN";

static FINITE_AUXILIARY_TAG: &'static str = "VAFIN";

static FINITE_MODAL_TAG: &'static str = "VMFIN";

lazy_static! {
    static ref FINITE_VERB_TAGS: HashSet<&'static str> = hashset!{
        FINITE_VERB_TAG,
        FINITE_AUXILIARY_TAG,
        FINITE_MODAL_TAG
    };

    static ref STRING_FIELD: HashMap<&'static str, Field> = hashmap!{
        "VF" => Field::VF,
        "MF" => Field::MF,
        "NF" => Field::NF
    } ;
}

fn relevant_head_tag(tag: &str) -> bool {
    tag.starts_with("N") || tag.starts_with("V")
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
    opts.optflag("a",
                 "all",
                 "extract all PPs, including PPs with no head competition");
    opts.optopt("f", "field", "field to extract from", "FIELD");
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


    let field_opt = matches.opt_str("f").unwrap_or("MF".to_owned());

    let field = match STRING_FIELD.get(field_opt.as_str()) {
        Some(field) => *field,
        None => {
            stderr!("Unknown field");
            process::exit(1);
        }
    };

    // Read CoNNL-X from stdin or file.
    let input = or_stdin(matches.free.get(0));
    let reader = conllx::Reader::new(or_exit(input.buf_read()));

    let output = or_stdout(matches.free.get(1));
    let mut writer = or_exit(output.buf_write());

    for sentence in reader.sentences() {
        let sentence = or_exit(sentence);
        let graph = sentence_to_graph(&sentence, false);
        print_ambiguous_pps(&mut writer,
                            &graph,
                            matches.opt_present("l"),
                            matches.opt_present("a"),
                            field)
    }
}

fn print_ambiguous_pps(writer: &mut Write,
                       graph: &DependencyGraph,
                       lemma: bool,
                       all: bool,
                       field: Field) {
    for edge in graph.raw_edges() {
        // Find PPs in the graph
        if edge.weight != DependencyEdge::Relation(Some(PP_RELATION)) {
            continue;
        }

        let head = graph[edge.source()].token;
        let head_pos = ok_or_continue!(head.pos());

        // Skip PPs with heads that we are not interested in
        if !relevant_head_tag(head_pos) {
            continue;
        }

        let pp_node = &graph[edge.target()];
        let pp_field = ok_or_continue!(feature_value(pp_node.token, TOPO_FIELD_FEATURE));

        if pp_field != field.string_value() {
            // Skip PPs that are not in the field that we are interested in.
            continue;
        }

        let pn_rel = ok_or_continue!(first_matching_edge(graph,
                                                         edge.target(),
                                                         EdgeDirection::Outgoing,
                                                         |e| {
                                                             *e == DependencyEdge::Relation(Some(PREP_COMPL_RELATION))
                                                         }));

        let dep_n = graph[pn_rel].token;

        let dep_form = ok_or_continue!(extract_form(pp_node.token, lemma));
        let dep_n_form = ok_or_continue!(extract_form(dep_n, lemma));

        let dep_pos = ok_or_continue!(pp_node.token.pos());
        let dep_n_pos = ok_or_continue!(dep_n.pos());

        let competition = match field {
            Field::VF => ok_or_continue!(find_competition_vf(graph, edge.target(), edge.source())),                
            Field::MF => ok_or_continue!(find_competition_mf(graph, edge.target(), edge.source())),
            Field::NF => panic!("NF handling missing"),
        };

        // Don't print when there is no ambiguity.
        if !all && competition.len() == 1 {
            continue;
        }

        // Fixme: we don't want ok_or_contiues in here, or the output should be written
        //        to a buffer first.
        or_exit(write!(writer,
                       "{} {} {} {}",
                       dep_form,
                       dep_pos,
                       dep_n_form,
                       dep_n_pos));
        for candidate in competition {
            let token = candidate.node.token;
            or_exit(write!(writer,
                           " {} {} {} {} {}",
                           ok_or_continue!(extract_form(&token, lemma)),
                           ok_or_continue!(token.pos()),
                           candidate.node.offset as isize - pp_node.offset as isize,
                           candidate.rank,
                           if candidate.head { 1 } else { 0 }));
        }

        or_exit(writeln!(writer, ""));
    }
}

struct CompetingHead<'a> {
    node: &'a DependencyNode<'a>,
    rank: isize,
    head: bool,
}

fn find_competition_vf<'a>(graph: &'a DependencyGraph<'a>,
                           p_idx: NodeIndex,
                           head_idx: NodeIndex)
                           -> Option<Vec<CompetingHead>> {
    let mut candidates = Vec::new();

    // Exclude cases where the head is left of the PP.
    // if graph[head_idx].offset < graph[p_idx].offset {
    //     return None;
    // }

    // Find left bracket
    let lk_idx = try_ok!(adjacent_tokens(graph, p_idx, Direction::Succeeding).find(|idx| {
        let node = &graph[*idx];

        match feature_value(node.token, "tf") {
            Some(field) => field == "LK",
            None => false,
        }
    }));

    let verb_idx = resolve_verb(graph, lk_idx);

    candidates.push(CompetingHead {
        node: &graph[verb_idx],
        rank: 1, // XXX
        head: verb_idx == head_idx ||
              ancestor_tokens(graph, verb_idx).find(|idx| *idx == head_idx).is_some(),
    });

    let preceding_is_noun = match adjacent_tokens(graph, p_idx, Direction::Preceeding).next() {
        Some(prec_idx) => graph[prec_idx].token.pos().unwrap().starts_with("N"),
        None => false,
    };

    let vf_tokens = adjacent_tokens(graph, p_idx, Direction::Preceeding).take_while(|idx| {
        match feature_value(&graph[*idx].token, "tf") {
            Some(field) => field == "VF" || field == "UK",
            None => false,
        }
    });

    add_tokens(graph, head_idx, vf_tokens, &mut candidates);


    // Only add MF tokens when the preceding token is not a noun...
    if !preceding_is_noun {
        // Left bracket should not contain any other material...
        let mf_tokens = adjacent_tokens(graph, lk_idx, Direction::Succeeding)
            .take_while(|idx| {
                match feature_value(&graph[*idx].token, "tf") {
                    Some(field) => field == "MF" || field == "UK",
                    None => false,
                }
            });

        add_tokens(graph, head_idx, mf_tokens, &mut candidates);
    }

    Some(candidates)
}

fn add_tokens<'a, I>(graph: &'a DependencyGraph<'a>,
                     head_idx: NodeIndex,
                     tokens: I,
                     candidates: &mut Vec<CompetingHead<'a>>)
    where I: Iterator<Item = NodeIndex>
{
    for idx in tokens {
        let node = &graph[idx];
        let pos = ok_or_break!(node.token.pos());

        let head_rank = -(candidates.len() as isize + 1);

        if relevant_head_tag(pos) {
            candidates.push(CompetingHead {
                node: node,
                rank: head_rank,
                head: head_idx == idx,
            });
        }
    }
}

fn find_competition_mf<'a>(graph: &'a DependencyGraph<'a>,
                           p_idx: NodeIndex,
                           head_idx: NodeIndex)
                           -> Option<Vec<CompetingHead>> {
    let mut candidates = Vec::new();

    for idx in adjacent_tokens(graph, p_idx, Direction::Preceeding) {
        let node = &graph[idx];
        let pos = ok_or_break!(node.token.pos());
        let tf = ok_or_break!(feature_value(node.token, TOPO_FIELD_FEATURE));

        let head_rank = -(candidates.len() as isize + 1);

        if FINITE_VERB_TAGS.contains(pos) {
            let verb_idx = resolve_verb(graph, idx);

            candidates.push(CompetingHead {
                node: &graph[verb_idx],
                rank: if verb_idx == idx { head_rank } else { 1 },
                head: verb_idx == head_idx,
            });

            return Some(candidates);

        } else if tf == TOPO_C_FIELD {
            // Find the finite verb of the clause
            if let Some(finite_idx) = traverse_c_to_vc(graph, idx) {
                let verb_idx = resolve_verb(graph, finite_idx);

                candidates.push(CompetingHead {
                    node: &graph[verb_idx],
                    rank: 1,
                    head: head_idx == verb_idx,
                });

                return Some(candidates);
            } else {
                // C-feld without a head.
                return None;
            }
        } else if tf == "MF" || tf == "UK" {
            if relevant_head_tag(pos) {
                candidates.push(CompetingHead {
                    node: node,
                    rank: head_rank,
                    head: head_idx == idx,
                });
            }
        } else {
            return None;
        }
    }

    None
}

fn traverse_c_to_vc(graph: &DependencyGraph, idx: NodeIndex) -> Option<NodeIndex> {
    for idx in ancestor_tokens(graph, idx) {
        let node = &graph[idx];
        let field = ok_or_break!(feature_value(node.token, TOPO_FIELD_FEATURE));

        if field == TOPO_RK_FIELD {
            return Some(idx);
        } else if field != TOPO_C_FIELD {
            return None;
        }
    }

    None
}

fn resolve_verb(graph: &DependencyGraph, verb: NodeIndex) -> NodeIndex {
    // Look for non-aux.
    match first_matching_edge(graph,
                              verb,
                              EdgeDirection::Outgoing,
                              |e| *e == DependencyEdge::Relation(Some(AUXILIARY_RELATION))) {
        Some(idx) => resolve_verb(graph, idx),
        None => verb,
    }
}

fn feature_value(token: &Token, feature: &str) -> Option<String> {
    token.features().map(Features::as_map).and_then(|mut f| f.remove(feature)).and_then(|v| v)
}
