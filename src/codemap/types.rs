// ABOUTME: Type definitions for the codemap graph-based code intelligence system.
// ABOUTME: Defines nodes (Symbol, File, Module), edges (Defines, Imports, Calls), and metadata.

use crate::api_types::FilePosition;
use serde::{Deserialize, Serialize};

// ============================================================
// ENUMS
// ============================================================

/// Confidence level for edge relationships
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,   // LSP-verified
    Medium, // AST-parsed
    Low,    // Heuristic
}

/// Source of edge provenance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    Lsp,
    Ast,
    Heuristic,
}

/// Node type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    Symbol,
    File,
    Module,
}

/// Edge kind enumeration (Phase 1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Defines, // File -> Symbol
    Imports, // File -> File/Module
    Calls,   // Symbol -> Symbol
}

/// Symbol kind (aligned with LSP SymbolKind)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Interface,
    Trait,
    Struct,
    Enum,
    EnumVariant,
    Type,
    TypeAlias,
    Field,
    Property,
    Variable,
    Constant,
    Module,
    Namespace,
    #[default]
    Unknown,
}

// ============================================================
// NODE IDs
// ============================================================

/// Unique identifier for nodes (blake3 hash of path+position)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    /// Generate ID for a symbol node
    pub fn for_symbol(path: &str, line: u32, character: u32) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        line.hash(&mut hasher);
        character.hash(&mut hasher);
        NodeId(format!("s_{:016x}", hasher.finish()))
    }

    /// Generate ID for a file node
    pub fn for_file(path: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        "file:".hash(&mut hasher);
        path.hash(&mut hasher);
        NodeId(format!("f_{:016x}", hasher.finish()))
    }

    /// Generate ID for a module node
    pub fn for_module(name: &str, path: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        "mod:".hash(&mut hasher);
        name.hash(&mut hasher);
        path.hash(&mut hasher);
        NodeId(format!("m_{:016x}", hasher.finish()))
    }

    /// Get the raw ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Unique identifier for edges
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub String);

impl EdgeId {
    pub fn new(from: &NodeId, to: &NodeId, kind: EdgeKind) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        from.0.hash(&mut hasher);
        to.0.hash(&mut hasher);
        (kind as u8).hash(&mut hasher);
        EdgeId(format!("e_{:016x}", hasher.finish()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ============================================================
// NODE TYPES
// ============================================================

/// Symbol node representing a function, method, type, or field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    pub id: NodeId,
    pub name: String,
    pub kind: SymbolKind,
    pub location: FilePosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_position: Option<FilePosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    pub file_version: u64,
    pub indexed_at: i64,
    pub is_public_api: bool,
}

/// File node representing a source file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub id: NodeId,
    pub path: String,
    pub language: String,
    pub content_hash: String,
    pub mtime: i64,
    pub line_count: u32,
    pub is_external: bool,
}

/// Module node for logical grouping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    pub id: NodeId,
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_file: Option<String>,
    pub is_external: bool,
}

/// Unified node enum for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Node {
    Symbol(SymbolNode),
    File(FileNode),
    Module(ModuleNode),
}

impl Node {
    pub fn id(&self) -> &NodeId {
        match self {
            Node::Symbol(n) => &n.id,
            Node::File(n) => &n.id,
            Node::Module(n) => &n.id,
        }
    }

    pub fn node_type(&self) -> NodeType {
        match self {
            Node::Symbol(_) => NodeType::Symbol,
            Node::File(_) => NodeType::File,
            Node::Module(_) => NodeType::Module,
        }
    }
}

// ============================================================
// EDGE METADATA
// ============================================================

/// Edge metadata for confidence, provenance, and timestamps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeMetadata {
    pub confidence: Confidence,
    pub provenance: Provenance,
    pub validated_at: i64,
    pub source_file_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_file_version: Option<u64>,
    pub is_cross_package: bool,
}

impl Default for EdgeMetadata {
    fn default() -> Self {
        Self {
            confidence: Confidence::Medium,
            provenance: Provenance::Ast,
            validated_at: 0,
            source_file_version: 0,
            target_file_version: None,
            is_cross_package: false,
        }
    }
}

/// A call site location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSite {
    pub line: u32,
    pub character: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

// ============================================================
// EDGE TYPES
// ============================================================

/// Defines edge: File -> Symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinesEdge {
    pub id: EdgeId,
    pub file_id: NodeId,
    pub symbol_id: NodeId,
    pub metadata: EdgeMetadata,
}

/// Imports edge: File -> File/Module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportsEdge {
    pub id: EdgeId,
    pub from_file_id: NodeId,
    pub to_target_id: NodeId,
    pub import_path: String,
    pub metadata: EdgeMetadata,
}

/// Calls edge: Symbol -> Symbol with callsites
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallsEdge {
    pub id: EdgeId,
    pub caller_id: NodeId,
    pub callee_id: NodeId,
    pub call_sites: Vec<CallSite>,
    pub metadata: EdgeMetadata,
}

/// Unified edge enum for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Edge {
    Defines(DefinesEdge),
    Imports(ImportsEdge),
    Calls(CallsEdge),
}

impl Edge {
    pub fn id(&self) -> &EdgeId {
        match self {
            Edge::Defines(e) => &e.id,
            Edge::Imports(e) => &e.id,
            Edge::Calls(e) => &e.id,
        }
    }

    pub fn edge_kind(&self) -> EdgeKind {
        match self {
            Edge::Defines(_) => EdgeKind::Defines,
            Edge::Imports(_) => EdgeKind::Imports,
            Edge::Calls(_) => EdgeKind::Calls,
        }
    }

    pub fn from_node_id(&self) -> &NodeId {
        match self {
            Edge::Defines(e) => &e.file_id,
            Edge::Imports(e) => &e.from_file_id,
            Edge::Calls(e) => &e.caller_id,
        }
    }

    pub fn to_node_id(&self) -> &NodeId {
        match self {
            Edge::Defines(e) => &e.symbol_id,
            Edge::Imports(e) => &e.to_target_id,
            Edge::Calls(e) => &e.callee_id,
        }
    }

    pub fn metadata(&self) -> &EdgeMetadata {
        match self {
            Edge::Defines(e) => &e.metadata,
            Edge::Imports(e) => &e.metadata,
            Edge::Calls(e) => &e.metadata,
        }
    }
}

// ============================================================
// QUERY TYPES
// ============================================================

/// Query mode for codemap tool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryMode {
    Overview,
    Impact,
    Context,
}

/// Codemap query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodemapQuery {
    pub mode: QueryMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_type: Option<EdgeKind>,
    pub depth: u32,
    pub detail: bool,
    pub limit: u32,
    pub offset: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub include_external: bool,
}

impl Default for CodemapQuery {
    fn default() -> Self {
        Self {
            mode: QueryMode::Overview,
            target: None,
            edge_type: None,
            depth: 2,
            detail: false,
            limit: 50,
            offset: 0,
            scope: None,
            include_external: false,
        }
    }
}

/// Module summary for overview mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub path: String,
    pub file_count: u32,
    pub symbol_count: u32,
}

/// Symbol summary with reference count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolSummary {
    pub name: String,
    pub kind: SymbolKind,
    pub location: FilePosition,
    pub reference_count: u32,
}

/// Codemap response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodemapResponse {
    pub mode: QueryMode,
    pub file_count: u32,
    pub symbol_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<ModuleSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<SymbolSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<Node>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<Edge>,
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

impl Default for CodemapResponse {
    fn default() -> Self {
        Self {
            mode: QueryMode::Overview,
            file_count: 0,
            symbol_count: 0,
            target: None,
            modules: Vec::new(),
            symbols: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            limit: 50,
            offset: 0,
            truncated: false,
        }
    }
}
