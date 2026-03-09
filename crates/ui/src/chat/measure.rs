pub fn wrapped_multiline_input_line_count(input: &str, first_width: u16, continuation_width: u16) -> usize {
    fn wrapped_segment_line_count(segment: &str, width: u16) -> usize {
        if width == 0 {
            return 1;
        }
        segment.chars().count().div_ceil(width as usize).max(1)
    }

    let mut segments = input.split('\n');
    let mut total = 0usize;

    if let Some(first) = segments.next() {
        total += wrapped_segment_line_count(first, first_width.max(1));
    } else {
        return 1;
    }

    for segment in segments {
        total += wrapped_segment_line_count(segment, continuation_width.max(1));
    }

    total.max(1)
}

#[cfg(test)]
mod tests {
    use super::wrapped_multiline_input_line_count;

    #[test]
    fn empty_input_still_counts_as_one_line() {
        assert_eq!(wrapped_multiline_input_line_count("", 10, 10), 1);
    }

    #[test]
    fn multiline_input_counts_first_and_continuation_widths() {
        let lines = wrapped_multiline_input_line_count("123456\n12", 4, 2);
        assert_eq!(lines, 3);
    }
}
