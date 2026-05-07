use rustc_errors::Applicability;
use rustc_lint::LateContext;
use rustc_resolve::rustdoc::{add_doc_fragment, main_body_opts, source_span_for_markdown_range};

use rustc_resolve::rustdoc::pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::{DOC_PARAGRAPHS_MISSING_PUNCTUATION, Fragments};

const MSG: &str = "doc paragraphs should end with a terminal punctuation mark";
const PUNCTUATION_SUGGESTION: char = '.';

pub fn check(cx: &LateContext<'_>, doc: &str, fragments: Fragments<'_>) {
    for missing_punctuation in is_missing_punctuation(doc) {
        match missing_punctuation {
            MissingPunctuation::Fixable(offset) => {
                // This ignores `#[doc]` attributes, which we do not handle.
                if let Some(span) = insert_span(cx, fragments, offset) {
                    clippy_utils::diagnostics::span_lint_and_sugg(
                        cx,
                        DOC_PARAGRAPHS_MISSING_PUNCTUATION,
                        span,
                        MSG,
                        "end the paragraph with some punctuation",
                        PUNCTUATION_SUGGESTION.to_string(),
                        Applicability::MaybeIncorrect,
                    );
                }
            },
            MissingPunctuation::Unfixable(offset) => {
                // This ignores `#[doc]` attributes, which we do not handle.
                if let Some(span) = insert_span(cx, fragments, offset) {
                    clippy_utils::diagnostics::span_lint_and_help(
                        cx,
                        DOC_PARAGRAPHS_MISSING_PUNCTUATION,
                        span,
                        MSG,
                        None,
                        "end the paragraph with some punctuation",
                    );
                }
            },
        }
    }
}

fn insert_span(cx: &LateContext<'_>, fragments: Fragments<'_>, offset: usize) -> Option<rustc_span::Span> {
    let needle = fragments.doc[..offset]
        .char_indices()
        .next_back()
        .map_or(offset, |(start, _)| start);

    let mut fragment_start = 0;
    for fragment in fragments.fragments {
        let mut fragment_doc = String::new();
        add_doc_fragment(&mut fragment_doc, fragment);
        let fragment_end = fragment_start + fragment_doc.len();

        if needle < fragment_end {
            let local_offset = offset - fragment_start;
            return source_span_for_markdown_range(
                cx.tcx,
                &fragment_doc,
                &(local_offset..local_offset),
                std::slice::from_ref(fragment),
            )
            .map(|(span, _)| span);
        }

        fragment_start = fragment_end;
    }

    None
}

#[must_use]
/// If punctuation is missing, returns the offset where new punctuation should be inserted.
fn is_missing_punctuation(doc_string: &str) -> Vec<MissingPunctuation> {
    // The colon is not exactly a terminal punctuation mark, but this is required for paragraphs that
    // introduce a table or a list for example.
    const TERMINAL_PUNCTUATION_MARKS: &[char] = &['.', '?', '!', '…', ':'];

    let mut no_report_depth = 0;
    let mut missing_punctuation = Vec::new();
    let mut current_paragraph = None;
    let mut current_event_is_missing_punctuation = false;

    for (event, offset) in
        Parser::new_ext(doc_string, main_body_opts() - Options::ENABLE_SMART_PUNCTUATION).into_offset_iter()
    {
        let last_event_was_missing_punctuation = current_event_is_missing_punctuation;
        current_event_is_missing_punctuation = false;

        match event {
            Event::Start(Tag::FootnoteDefinition(_) | Tag::Heading { .. } | Tag::HtmlBlock | Tag::Table(_)) => {
                no_report_depth += 1;
            },
            Event::Start(Tag::CodeBlock(..) | Tag::List(..)) => {
                no_report_depth += 1;
                if last_event_was_missing_punctuation {
                    // Remove the error from the previous paragraph as it is followed by a code
                    // block or a list.
                    missing_punctuation.pop();
                }
            },
            Event::End(TagEnd::FootnoteDefinition) => {
                no_report_depth -= 1;
            },
            Event::End(
                TagEnd::CodeBlock | TagEnd::Heading(_) | TagEnd::HtmlBlock | TagEnd::List(_) | TagEnd::Table,
            ) => {
                no_report_depth -= 1;
                current_paragraph = None;
            },
            Event::InlineHtml(_) | Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => {
                current_paragraph = None;
            },
            Event::End(TagEnd::Paragraph) => {
                if let Some(mp) = current_paragraph {
                    missing_punctuation.push(mp);
                    current_event_is_missing_punctuation = true;
                }
            },
            Event::Code(..) | Event::Start(Tag::Link { .. }) | Event::End(TagEnd::Link)
                if no_report_depth == 0 && !offset.is_empty() =>
            {
                if trim_trailing_symbols(&doc_string[..offset.end]).ends_with(TERMINAL_PUNCTUATION_MARKS) {
                    current_paragraph = None;
                } else {
                    current_paragraph = Some(MissingPunctuation::Fixable(offset.end));
                }
            },
            Event::Text(..) if no_report_depth == 0 && !offset.is_empty() => {
                let trimmed = trim_trailing_symbols(&doc_string[..offset.end]);
                if trimmed.ends_with(TERMINAL_PUNCTUATION_MARKS) {
                    current_paragraph = None;
                } else if let Some(t) = trimmed.strip_suffix(|c| c == ')' || c == '"') {
                    if t.ends_with(TERMINAL_PUNCTUATION_MARKS) {
                        // Avoid false positives.
                        current_paragraph = None;
                    } else {
                        current_paragraph = Some(MissingPunctuation::Unfixable(offset.end));
                    }
                } else {
                    current_paragraph = Some(MissingPunctuation::Fixable(offset.end));
                }
            },
            _ => {},
        }
    }

    missing_punctuation
}

fn trim_trailing_symbols(s: &str) -> &str {
    s.trim_end_matches(|c: char|
        // Source: https://unicodeplus.com
        matches!(c as u32,
            0x1F300..=0x1F5FF | // Miscellaneous Symbols and Pictographs
            0x1F600..=0x1F64F | // Emoticons
            0x1F900..=0x1F9FF | // Supplemental Symbols and Pictographs
            0x2700..=0x27BF   | // Dingbats
            0x1FA70..=0x1FAFF | // Symbols and Pictographs Extended-A
            0x1F680..=0x1F6FF | // Transport and Map Symbols
            0x2600..=0x26FF | // Miscellaneous Symbols
            0xFE00..=0xFE0F // Variation selectors
        ) || c.is_whitespace())
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MissingPunctuation {
    Fixable(usize),
    Unfixable(usize),
}
