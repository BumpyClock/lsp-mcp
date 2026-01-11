// ABOUTME: Identifier lookup utilities for position-based symbol resolution.
// ABOUTME: Finds identifiers at positions with closest-match fallback.

use crate::api_types::{FilePosition, Identifier};
use crate::service::types::errors::PositionError;

pub(crate) async fn find_identifier_at_position(
    identifiers: Vec<Identifier>,
    position: &FilePosition,
) -> Result<Identifier, PositionError> {
    // Try exact containment first
    if let Some(exact_match) = identifiers
        .iter()
        .find(|i| i.file_range.contains(position.clone()))
    {
        return Ok(exact_match.clone().with_kind_defaulted());
    }

    // Fallback: Try same-line matching
    // This helps when ast-grep context ranges don't precisely match the identifier position
    let same_line_matches: Vec<_> = identifiers
        .iter()
        .filter(|i| {
            // Identifier starts on the same line as the position
            i.file_range.range.start.line == position.position.line
        })
        .collect();

    // If there's exactly one identifier on the same line, use it
    if same_line_matches.len() == 1 {
        return Ok(same_line_matches[0].clone().with_kind_defaulted());
    }

    // If multiple identifiers on same line, pick the closest one by character position
    if !same_line_matches.is_empty() {
        let closest_on_line = same_line_matches.into_iter().min_by_key(|id| {
            let start_diff = (id.file_range.range.start.character as i32
                - position.position.character as i32)
                .abs();
            let end_diff = (id.file_range.range.end.character as i32
                - position.position.character as i32)
                .abs();
            start_diff.min(end_diff)
        });

        if let Some(best_match) = closest_on_line {
            return Ok(best_match.clone().with_kind_defaulted());
        }
    }

    // Final fallback: compute distances and return error with closest matches
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

            (
                id.clone().with_kind_defaulted(),
                (start_distance.min(end_distance)) as f64,
            )
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
