//! Text from outside programs (a tool's output, a model name) made safe to keep and show: no
//! terminal escape sequence and no control character survives, so it can never act on the terminal.

/// `s` without CSI and OSC sequences, other escapes and control characters. Tabs and newlines stay;
/// a sequence cut short by the end of the text is dropped whole.
pub fn strip_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    // Parameter and intermediate bytes, then one final byte in 0x40..=0x7e.
                    for n in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    // Up to BEL, or ESC followed by a backslash.
                    while let Some(n) = chars.next() {
                        if n == '\x07' {
                            break;
                        }
                        if n == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next(); // a two-character escape
                }
                None => {}
            },
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csi_and_osc_sequences_are_removed() {
        let evil = "a\x1b[31mred\x1b[0m \x1b]52;c;aGk=\x07done\x1b]0;title\x1b\\!";
        assert_eq!(strip_escapes(evil), "ared done!");
    }

    #[test]
    fn plain_text_tabs_and_newlines_survive() {
        assert_eq!(strip_escapes("héllo\tworld\nnext"), "héllo\tworld\nnext");
    }

    #[test]
    fn other_control_characters_and_carriage_returns_are_dropped() {
        assert_eq!(strip_escapes("a\x00b\x07c\rd\x7fe"), "abcde");
    }

    #[test]
    fn a_lone_or_truncated_escape_is_dropped() {
        assert_eq!(strip_escapes("end\x1b"), "end");
        assert_eq!(strip_escapes("end\x1b[31"), "end");
        assert_eq!(strip_escapes("end\x1b]0;never closed"), "end");
    }
}
