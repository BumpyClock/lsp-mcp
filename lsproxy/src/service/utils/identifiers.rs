// ABOUTME: Identifier lookup utilities for position-based symbol resolution.
// ABOUTME: Finds identifiers at positions with closest-match fallback.

use crate::api_types::{FilePosition, Identifier};
use crate::service::types::errors::PositionError;

pub(crate) async fn find_identifier_at_position(
    identifiers: Vec<Identifier>,
    position: &FilePosition,
) -> Result<Identifier, PositionError> {
    if let Some(exact_match) = identifiers
        .iter()
        .find(|i| i.file_range.contains(position.clone()))
    {
        return Ok(exact_match.clone().with_kind_defaulted());
    }

    let mut with_distances: Vec<_> = identifiers
        .iter()
        .map(|id| {
            let start_line_diff =
                (id.file_range.range.start.line as i32 - position.position.line as i32).abs();
            let start_char_diff = (id.file_range.range.start.character as i32
                - position.position.character as i32)
                .abs();
            let start_distance = start_line_diff * 100 + start_char_diff;

            let end_line_diff =
                (id.file_range.range.end.line as i32 - position.position.line as i32).abs();
            let end_char_diff = (id.file_range.range.end.character as i32
                - position.position.character as i32)
                .abs();
            let end_distance = end_line_diff * 100 + end_char_diff;

            (id.clone().with_kind_defaulted(), (start_distance.min(end_distance)) as f64)
        })
        .collect();

    with_distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let closest = with_distances
        .into_iter()
        .take(3)
        .map(|(id, _)| id)
        .collect();

    Err(PositionError::IdentifierNotFound { closest })
}
