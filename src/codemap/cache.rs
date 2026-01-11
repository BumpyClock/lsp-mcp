// ABOUTME: In-memory graph cache for fast codemap traversal queries.
// ABOUTME: Provides O(1) node lookup and efficient edge traversal.

use crate::codemap::types::*;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet, VecDeque};

/// In-memory graph cache for fast traversal
pub struct GraphCache {
    /// Node ID -> Node data
    nodes: RwLock<HashMap<NodeId, Node>>,

    /// Adjacency list: from_node -> [(edge_kind, to_node, edge_id)]
    outgoing_edges: RwLock<HashMap<NodeId, Vec<(EdgeKind, NodeId, EdgeId)>>>,

    /// Reverse adjacency: to_node -> [(edge_kind, from_node, edge_id)]
    incoming_edges: RwLock<HashMap<NodeId, Vec<(EdgeKind, NodeId, EdgeId)>>>,

    /// Edge ID -> Edge data
    edges: RwLock<HashMap<EdgeId, Edge>>,

    /// Symbol name -> symbol IDs (for name-based lookup, case-insensitive)
    symbol_index: RwLock<HashMap<String, Vec<NodeId>>>,

    /// File path -> file node ID
    file_index: RwLock<HashMap<String, NodeId>>,

    /// File path -> symbol IDs defined in that file
    file_symbols: RwLock<HashMap<String, Vec<NodeId>>>,
}

impl GraphCache {
    /// Create an empty cache
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            outgoing_edges: RwLock::new(HashMap::new()),
            incoming_edges: RwLock::new(HashMap::new()),
            edges: RwLock::new(HashMap::new()),
            symbol_index: RwLock::new(HashMap::new()),
            file_index: RwLock::new(HashMap::new()),
            file_symbols: RwLock::new(HashMap::new()),
        }
    }

    /// Clear all data
    pub fn clear(&self) {
        self.nodes.write().clear();
        self.outgoing_edges.write().clear();
        self.incoming_edges.write().clear();
        self.edges.write().clear();
        self.symbol_index.write().clear();
        self.file_index.write().clear();
        self.file_symbols.write().clear();
    }

    // ============================================================
    // NODE OPERATIONS
    // ============================================================

    /// Insert or update a node
    pub fn insert_node(&self, node: Node) {
        let node_id = node.id().clone();

        match &node {
            Node::Symbol(s) => {
                let name_lower = s.name.to_lowercase();
                self.symbol_index
                    .write()
                    .entry(name_lower)
                    .or_insert_with(Vec::new)
                    .push(node_id.clone());

                let file_path = s.location.path.clone();
                self.file_symbols
                    .write()
                    .entry(file_path)
                    .or_insert_with(Vec::new)
                    .push(node_id.clone());
            }
            Node::File(f) => {
                self.file_index
                    .write()
                    .insert(f.path.clone(), node_id.clone());
            }
            Node::Module(_) => {}
        }

        self.nodes.write().insert(node_id, node);
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &NodeId) -> Option<Node> {
        self.nodes.read().get(id).cloned()
    }

    /// Remove a node and all its edges
    pub fn remove_node(&self, id: &NodeId) {
        let node = self.nodes.write().remove(id);

        if let Some(node) = node {
            match node {
                Node::Symbol(s) => {
                    let name_lower = s.name.to_lowercase();
                    if let Some(ids) = self.symbol_index.write().get_mut(&name_lower) {
                        ids.retain(|nid| nid != id);
                    }

                    let file_path = &s.location.path;
                    if let Some(ids) = self.file_symbols.write().get_mut(file_path) {
                        ids.retain(|nid| nid != id);
                    }
                }
                Node::File(f) => {
                    self.file_index.write().remove(&f.path);
                }
                Node::Module(_) => {}
            }
        }

        let outgoing = self.outgoing_edges.write().remove(id);
        let incoming = self.incoming_edges.write().remove(id);

        let mut edge_ids_to_remove = Vec::new();

        if let Some(outgoing) = outgoing {
            for (_, _, edge_id) in outgoing {
                edge_ids_to_remove.push(edge_id);
            }
        }

        if let Some(incoming) = incoming {
            for (_, _, edge_id) in incoming {
                edge_ids_to_remove.push(edge_id);
            }
        }

        let mut edges = self.edges.write();
        for edge_id in edge_ids_to_remove {
            edges.remove(&edge_id);
        }
    }

    /// Check if node exists
    pub fn contains_node(&self, id: &NodeId) -> bool {
        self.nodes.read().contains_key(id)
    }

    /// Get all nodes
    pub fn get_all_nodes(&self) -> Vec<Node> {
        self.nodes.read().values().cloned().collect()
    }

    /// Get node count
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    // ============================================================
    // EDGE OPERATIONS
    // ============================================================

    /// Insert an edge (updates adjacency lists)
    pub fn insert_edge(&self, edge: Edge) {
        let edge_id = edge.id().clone();
        let edge_kind = edge.edge_kind();
        let from_id = edge.from_node_id().clone();
        let to_id = edge.to_node_id().clone();

        self.outgoing_edges
            .write()
            .entry(from_id.clone())
            .or_insert_with(Vec::new)
            .push((edge_kind, to_id.clone(), edge_id.clone()));

        self.incoming_edges
            .write()
            .entry(to_id.clone())
            .or_insert_with(Vec::new)
            .push((edge_kind, from_id.clone(), edge_id.clone()));

        self.edges.write().insert(edge_id, edge);
    }

    /// Get an edge by ID
    pub fn get_edge(&self, id: &EdgeId) -> Option<Edge> {
        self.edges.read().get(id).cloned()
    }

    /// Remove an edge
    pub fn remove_edge(&self, id: &EdgeId) {
        let edge = self.edges.write().remove(id);

        if let Some(edge) = edge {
            let from_id = edge.from_node_id();
            let to_id = edge.to_node_id();

            if let Some(outgoing) = self.outgoing_edges.write().get_mut(from_id) {
                outgoing.retain(|(_, _, eid)| eid != id);
            }

            if let Some(incoming) = self.incoming_edges.write().get_mut(to_id) {
                incoming.retain(|(_, _, eid)| eid != id);
            }
        }
    }

    /// Get edge count
    pub fn edge_count(&self) -> usize {
        self.edges.read().len()
    }

    // ============================================================
    // TRAVERSAL
    // ============================================================

    /// Get all outgoing edges from a node
    pub fn get_outgoing(&self, node_id: &NodeId) -> Vec<(EdgeKind, NodeId, EdgeId)> {
        self.outgoing_edges
            .read()
            .get(node_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all incoming edges to a node
    pub fn get_incoming(&self, node_id: &NodeId) -> Vec<(EdgeKind, NodeId, EdgeId)> {
        self.incoming_edges
            .read()
            .get(node_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get outgoing edges filtered by kind
    pub fn get_outgoing_by_kind(&self, node_id: &NodeId, kind: EdgeKind) -> Vec<(NodeId, EdgeId)> {
        self.outgoing_edges
            .read()
            .get(node_id)
            .map(|edges| {
                edges
                    .iter()
                    .filter(|(k, _, _)| *k == kind)
                    .map(|(_, nid, eid)| (nid.clone(), eid.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get incoming edges filtered by kind
    pub fn get_incoming_by_kind(&self, node_id: &NodeId, kind: EdgeKind) -> Vec<(NodeId, EdgeId)> {
        self.incoming_edges
            .read()
            .get(node_id)
            .map(|edges| {
                edges
                    .iter()
                    .filter(|(k, _, _)| *k == kind)
                    .map(|(_, nid, eid)| (nid.clone(), eid.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all callers of a symbol (incoming Calls edges)
    pub fn get_callers(&self, symbol_id: &NodeId) -> Vec<(NodeId, EdgeId)> {
        self.get_incoming_by_kind(symbol_id, EdgeKind::Calls)
    }

    /// Get all callees of a symbol (outgoing Calls edges)
    pub fn get_callees(&self, symbol_id: &NodeId) -> Vec<(NodeId, EdgeId)> {
        self.get_outgoing_by_kind(symbol_id, EdgeKind::Calls)
    }

    /// BFS traversal from a node with depth limit
    /// Returns (node_id, depth) pairs
    pub fn traverse_bfs(
        &self,
        start: &NodeId,
        edge_kinds: &[EdgeKind],
        direction: TraversalDirection,
        max_depth: usize,
    ) -> Vec<(NodeId, usize)> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        queue.push_back((start.clone(), 0));
        visited.insert(start.clone());

        while let Some((node_id, depth)) = queue.pop_front() {
            result.push((node_id.clone(), depth));

            if depth >= max_depth {
                continue;
            }

            let neighbors = match direction {
                TraversalDirection::Outgoing => self.get_outgoing(&node_id),
                TraversalDirection::Incoming => self.get_incoming(&node_id),
                TraversalDirection::Both => {
                    let mut all = self.get_outgoing(&node_id);
                    all.extend(self.get_incoming(&node_id));
                    all
                }
            };

            for (kind, neighbor_id, _) in neighbors {
                if edge_kinds.contains(&kind) && !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id.clone());
                    queue.push_back((neighbor_id, depth + 1));
                }
            }
        }

        result
    }

    // ============================================================
    // LOOKUP
    // ============================================================

    /// Search symbols by name prefix (case-insensitive)
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<NodeId> {
        let query_lower = query.to_lowercase();
        let symbol_index = self.symbol_index.read();

        let mut results = Vec::new();
        for (name, ids) in symbol_index.iter() {
            if name.starts_with(&query_lower) {
                results.extend(ids.clone());
                if results.len() >= limit {
                    break;
                }
            }
        }

        results.truncate(limit);
        results
    }

    /// Get file node by path
    pub fn get_file_by_path(&self, path: &str) -> Option<NodeId> {
        self.file_index.read().get(path).cloned()
    }

    /// Get all symbols defined in a file
    pub fn get_symbols_in_file(&self, path: &str) -> Vec<NodeId> {
        self.file_symbols
            .read()
            .get(path)
            .cloned()
            .unwrap_or_default()
    }

    // ============================================================
    // BULK OPERATIONS
    // ============================================================

    /// Insert multiple nodes
    pub fn insert_nodes(&self, nodes: Vec<Node>) {
        for node in nodes {
            self.insert_node(node);
        }
    }

    /// Insert multiple edges
    pub fn insert_edges(&self, edges: Vec<Edge>) {
        for edge in edges {
            self.insert_edge(edge);
        }
    }

    /// Remove all data for a file (file node, symbols, edges)
    pub fn remove_file_data(&self, path: &str) {
        let file_node_id = self.file_index.read().get(path).cloned();

        if let Some(file_id) = file_node_id {
            self.remove_node(&file_id);
        }

        let symbol_ids = self
            .file_symbols
            .read()
            .get(path)
            .cloned()
            .unwrap_or_default();

        for symbol_id in symbol_ids {
            self.remove_node(&symbol_id);
        }

        self.file_symbols.write().remove(path);
    }
}

/// Direction for graph traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

impl Default for GraphCache {
    fn default() -> Self {
        Self::new()
    }
}
