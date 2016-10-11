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
use std::io::{BufRead, Write};
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
            Field::NF => "NF",
        }
    }
}

struct TrainingInstance<'a> {
    pub prep: NodeIndex,
    pub prep_obj: NodeIndex,
    pub candidates: Vec<CompetingHead<'a>>,
}

static PREP_COMPL_RELATION: &'static str = "PN";

static PP_RELATION: &'static str = "PP";

static AUXILIARY_RELATION: &'static str = "AUX";

static TOPO_FIELD_FEATURE: &'static str = "tf";

static TOPO_LK_BRACKET: &'static str = "LK";

static TOPO_RK_FIELD: &'static str = "VC";

static TOPO_C_FIELD: &'static str = "C";

static TOPO_UNKNOWN_FIELD: &'static str = "UK";

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
    };
}

fn relevant_head_tag(tag: &str) -> bool {
    tag.starts_with("N") || tag.starts_with("V")
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options] [INPUT_FILE] [OUTPUT_FILE]", program);
    stderr!("{}", opts.usage(&brief));
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
    opts.optflag("s", "stats", "print dataset statistics");
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

    if matches.opt_present("s") {
        print_statistics(reader);
    } else {
        print_ambiguous_pps(reader, &mut writer, matches.opt_present("l"), matches.opt_present("a"));
    }


}

fn print_statistics<R>(reader: conllx::Reader<R>) where R: BufRead {
    let mut n_relevant_tags = 0;
    let mut n_instances = 0;
    let mut n_candidate_heads = 0;

    for sentence in reader.sentences() {
        let sentence = or_exit(sentence);
        let graph = sentence_to_graph(&sentence, false);

        n_relevant_tags += sentence.iter().filter(|t|
            t.pos().map(relevant_head_tag).unwrap_or(false)
        ).count();

        for instance in extract_ambiguous_pps(&graph, false) {
            n_candidate_heads += instance.candidates.len();
            n_instances += 1;
        }
    }

    println!("Average relevant tags: {}", n_relevant_tags as f64 / n_instances as f64);
    println!("Average candidate heads: {}", n_candidate_heads as f64 / n_instances as f64);
}

fn print_ambiguous_pps<R>(reader: conllx::Reader<R>, writer: &mut Write,
                       lemma: bool,
                       all: bool) where R: BufRead {
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

fn print_graph_ambiguous_pps(writer: &mut Write,
                       graph: &DependencyGraph,
                       lemma: bool,
                       all: bool,
                       field: Field) {
    for instance in extract_ambiguous_pps(graph, all, field) {
        let prep = graph[instance.prep].token;
        let prep_obj = graph[instance.prep_obj].token;

        or_exit(write!(writer,
                       "{} {} {} {}",
                       ok_or_continue!(extract_form(&prep, lemma)),
                       ok_or_continue!(prep.pos()),
                       ok_or_continue!(extract_form(&prep_obj, lemma)),
                       ok_or_continue!(prep_obj.pos())));

        let ranks = compute_ranks(graph[instance.prep].offset, &instance.candidates);

        for (rank, candidate) in ranks.iter().zip(instance.candidates) {
            let token = candidate.node.token;
            or_exit(write!(writer,
                           " {} {} {} {} {}",
                           ok_or_continue!(extract_form(&token, lemma)),
                           ok_or_continue!(token.pos()),
                           candidate.node.offset as isize - graph[instance.prep].offset as isize,
                           rank,
                           if candidate.head { 1 } else { 0 }));
        }

        or_exit(writeln!(writer, ""));
    }

}

fn extract_ambiguous_pps<'a>(graph: &'a DependencyGraph<'a>,
                             all: bool)
                             -> Vec<TrainingInstance<'a>> {
    let mut instances = Vec::new();

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

        let field = ok_or_continue!(STRING_FIELD.get(pp_field.as_str()));

        let pn_rel = ok_or_continue!(first_matching_edge(graph,
                                                         edge.target(),
                                                         EdgeDirection::Outgoing,
                                                         |e| {
                                                             *e == DependencyEdge::Relation(Some(PREP_COMPL_RELATION))
                                                         }));

        let competition = match *field {
            Field::VF => ok_or_continue!(find_competition_vf(graph, edge.target(), edge.source())),                
            Field::MF => ok_or_continue!(find_competition_mf(graph, edge.target(), edge.source())),
            Field::NF => ok_or_continue!(find_competition_nf(graph, edge.target(), edge.source())),
        };

        // Don't print when there is no ambiguity.
        if !competition.iter().any(|x| x.head) || (!all && competition.len() == 1) {
            continue;
        }

        instances.push(TrainingInstance {
            prep: edge.target(),
            prep_obj: pn_rel,
            candidates: competition,
        });
    }

    instances
}

fn compute_ranks(p_offset: usize, competition: &Vec<CompetingHead>) -> Vec<isize> {
    let indices: Vec<_> = (0..competition.len()).collect();

    let mut before: Vec<_> =
        indices.iter().filter(|&idx| competition[*idx].node.offset < p_offset).collect();
    before.sort_by(|&a, &b| Ord::cmp(&competition[*b].node.offset, &competition[*a].node.offset));

    let mut after: Vec<_> =
        indices.iter().filter(|&idx| competition[*idx].node.offset > p_offset).collect();
    after.sort_by(|&a, &b| Ord::cmp(&competition[*a].node.offset, &competition[*b].node.offset));

    let mut ranks = vec![0; competition.len()];

    for (rank, &idx) in before.iter().enumerate() {
        ranks[*idx] = -(rank as isize + 1);
    }

    for (rank, &idx) in after.iter().enumerate() {
        ranks[*idx] = rank as isize + 1;
    }

    ranks
}

#[derive(Clone, Debug)]
struct CompetingHead<'a> {
    node: &'a DependencyNode<'a>,
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

        match feature_value(node.token, TOPO_FIELD_FEATURE) {
            Some(field) => field == TOPO_LK_BRACKET,
            None => false,
        }
    }));

    let verb_idx = resolve_verb(graph, lk_idx);

    candidates.push(CompetingHead {
        node: &graph[verb_idx],
        head: verb_idx == head_idx ||
              ancestor_tokens(graph, verb_idx).find(|idx| *idx == head_idx).is_some(),
    });

    let preceding_is_noun = match adjacent_tokens(graph, p_idx, Direction::Preceeding).next() {
        Some(prec_idx) => graph[prec_idx].token.pos().unwrap().starts_with("N"),
        None => false,
    };

    let vf_tokens = adjacent_tokens(graph, p_idx, Direction::Preceeding).take_while(|idx| {
        match feature_value(&graph[*idx].token, TOPO_FIELD_FEATURE) {
            Some(field) => field == Field::VF.string_value() || field == TOPO_UNKNOWN_FIELD,
            None => false,
        }
    });

    add_tokens(graph, head_idx, vf_tokens, &mut candidates);


    // Only add MF tokens when the preceding token is not a noun...
    if !preceding_is_noun {
        // Left bracket should not contain any other material...
        let mf_tokens = adjacent_tokens(graph, lk_idx, Direction::Succeeding)
            .take_while(|idx| {
                match feature_value(&graph[*idx].token, TOPO_FIELD_FEATURE) {
                    Some(field) => field == Field::MF.string_value() || field == TOPO_UNKNOWN_FIELD,
                    None => false,
                }
            });

        add_tokens(graph, head_idx, mf_tokens, &mut candidates);
    }

    Some(candidates)
}

fn find_competition_nf<'a>(graph: &'a DependencyGraph<'a>,
                           p_idx: NodeIndex,
                           head_idx: NodeIndex)
                           -> Option<Vec<CompetingHead>> {
    let mut candidates = Vec::new();

    // Exclude cases where the head is left of the PP.
    // if graph[head_idx].offset < graph[p_idx].offset {
    //     return None;
    // }

    // Find a bracket
    let bracket_idx = try_ok!(adjacent_tokens(graph, p_idx, Direction::Preceeding).find(|idx| {
        let node = &graph[*idx];

        let pos = node.token.pos().unwrap();

        match feature_value(node.token, TOPO_FIELD_FEATURE) {
            Some(field) => {
                (field == TOPO_RK_FIELD || field == TOPO_LK_BRACKET) && pos.starts_with("V")
            }
            None => false,
        }
    }));

    let verb_idx = resolve_verb(graph, bracket_idx);

    candidates.push(CompetingHead {
        node: &graph[verb_idx],
        head: verb_idx == head_idx ||
              ancestor_tokens(graph, verb_idx).find(|idx| *idx == head_idx).is_some(),
    });

    let preceding_is_noun = match adjacent_tokens(graph, p_idx, Direction::Preceeding).next() {
        Some(prec_idx) => graph[prec_idx].token.pos().unwrap().starts_with("N"),
        None => false,
    };

    let nf_tokens = adjacent_tokens(graph, p_idx, Direction::Preceeding).take_while(|idx| {
        match feature_value(&graph[*idx].token, TOPO_FIELD_FEATURE) {
            Some(field) => field == Field::NF.string_value() || field == TOPO_UNKNOWN_FIELD,
            None => false,
        }
    });

    add_tokens(graph, head_idx, nf_tokens, &mut candidates);


    // Only add MF tokens when the preceding token is not a noun...
    if !preceding_is_noun {
        let lk_idx = try_ok!(adjacent_tokens(graph, p_idx, Direction::Preceeding).find(|idx| {
            let node = &graph[*idx];

            match feature_value(node.token, TOPO_FIELD_FEATURE) {
                Some(field) => field == TOPO_C_FIELD || field == TOPO_LK_BRACKET,
                None => false,
            }
        }));

        // Left bracket should not contain any other material...
        let mf_tokens = adjacent_tokens(graph, lk_idx, Direction::Succeeding)
            .take_while(|idx| {
                match feature_value(&graph[*idx].token, TOPO_FIELD_FEATURE) {
                    Some(field) => field == Field::MF.string_value() || field == TOPO_UNKNOWN_FIELD,
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

        if relevant_head_tag(pos) {
            candidates.push(CompetingHead {
                node: node,
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

        if FINITE_VERB_TAGS.contains(pos) {
            let verb_idx = resolve_verb(graph, idx);

            candidates.push(CompetingHead {
                node: &graph[verb_idx],
                head: verb_idx == head_idx,
            });

            return Some(candidates);

        } else if tf == TOPO_C_FIELD {
            // Find the finite verb of the clause
            if let Some(finite_idx) = traverse_c_to_vc(graph, idx) {
                let verb_idx = resolve_verb(graph, finite_idx);

                candidates.push(CompetingHead {
                    node: &graph[verb_idx],
                    head: head_idx == verb_idx,
                });

                return Some(candidates);
            } else {
                // C-feld without a head.
                return None;
            }
        } else if tf == Field::MF.string_value() || tf == TOPO_UNKNOWN_FIELD {
            if relevant_head_tag(pos) {
                candidates.push(CompetingHead {
                    node: node,
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
