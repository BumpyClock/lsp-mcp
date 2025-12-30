// ABOUTME: Pagination utilities for service layer responses.
// ABOUTME: Provides generic pagination for list operations with limit/offset.

pub const DEFAULT_LIST_LIMIT: u32 = 200;

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct Pagination {
    pub limit: u32,
    pub offset: u32,
    pub truncated: bool,
}

pub(crate) fn paginate_items<T>(
    items: Vec<T>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> (Vec<T>, Pagination) {
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let offset = offset.unwrap_or(0);
    let start = offset as usize;
    let end = std::cmp::min(start.saturating_add(limit as usize), items.len());
    let truncated = end < items.len();
    let paginated = items.into_iter().skip(start).take(limit as usize).collect();
    (
        paginated,
        Pagination {
            limit,
            offset,
            truncated,
        },
    )
}
