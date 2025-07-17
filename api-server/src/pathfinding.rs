use std::{
    cmp::Ord,
    collections::{BTreeSet, HashMap},
    fmt::{Debug, Display},
    hash::Hash,
    sync::Arc,
};

use log::error;
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

use crate::AppSettings;

pub type NodeId = u32;
pub type EdgeWeight = f32;
pub type AdjacencyMap<V> = HashMap<V, HashMap<V, EdgeWeight>>;

const MIN_RSSI: i32 = -120;
const MAX_RSSI: i32 = 0;
const MIN_SNR: f32 = -20.0;
const MAX_SNR: f32 = 30.0;
static MIN_WEIGHT: Lazy<EdgeWeight> = Lazy::new(|| compute_edge_weight(MAX_RSSI, MAX_SNR));
static MAX_WEIGHT: Lazy<EdgeWeight> = Lazy::new(|| compute_edge_weight(MIN_RSSI, MIN_SNR));

static WEIGHT_RANGE: Lazy<EdgeWeight> = Lazy::new(|| {
    let result = *MAX_WEIGHT - *MIN_WEIGHT;

    if result <= 0.0 {
        panic!("Weight range must be greater than 0, got: {}", result);
    }

    result
});

const MAX_HOPS: usize = 10;

fn proportionalise_weight(weight: EdgeWeight) -> EdgeWeight {
    (weight / *WEIGHT_RANGE) * (MAX_HOPS as EdgeWeight)
}

// This controls the weight of each edge in the graph bassed on RSSI and SNR values.
fn compute_edge_weight(_rssi: i32, snr: f32) -> EdgeWeight {
    if snr < MIN_SNR {
        EdgeWeight::MAX
    } else {
        // As of writing this I'm a 17 year old who can code and I have no clue what the optimal
        // formula for this is. Some very brief reseach suggests that we may only need SNR, so for
        // now I've made SNR and weight inversely proportional (since higher SNR is better, i.e.
        // lower weight).
        // let snr_linear = 10_f32.powf(snr / 10.0);
        // 1.0 / snr_linear
        -_rssi as f32 - snr
    }
}

pub fn compute_edge_weight_proportionalised(rssi: i32, snr: f32) -> EdgeWeight {
    proportionalise_weight(compute_edge_weight(rssi, snr))
}

/// This determines how desirable a route is based on the total cost (sum of edge weights calculated
/// with the above function) and the number of hops (edges) in the route.
async fn get_route_cost(
    app_settings: Arc<Mutex<AppSettings>>,
    cost: EdgeWeight,
    hop_count: usize,
) -> EdgeWeight {
    let app_settings = app_settings.lock().await;

    (cost * app_settings.route_cost_weight)
        + (hop_count as EdgeWeight * app_settings.route_hops_weight)
}

#[derive(Clone, PartialEq, Debug)]
pub struct DijkstraEntry<V: Clone> {
    pub total_distance: EdgeWeight,
    pub total_cost: EdgeWeight,
    pub previous: Option<V>,
    pub hop_count: usize,
}

type DijkstraResult<V> = HashMap<V, DijkstraEntry<V>>;

pub async fn dijkstra<V>(
    app_settings: Arc<Mutex<AppSettings>>,
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

        let initial_dist = if node_id == start {
            0.0 as EdgeWeight
        } else {
            EdgeWeight::MAX
        };

        result.insert(
            node_id.clone(),
            DijkstraEntry {
                total_distance: initial_dist,
                total_cost: initial_dist,
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

            let old_cost = result.get(neighbour).unwrap().total_cost;

            let new_cost = get_route_cost(
                app_settings.clone(),
                current_entry.total_distance + weight,
                current_entry.hop_count + 1,
            )
            .await;

            println!(
                "current: {:?}, neighbour: {:?} (w = {}), old_cost: {}, new_cost: {}",
                current, neighbour, weight, old_cost, new_cost
            );

            if new_cost < old_cost {
                result.insert(
                    neighbour.clone(),
                    DijkstraEntry {
                        total_distance: current_entry.total_distance + weight,
                        total_cost: new_cost,
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
pub async fn compute_next_hops_map<V>(
    app_settings: Arc<Mutex<AppSettings>>,
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

        let dijkstra_table = dijkstra(
            app_settings.clone(),
            &adjacency_map,
            &gateway_ids,
            gateway_id,
        )
        .await;

        println!(
            "gateway_id: {}, dijkstra_table: {:?}",
            gateway_id, dijkstra_table
        );

        for (node_id, entry) in dijkstra_table.iter().to_owned() {
            if node_id == gateway_id {
                continue;
            }

            // insert vec if not already present
            if !result.contains_key(node_id) {
                result.insert(node_id.clone(), Vec::with_capacity(1));
            }

            // if the next hop entry already exists for this node, skip it
            if result.get(node_id).clone().unwrap().contains(&entry) {
                continue;
            }

            let insert_position = result
                .get(node_id)
                .unwrap()
                .binary_search_by(|entry| entry.total_cost.partial_cmp(&entry.total_cost).unwrap())
                .unwrap_or_else(|e| e);

            result
                .get_mut(node_id)
                .unwrap()
                .insert(insert_position, entry.clone());
        }
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
