use std::{
    cmp::Ord,
    collections::{BTreeSet, HashMap},
    fmt::{Debug, Display},
    hash::Hash,
};

use log::debug;

pub type NodeId = u32;
type EdgeWeight = f32;
pub type AdjacencyMap<V> = HashMap<V, HashMap<V, EdgeWeight>>;

pub fn compute_edge_weight(rssi: i32, snr: f32) -> EdgeWeight {
    -rssi as f32 - snr
}

struct DistAndPrevious<V> {
    pub distance: EdgeWeight,
    pub previous: Option<V>,
}

type DijkstraResult<V> = HashMap<V, DistAndPrevious<V>>;

fn dijkstra<V>(adjacency_map: &AdjacencyMap<V>, start: &V) -> DijkstraResult<V>
where
    V: Clone + Eq + Ord + std::hash::Hash + Debug,
{
    let mut result = HashMap::new();

    // initialise all nodes with distance = infinity and previous = None (except for start node
    // which has distance = 0)
    for node in adjacency_map.keys() {
        result.insert(
            node.clone(),
            DistAndPrevious {
                distance: if node == start { 0.0 } else { EdgeWeight::MAX },
                previous: None,
            },
        );
    }

    // all nodes are unvisited at the start
    let mut unvisited = BTreeSet::from_iter(adjacency_map.keys());

    while !unvisited.is_empty() {
        // unvisited node with the smallest distance
        let current = *unvisited
            .iter()
            .min_by(|a, b| {
                result
                    .get(a)
                    .unwrap()
                    .distance
                    // have to use partial_cmp because we can't .cmp floats
                    .partial_cmp(&result.get(b).unwrap().distance)
                    .unwrap()
            })
            .unwrap();

        unvisited.remove(current);

        let current_distance = result.get(current).unwrap().distance;

        for (neighbour, weight) in adjacency_map.get(current).unwrap() {
            if !unvisited.contains(neighbour) {
                continue;
            }

            let new_dist_to_neighbour = current_distance + weight;
            let old_dist_to_neighbour = result.get(neighbour).unwrap().distance;

            if new_dist_to_neighbour < old_dist_to_neighbour {
                result.insert(
                    neighbour.clone(),
                    DistAndPrevious {
                        distance: new_dist_to_neighbour,
                        previous: Some(current.clone()),
                    },
                );
            }
        }
    }

    result
}

pub type Routes<V> = HashMap<V, Vec<Vec<V>>>;

pub fn compute_routes<V>(adjacency_map: AdjacencyMap<V>, gateway_ids: Vec<V>) -> Routes<V>
where
    V: Hash + Eq + Ord + Clone + Display + Debug,
{
    let mut result: Routes<V> = HashMap::new();

    for gateway_id in &gateway_ids {
        if !adjacency_map.contains_key(gateway_id) {
            panic!("Gateway ID {} not found in adjacency map", gateway_id);
        }

        let dijkstra_table = dijkstra(&adjacency_map, gateway_id);

        for node_id in adjacency_map.keys() {
            if gateway_ids.contains(node_id) {
                continue;
            }

            if !result.contains_key(node_id) {
                result.insert(node_id.clone(), Vec::new());
            }

            // trace route through the table

            let mut path = Vec::new();
            let mut current = node_id;

            while let Some(previous) = &dijkstra_table.get(current).unwrap().previous {
                path.push(previous.clone());
                current = previous;
            }

            result.get_mut(node_id).unwrap().push(path);
        }
    }

    debug!("Finished computed routes");

    result
}
