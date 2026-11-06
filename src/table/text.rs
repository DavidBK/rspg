//! Cutting strings by terminal columns (not bytes or chars).

use std::ops::Range;

use unicode_width::UnicodeWidthChar;

/// Printable ASCII: one byte = one terminal column, so columns can be
/// sliced as bytes. Most psql output is like this.
fn is_simple(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// Cut `width` terminal columns out of `s`, starting at column `start`.
/// The result is always exactly `width` columns wide (padded with spaces),
/// and a double-width char cut in half is replaced by a space.
pub fn slice_cols(s: &str, start: usize, width: usize) -> String {
    let end = start + width;
    let mut out = String::with_capacity(width);

    if is_simple(s) {
        let part = &s[start.min(s.len())..end.min(s.len())];
        out.push_str(part);
        out.extend(std::iter::repeat_n(' ', width - part.len()));
        return out;
    }

    let mut col = 0;
    let mut used = 0;
    for c in s.chars() {
        if col >= end {
            break;
        }
        let w = c.width().unwrap_or(0);
        let next = col + w;
        if col >= start && next <= end {
            out.push(c);
            used += w;
        } else if next > start {
            // Wide char straddling an edge: fill its visible part with spaces.
            let visible = next.min(end) - col.max(start);
            out.extend(std::iter::repeat_n(' ', visible));
            used += visible;
        }
        col = next;
    }
    out.extend(std::iter::repeat_n(' ', width - used));
    out
}

/// The trimmed text in the columns `cols` of `line`.
pub fn cell_text(line: &str, cols: &Range<usize>) -> String {
    slice_cols(line, cols.start, cols.len()).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_ascii() {
        assert_eq!(slice_cols("abcdef", 1, 3), "bcd");
        assert_eq!(slice_cols("abc", 2, 3), "c  ");
        assert_eq!(slice_cols("abc", 5, 2), "  ");
    }

    #[test]
    fn slices_wide_chars() {
        assert_eq!(slice_cols("ab", 0, 4), "ab  ");
        assert_eq!(slice_cols("a日b", 1, 2), "日");
        assert_eq!(slice_cols("a日b", 2, 2), " b"); // right half of 日
        assert_eq!(slice_cols("a日b", 0, 2), "a "); // left half of 日
    }
}
