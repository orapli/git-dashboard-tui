//! Copying one identifier to the system clipboard with OSC 52.
//!
//! # Why this is allowed to emit what `git::strip_control_sequences` forbids
//!
//! That sanitiser deletes OSC sequences — `ESC ] 52 ; c ; <base64> BEL` in
//! particular — from **everything git hands back**, because a repository is
//! not trusted: a branch name or a line of a file must never be able to write
//! the user's clipboard just by being displayed.
//!
//! This module is the opposite direction and the only exception: the user
//! pressed `y`, the payload is the one identifier the dashboard chose for the
//! current selection, and it is emitted once, synchronously, by us. The rule
//! it preserves is *provenance*, not "no OSC ever" — repository text may not
//! reach the terminal as a control sequence, and here it cannot: the payload
//! is base64-encoded, so its bytes can never be read as a sequence, and it is
//! sanitised first anyway ([`sanitize`]) so that a name carrying an escape
//! cannot even ride along inside the copied text.
//!
//! The terminal may well ignore the sequence (OSC 52 is off by default in
//! several, and tmux/screen need their own passthrough), and there is no reply
//! to read back: nothing here can confirm the clipboard was actually written,
//! which is why the caller's confirmation message says what *was sent* rather
//! than claiming success.

/// Longest identifier we will copy. Everything the yank key offers — a hash, a
/// ref, a path — is far shorter; a refusal past this is a sign the value is
/// not what we think it is, and silently truncating a path is worse than not
/// copying it.
pub const MAX_CLIPBOARD_BYTES: usize = 4096;

/// Characters `char::is_control` does not cover — it knows only C0, C1 and
/// DEL — but which change how the copied text *reads* once it is pasted.
///
/// The payloads offered by `y` are branch names, tags, file paths and commit
/// author e-mail addresses, every one of them chosen by the repository. Git
/// refuses very little inside a refname, so `U+202E RIGHT-TO-LEFT OVERRIDE`
/// and friends can sit in one: pasted into a shell or a review comment, the
/// identifier then renders as something other than the bytes that were
/// copied (the trojan-source presentation trick). There is no execution risk
/// — the bytes are inert — but a deceptive identifier is exactly what an
/// identifier must not be.
///
/// Stripped rather than refused, consistently with the control characters
/// beside them: the sanitiser's contract is "reduce this to something safe",
/// and a hash or a path that merely *carries* an invisible is still the value
/// the user asked for once the invisible is gone. Refusal stays for the two
/// cases where nothing usable is left — empty, or over the cap.
fn is_invisible_or_bidi(c: char) -> bool {
    matches!(c,
        // Zero-width joiners/non-joiners and the LRM/RLM bidi marks.
        '\u{200b}'..='\u{200f}'
        // LINE SEPARATOR / PARAGRAPH SEPARATOR: a paste would submit on them.
        | '\u{2028}' | '\u{2029}'
        // LRE…RLO: bidi embeddings and overrides.
        | '\u{202a}'..='\u{202e}'
        // WORD JOINER, the invisible operators, and the LRI…PDI isolates.
        | '\u{2060}'..='\u{2069}'
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    /// Nothing left to copy once control characters were removed.
    Empty,
    /// Sanitised length in bytes, which exceeded [`MAX_CLIPBOARD_BYTES`].
    TooLong(usize),
}

/// Reduce a value to something safe to put on the clipboard: no control
/// characters (a paste would otherwise execute in the shell the user is
/// jumping to), no invisible or bidi-reordering characters (see
/// [`is_invisible_or_bidi`] — a paste would otherwise *read* as something
/// else), no surrounding whitespace, bounded length.
pub fn sanitize(text: &str) -> Result<String, ClipboardError> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control() && !is_invisible_or_bidi(*c))
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        return Err(ClipboardError::Empty);
    }
    if cleaned.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardError::TooLong(cleaned.len()));
    }
    Ok(cleaned)
}

/// The OSC 52 sequence that sets the clipboard (`c`) to `text`.
///
/// BEL terminates it rather than ST: both are legal, BEL is understood by
/// every terminal that implements OSC 52 at all.
pub fn osc52_sequence(text: &str) -> String {
    format!("\u{1b}]52;c;{}\u{7}", base64(text.as_bytes()))
}

/// Sanitise `text`, send it to the terminal's clipboard, and return what was
/// actually sent so the caller can show it.
pub fn copy(text: &str) -> Result<String, ClipboardError> {
    let clean = sanitize(text)?;
    emit(&osc52_sequence(&clean));
    Ok(clean)
}

/// Write the sequence straight to stdout.
///
/// Safe to do between frames even with ratatui in raw mode on the alternate
/// screen: an OSC sequence occupies no cells and moves no cursor, so the next
/// draw is unaffected — and it is flushed immediately rather than left in a
/// buffer that the backend might interleave with a frame.
#[cfg(not(test))]
fn emit(sequence: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(sequence.as_bytes());
    let _ = out.flush();
}

#[cfg(test)]
thread_local! {
    static EMITTED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Tests must not spray escape sequences over the test runner's terminal, so
/// the sequence is recorded instead of written.
#[cfg(test)]
fn emit(sequence: &str) {
    EMITTED.with(|e| *e.borrow_mut() = Some(sequence.to_string()));
}

#[cfg(test)]
pub fn take_emitted() -> Option<String> {
    EMITTED.with(|e| e.borrow_mut().take())
}

/// Standard base64, no line breaks — OSC 52 payloads are one line.
/// Hand-rolled rather than adding a dependency for twelve lines of table
/// lookup.
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b1 = *chunk.first().unwrap_or(&0) as u32;
        let b2 = *chunk.get(1).unwrap_or(&0) as u32;
        let b3 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b1 << 16) | (b2 << 8) | b3;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // Non-ASCII goes through as UTF-8 bytes, not as chars.
        assert_eq!(base64("リポ".as_bytes()), "44Oq44Od");
    }

    #[test]
    fn control_characters_are_stripped_before_anything_is_emitted() {
        // A branch name carrying its own OSC 52 must not reach the terminal
        // as a second sequence, nor a newline that would submit a paste.
        let clean = sanitize("main\u{1b}]52;c;ZXZpbA==\u{7}\n").unwrap();
        assert_eq!(clean, "main]52;c;ZXZpbA==");
        assert!(!clean.chars().any(|c| c.is_control()));
        assert_eq!(sanitize("\u{7}\u{1b}\n\t  "), Err(ClipboardError::Empty));
        assert_eq!(sanitize("   "), Err(ClipboardError::Empty));
    }

    /// `char::is_control` covers C0, C1 and DEL and nothing else, so the
    /// characters that make a copied identifier *render* as a different one
    /// went straight through. A ref may contain them, and the value lands in
    /// a shell or a review comment.
    #[test]
    fn invisible_and_bidi_characters_are_stripped_too() {
        // The classic presentation attack: RLO makes `gnp.exe` read as
        // `exe.png`, and the PDF pops the override again.
        assert_eq!(
            sanitize("release-\u{202e}gnp.\u{202c}exe").unwrap(),
            "release-gnp.exe"
        );
        // Zero-width space splitting a branch name, and an LRM/RLM pair.
        assert_eq!(
            sanitize("fea\u{200b}ture/\u{200e}x\u{200f}").unwrap(),
            "feature/x"
        );
        // Every listed range, plus the separators that would submit a paste.
        for c in [
            '\u{200b}', '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}', '\u{2028}', '\u{2029}',
            '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2060}', '\u{2066}',
            '\u{2067}', '\u{2068}', '\u{2069}',
        ] {
            assert_eq!(
                sanitize(&format!("a{c}b")).unwrap(),
                "ab",
                "U+{:04X}",
                c as u32
            );
            assert_eq!(
                sanitize(&c.to_string()),
                Err(ClipboardError::Empty),
                "U+{:04X} alone leaves nothing to copy",
                c as u32
            );
        }
        // Neighbouring characters that are ordinary text must survive: the
        // ranges are closed intervals, not "everything around U+2000".
        for s in ["a\u{200a}b", "a\u{2010}b", "a\u{205f}b", "a\u{206a}b"] {
            assert_eq!(sanitize(s).unwrap(), s);
        }
    }

    #[test]
    fn length_is_capped() {
        let long = "a".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert_eq!(
            sanitize(&long),
            Err(ClipboardError::TooLong(MAX_CLIPBOARD_BYTES + 1))
        );
        assert!(sanitize(&"a".repeat(MAX_CLIPBOARD_BYTES)).is_ok());
    }

    #[test]
    fn the_emitted_sequence_is_well_formed() {
        take_emitted();
        let copied = copy(" abc1234 ").unwrap();
        assert_eq!(copied, "abc1234");
        let seq = take_emitted().unwrap();
        assert_eq!(seq, "\u{1b}]52;c;YWJjMTIzNA==\u{7}");
        assert!(seq.starts_with("\u{1b}]52;c;"));
        assert!(seq.ends_with('\u{7}'));
        // The payload is base64, so no byte of it can be read as a sequence.
        assert!(
            seq["\u{1b}]52;c;".len()..seq.len() - 1]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
        );
        // A refused value emits nothing at all.
        assert_eq!(copy("\n\n"), Err(ClipboardError::Empty));
        assert_eq!(take_emitted(), None);
    }
}
