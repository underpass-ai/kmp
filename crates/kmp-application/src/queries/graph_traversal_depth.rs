pub const MIN_NATIVE_GRAPH_TRAVERSAL_DEPTH: u32 = 1;
pub const DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH: u32 = 10;
pub const MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH: u32 = 25;

pub fn clamp_native_graph_traversal_depth(depth: u32) -> u32 {
    match depth {
        0 => DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH,
        _ => depth.clamp(
            MIN_NATIVE_GRAPH_TRAVERSAL_DEPTH,
            MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH,
        ),
    }
}

pub fn clamp_native_graph_subtree_depth(depth: u32) -> u32 {
    depth.min(MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH)
}
