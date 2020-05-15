mod flat;
mod hierarchical;

use petgraph::graph::EdgeIndex;

use crate::controller::OSProfilerController;
use crate::grouping::Group;
use crate::manifest::Manifest;
use crate::search::flat::FlatSearch;
use crate::search::hierarchical::HierarchicalSearch;
use crate::settings::Settings;
use crate::trace::TracepointID;

pub trait SearchStrategy {
    fn search(&self, group: &Group, edge: EdgeIndex, budget: usize) -> Vec<TracepointID>;
}

#[derive(Debug)]
pub enum SearchStrategyType {
    Flat,
    Hierarchical,
}

pub fn get_strategy(
    s: &Settings,
    m: &'static Manifest,
    c: &'static OSProfilerController,
) -> Box<dyn SearchStrategy> {
    match &s.search_strategy {
        SearchStrategyType::Flat => Box::new(FlatSearch::new(s, m, c)),
        SearchStrategyType::Hierarchical => Box::new(HierarchicalSearch::new(s, m, c)),
    }
}
