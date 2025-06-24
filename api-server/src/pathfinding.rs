use std::{
    cmp::Ord,
    collections::{BTreeSet, HashMap},
    fmt::{Debug, Display},
    hash::Hash,
};

use log::error;

pub type NodeId = u32;
type EdgeWeight = f32;
pub type AdjacencyMap<V> = HashMap<V, HashMap<V, EdgeWeight>>;

// The following two functions let you control the pathfinding.

// This controls the weight of each edge in the graph bassed on RSSI and SNR values.
pub fn compute_edge_weight(_rssi: i32, snr: f32) -> EdgeWeight {
    if snr < 0.0 {
        EdgeWeight::MAX
    } else {
        // As of writing this I'm a 17 year old who can code and I have no clue what the optimal
        // formula for this is. Some very brief reseach suggests that we may only need SNR, so for
        // now I've made SNR and weight inversely proportional (since higher SNR is better, i.e.
        // lower weight).
        // 1.0 / (snr + 0.1)
        -_rssi as f32 - snr
    }
}

/// This determines how desirable a route is based on the total cost (sum of edge weights calculated
/// with the above function) and the number of hops (edges) in the route.
fn get_route_cost<V: Clone>(dijkstra_entry: &DijkstraEntry<V>) -> EdgeWeight {
    // Currently these weights are pretty arbitrary, but reflect some of my observations so far
    // playing around with the T-Beams.
    const COST_WEIGHT: EdgeWeight = 0.2;
    const HOP_WEIGHT: EdgeWeight = 0.8;

    (dijkstra_entry.total_cost * COST_WEIGHT)
        + (dijkstra_entry.hop_count as EdgeWeight * HOP_WEIGHT)
}

#[derive(Clone, PartialEq, Debug)]
pub struct DijkstraEntry<V: Clone> {
    pub total_cost: EdgeWeight,
    pub previous: Option<V>,
    pub hop_count: usize,
}

type DijkstraResult<V> = HashMap<V, DijkstraEntry<V>>;

pub fn dijkstra<V>(
    adjacency_map: &AdjacencyMap<V>,
    gateway_ids: &Vec<V>,
    start: &V,
) -> DijkstraResult<V>
where
    V: Clone + Eq + Ord + std::hash::Hash + Debug,
{
    let mut result = HashMap::new();

    // initialise all nodes with distance = infinity and previous = None (except for start node
    // which has distance = 0). note: we don't strictly need the `as EdgeWeight` casts but it saves
    // removing/adding the `.0` if the EdgeWeight type changes
    for node_id in adjacency_map.keys() {
        if node_id != start && gateway_ids.contains(node_id) {
            continue;
        }

        result.insert(
            node_id.clone(),
            DijkstraEntry {
                total_cost: if node_id == start {
                    0.0 as EdgeWeight
                } else {
                    EdgeWeight::MAX
                },
                previous: None,
                hop_count: 0,
            },
        );
    }

    // all nodes are unvisited at the start
    let mut unvisited = BTreeSet::from_iter(
        adjacency_map
            .keys()
            .filter(|node_id| *node_id == start || !gateway_ids.contains(node_id)),
    );

    while !unvisited.is_empty() {
        // unvisited node with the smallest distance
        let current = *unvisited
            .iter()
            .min_by(|a, b| {
                result
                    .get(a)
                    .unwrap()
                    .total_cost
                    // have to use partial_cmp because we can't .cmp floats
                    .partial_cmp(&result.get(b).unwrap().total_cost)
                    .unwrap()
            })
            .unwrap();

        unvisited.remove(current);

        let current_entry = result.get(current).unwrap().clone();

        for (neighbour, weight) in adjacency_map.get(current).unwrap() {
            if !unvisited.contains(neighbour) {
                continue;
            }

            let new_dist_to_neighbour = current_entry.total_cost + weight;
            let old_dist_to_neighbour = result.get(neighbour).unwrap().total_cost;

            if new_dist_to_neighbour < old_dist_to_neighbour {
                result.insert(
                    neighbour.clone(),
                    DijkstraEntry {
                        total_cost: new_dist_to_neighbour,
                        previous: Some(current.clone()),
                        hop_count: current_entry.hop_count + 1,
                    },
                );
            }
        }
    }

    result
}

/// Given a graph represented by an adjacency map and a list of gateway nodes represented as
/// vertices, this function produces a table mapping each normal node to a list of nodes it should
/// go to next to reach all accessable gateway nodes in the mesh (in order from best to worst).
/// This information alone is not enough to know the full route, but with each hop, the next node
/// can use what it knows about the best next hops for itself to continue.
pub fn compute_next_hops_map<V>(
    adjacency_map: AdjacencyMap<V>,
    gateway_ids: Vec<V>,
) -> HashMap<V, Vec<V>>
where
    V: Hash + Eq + Ord + Clone + Display + Debug,
{
    let mut result = HashMap::<V, Vec<DijkstraEntry<V>>>::new();

    for gateway_id in &gateway_ids {
        if !adjacency_map.contains_key(gateway_id) {
            error!(
                "Gateway ID {} not found in adjacency map. Returning early",
                gateway_id
            );

            return HashMap::new();
        }

        let dijkstra_table = dijkstra(&adjacency_map, &gateway_ids, gateway_id);

        for (node_id, entry) in dijkstra_table.iter().to_owned() {
            if node_id == gateway_id {
                continue;
            }

            if !result.contains_key(node_id) {
                result.insert(node_id.clone(), Vec::with_capacity(1));
            }

            if result.get(node_id).clone().unwrap().contains(&entry) {
                continue;
            }

            result.get_mut(node_id).unwrap().push(entry.clone());
        }
    }

    // sort each nodes list of next hops by their score
    for (_, next_hop_entries) in result.iter_mut() {
        next_hop_entries.sort_by(|a, b| get_route_cost(a).partial_cmp(&get_route_cost(b)).unwrap());
    }

    // map entries to the id of the node they point to (since we don't need any of the other
    // information now), and return that
    result
        .iter()
        .map(|(node_id, next_hop_entries)| {
            (
                node_id.clone(),
                next_hop_entries
                    .iter()
                    .map(|entry| {
                        entry
                            .previous
                            .clone()
                            .unwrap_or_else(|| panic!("Node {:?} has no previous node", node_id))
                    })
                    .collect(),
            )
        })
        .collect()
}
