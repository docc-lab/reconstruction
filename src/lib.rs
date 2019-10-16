extern crate chrono;
extern crate config;
extern crate crypto;
extern crate petgraph;
extern crate redis;
extern crate serde;
extern crate serde_json;
extern crate stats;
extern crate uuid;
#[macro_use]
extern crate lazy_static;

pub mod cct;
pub mod controller;
pub mod critical;
pub mod grouping;
pub mod manifest;
pub mod osprofiler;
pub mod trace;

use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;
use std::io::stdin;
use std::path::PathBuf;

use config::{Config, File, FileFormat};
use petgraph::dot::Dot;

use controller::OSProfilerController;
use critical::CriticalPath;
use grouping::Group;
use manifest::Manifest;
use osprofiler::OSProfilerReader;
use osprofiler::REQUEST_TYPE_MAP;
use trace::Event;
use trace::EventEnum;

/// Make a single instrumentation decision.
pub fn make_decision(epoch_file: &str, dry_run: bool) {
    let settings = get_settings();
    let controller = OSProfilerController::from_settings(&settings);
    let manifest_file = PathBuf::from(settings.get("manifest_file").unwrap());
    let manifest =
        Manifest::from_file(manifest_file.as_path()).expect("Couldn't read manifest from cache");
    let reader = OSProfilerReader::from_settings(&settings);
    let traces = reader.read_trace_file(epoch_file);
    let critical_paths = traces.iter().map(|t| CriticalPath::from_trace(t)).collect();
    let mut groups = Group::from_critical_paths(critical_paths);
    groups.sort_by(|a, b| b.variance.partial_cmp(&a.variance).unwrap()); // descending order
    println!("\n\nGroups sorted by variance:\n");
    for group in &groups {
        println!("Group is: {}", group);
    }
    let problem_group = &groups[0];
    println!("\n\nEdges sorted by variance:\n");
    let problem_edges = problem_group.problem_edges();
    for edge in &problem_edges {
        let endpoints = problem_group.g.edge_endpoints(*edge).unwrap();
        println!(
            "({} -> {}): {:?}",
            problem_group.g[endpoints.0], problem_group.g[endpoints.1], problem_group.g[*edge]
        );
    }
    let problem_edge = problem_edges[0];
    println!("\n\nNext tracepoints to enable:\n");
    let tracepoints = manifest.search(problem_group, problem_edge);
    println!("{:?}", tracepoints);
    if !dry_run {
        controller.enable(&tracepoints);
        println!("Enabled tracepoints.");
    }
}

pub fn disable_all() {
    let settings = get_settings();
    let controller = OSProfilerController::from_settings(&settings);
    controller.diable_all();
}

pub fn enable_all() {
    let settings = get_settings();
    let controller = OSProfilerController::from_settings(&settings);
    controller.enable_all();
}

pub fn enable_skeleton() {
    let settings = get_settings();
    let mut manifest_file = PathBuf::from(settings.get("pythia_cache").unwrap());
    manifest_file.push("manifest.json");
    let manifest =
        Manifest::from_file(manifest_file.as_path()).expect("Couldn't read manifest from cache");
    let controller = OSProfilerController::from_settings(&settings);
    controller.diable_all();
    let mut to_enable = manifest.entry_points();
    to_enable.extend(REQUEST_TYPE_MAP.keys().into_iter());
    controller.enable(&to_enable);
    println!("Enabled following tracepoints: {:?}", to_enable);
}

pub fn get_manifest(manfile: &str) {
    let settings = get_settings();
    let reader = OSProfilerReader::from_settings(&settings);
    let traces = reader.read_trace_file(manfile);
    let manifest_method = settings.get("manifest_method").unwrap();
    if manifest_method == "Poset" {
        // let manifest = Poset::from_trace_list(traces);
        // println!("{}", Dot::new(&manifest.g));
    } else if manifest_method == "CCT" {
        let manifest = Manifest::from_trace_list(traces);
        println!("{}", manifest);
        let manifest_file = PathBuf::from(settings.get("manifest_file").unwrap());
        if manifest_file.exists() {
            println!(
                "The manifest file {:?} exists. Overwrite? [y/N]",
                manifest_file
            );
            let mut s = String::new();
            stdin().read_line(&mut s).unwrap();
            if s.chars().nth(0).unwrap() != 'y' {
                return;
            }
            println!("Overwriting");
        }
        manifest.to_file(manifest_file.as_path());
    }
}

pub fn get_trace(trace_id: &str) {
    let settings = get_settings();
    let reader = OSProfilerReader::from_settings(&settings);
    let trace = reader.get_trace_from_base_id(trace_id);
    println!("{}", Dot::new(&trace.g));
}

pub fn get_crit(trace_id: &str) {
    let settings = get_settings();
    let reader = OSProfilerReader::from_settings(&settings);
    let trace = reader.get_trace_from_base_id(trace_id);
    let crit = CriticalPath::from_trace(&trace);
    println!("{}", Dot::new(&crit.g.g));
}

pub fn show_config() {
    let settings = get_settings();
    println!("{:?}", settings);
}

fn get_settings() -> HashMap<String, String> {
    let mut settings = Config::default();
    settings
        .merge(File::new(
            "/opt/stack/reconstruction/Settings.toml",
            FileFormat::Toml,
        ))
        .unwrap();
    let mut results = settings.try_into::<HashMap<String, String>>().unwrap();
    let mut manifest_file = PathBuf::from(results.get("pythia_cache").unwrap());
    manifest_file.push("manifest.json");
    results.insert(
        "manifest_file".to_string(),
        manifest_file.to_string_lossy().to_string(),
    );
    let mut trace_cache = PathBuf::from(results.get("pythia_cache").unwrap());
    trace_cache.push("traces");
    results.insert(
        "trace_cache".to_string(),
        trace_cache.to_string_lossy().to_string(),
    );
    results
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct ManifestNode {
    pub tracepoint_id: String,
    pub variant: EventEnum,
}

impl ManifestNode {
    fn _from_event(span: &Event) -> ManifestNode {
        ManifestNode {
            tracepoint_id: span.tracepoint_id.clone(),
            variant: span.variant,
        }
    }
}

impl Display for ManifestNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const LINE_WIDTH: usize = 75;
        // Break the tracepoint id into multiple lines so that the graphs look prettier
        let mut result = String::with_capacity(self.tracepoint_id.len() + 10);
        let mut written = 0;
        while written <= self.tracepoint_id.len() {
            if written + LINE_WIDTH <= self.tracepoint_id.len() {
                result.push_str(&self.tracepoint_id[written..written + LINE_WIDTH]);
                result.push_str("-\n");
            } else {
                result.push_str(&self.tracepoint_id[written..self.tracepoint_id.len()]);
            }
            written += LINE_WIDTH;
        }
        match self.variant {
            EventEnum::Entry => result.push_str(": S"),
            EventEnum::Exit => result.push_str(": E"),
            EventEnum::Annotation => result.push_str(": A"),
        };
        write!(f, "{}", result)
    }
}

// struct Poset {
//     g: Graph<ManifestNode, u32> // Edge weights indicate number of occurance of an ordering.
// }

// impl Poset {
//     fn from_trace_list(list: Vec<OSProfilerDAG>) -> Poset {
//         let mut dag = Graph::<ManifestNode, u32>::new();
//         let mut node_index_map = HashMap::new();
//         for trace in &list {
//             for nid in trace.g.raw_nodes() {
//                 let node = ManifestNode::from_event(&nid.weight.span);
//                 match node_index_map.get(&node) {
//                     Some(_) => {},
//                     None => {
//                         node_index_map.insert(node.clone(), dag.add_node(node));
//                     }
//                 }
//             }
//         }
//         for trace in &list {
//             for edge in trace.g.raw_edges() {
//                 let source = *node_index_map.get(&ManifestNode::from_event(&trace.g[edge.source()].span)).unwrap();
//                 let target = *node_index_map.get(&ManifestNode::from_event(&trace.g[edge.target()].span)).unwrap();
//                 match dag.find_edge(source, target) {
//                     Some(idx) => {
//                         dag[idx] += 1;
//                     },
//                     None => {
//                         dag.add_edge(source, target, 1);
//                     }
//                 }
//             }
//         }
//         Poset{g: dag}
//     }
// }
