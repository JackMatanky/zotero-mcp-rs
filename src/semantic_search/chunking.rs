//! Paragraph-aware text chunking for embedding.
//!
//! Splits on blank-line-separated paragraphs, accumulating into chunks up to
//! `max_chars`. Never splits a paragraph across chunks unless the paragraph
//! itself exceeds `max_chars`, in which case it is split at sentence
//! boundaries, then (if a single sentence still exceeds `max_chars`) at a
//! hard character-count boundary on a UTF-8 char boundary.

/// Splits `text` into chunks of at most `max_chars` characters each,
/// preferring paragraph (`"\n\n"`) boundaries, then sentence boundaries
/// (`". "`, `"! "`, `"? "`), then a hard UTF-8-safe character cut as a last
/// resort. Empty or all-whitespace paragraphs are dropped. Returns an empty
/// `Vec` if `text` is empty or all-whitespace after trimming.
pub(crate) fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let paragraphs: Vec<&str> =
        text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).collect();

    let mut chunks = Vec::new();
    let mut current = String::new();
    for para in paragraphs {
        let joined_len =
            current.len().saturating_add(2).saturating_add(para.len());
        if !current.is_empty() && joined_len > max_chars {
            chunks.push(std::mem::take(&mut current));
        }
        if para.len() > max_chars {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.extend(split_long_segment(para, max_chars, &[
                ". ", "! ", "? ",
            ]));
            continue;
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Splits `segment` (already known to exceed `max_chars`) at the first
/// separator in `separators` that yields sub-pieces all `<= max_chars`,
/// accumulating pieces the same way `chunk_text` accumulates paragraphs. If
/// no separator in `separators` helps (a single piece is still too long),
/// falls back to `hard_split`.
fn split_long_segment(
    segment: &str,
    max_chars: usize,
    separators: &[&str],
) -> Vec<String> {
    let Some((sep, rest)) = separators.split_first() else {
        return hard_split(segment, max_chars);
    };
    let pieces: Vec<&str> =
        segment.split(*sep).map(str::trim).filter(|p| !p.is_empty()).collect();
    if pieces.len() <= 1 {
        return split_long_segment(segment, max_chars, rest);
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for piece in pieces {
        let joined_len =
            current.len().saturating_add(sep.len()).saturating_add(piece.len());
        if !current.is_empty() && joined_len > max_chars {
            chunks.push(std::mem::take(&mut current));
        }
        if piece.len() > max_chars {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.extend(split_long_segment(piece, max_chars, rest));
            continue;
        }
        if !current.is_empty() {
            current.push_str(sep);
        }
        current.push_str(piece);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Hard-splits `text` into pieces whose *byte* length is `<= max_chars`,
/// cutting only on UTF-8 char boundaries (never mid-codepoint). Used only
/// when no separator in `split_long_segment`'s list breaks a segment into
/// small-enough pieces. A single character whose own byte length exceeds
/// `max_chars` (e.g. a 4-byte emoji with `max_chars = 1`) is kept whole
/// rather than corrupting UTF-8 — the resulting chunk exceeds `max_chars` in
/// that pathological case only.
fn hard_split(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![text.to_owned()];
    }
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut chunks = Vec::new();
    let mut start = 0_usize;
    let mut last_boundary = 0_usize;
    for &idx in &boundaries {
        if idx == 0 {
            continue;
        }
        if idx.saturating_sub(start) > max_chars {
            if let Some(slice) = text.get(start..last_boundary) {
                if !slice.is_empty() {
                    chunks.push(slice.to_owned());
                }
            }
            start = last_boundary;
        }
        last_boundary = idx;
    }
    if let Some(slice) = text.get(start..) {
        if !slice.is_empty() {
            chunks.push(slice.to_owned());
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn empty_input_returns_empty_vec() {
        assert_eq!(chunk_text("", 100), Vec::<String>::new());
        assert_eq!(chunk_text("   \n\n  ", 100), Vec::<String>::new());
    }

    #[test]
    fn single_short_paragraph_returns_one_chunk() {
        let chunks = chunk_text("hello world", 100);
        assert_eq!(chunks, vec!["hello world".to_owned()]);
    }

    #[test]
    fn combined_paragraphs_exceeding_max_split_into_multiple_chunks() {
        let a = "a".repeat(60);
        let b = "b".repeat(60);
        let text = format!("{a}\n\n{b}");
        let chunks = chunk_text(&text, 100);
        assert_eq!(chunks.len(), 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 100);
        }
        assert_eq!(chunks.first().unwrap(), &a);
        assert_eq!(chunks.get(1).unwrap(), &b);
    }

    #[test]
    fn long_paragraph_gets_sentence_split() {
        let sentence = "This is a sentence. ".repeat(20);
        let chunks = chunk_text(&sentence, 100);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= 100);
        }
    }

    #[test]
    fn long_sentence_with_no_separators_gets_hard_split() {
        let text = "a".repeat(250);
        let chunks = chunk_text(&text, 100);
        for chunk in &chunks {
            assert!(chunk.len() <= 100);
        }
        assert_eq!(chunks.join(""), text);
    }

    #[test]
    fn multi_byte_utf8_hard_splits_without_panicking() {
        let text = "é".repeat(150) + &"🎉".repeat(50);
        let chunks = chunk_text(&text, 50);
        for chunk in &chunks {
            assert!(chunk.len() <= 50);
            // Must be valid UTF-8 (String type already guarantees this, but
            // exercise iteration over chars to ensure no mid-codepoint cut).
            let _: Vec<char> = chunk.chars().collect();
        }
        assert_eq!(chunks.concat(), text);
    }
}
