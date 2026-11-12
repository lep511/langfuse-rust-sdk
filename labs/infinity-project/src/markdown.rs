//! Markdown for the terminal, parsed with `markdown-rs` (GitHub flavour).
//!
//! [`render`] turns a Markdown text into lines of styled text that fit a
//! width: wrapped paragraphs, headings, lists, quotes, code blocks, and
//! tables drawn with borders. [`MarkdownStream`] does the same for a text
//! that arrives in chunks, and gives each part as soon as nothing that
//! follows can change it.

use std::ops::Range;

use markdown::mdast::{AlignKind, List, Node, Table};
use markdown::{ParseOptions, to_mdast};
use richrs::color::{Color, StandardColor};
use richrs::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const CODE_COLOR: Color = Color::Standard(StandardColor::Green);
const HEADING_COLOR: Color = Color::Standard(StandardColor::Cyan);
/// Columns are not narrowed below this to make a table fit.
const MIN_COLUMN_WIDTH: usize = 4;

/// A piece of text in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Style::new())
    }

    fn width(&self) -> usize {
        self.text.width()
    }
}

/// One line of output. An empty one is a blank line.
pub type Line = Vec<Span>;

/// The style of borders, rules and other marks that are not the text.
fn subtle() -> Style {
    Style::new().dim()
}

/// Renders a whole Markdown text, with a blank line between its blocks.
#[cfg(test)]
pub fn render(source: &str, width: usize) -> Vec<Line> {
    blocks(&parse(source), width, true)
}

/// The top-level blocks of `source`.
fn parse(source: &str) -> Vec<Node> {
    match to_mdast(source, &ParseOptions::gfm()) {
        Ok(Node::Root(root)) => root.children,
        // Only MDX syntax can fail to parse, and it is not enabled.
        _ => Vec::new(),
    }
}

/// Renders Markdown that arrives in chunks.
///
/// [`push`](Self::push) returns what can no longer change, and keeps the
/// rest for later:
///
/// - Only whole lines are read. Half a line can look like something it is
///   not: `2.` is a list item until it goes on as `2.5 million`.
/// - A block is final once the next one starts: until then a paragraph may
///   grow, and a line of text may turn out to be the header of a table. Some
///   are final sooner: a heading at the end of its line, and a paragraph, a
///   table or a quote at the blank line after it.
/// - Two kinds of block would make for a long wait and are given piece by
///   piece: a list gives each item once the next one starts, and a code
///   block gives each line.
#[derive(Default)]
pub struct MarkdownStream {
    /// The source that is not rendered in full: the block in progress, from
    /// its start.
    pending: String,
    /// How much of the block in progress is rendered already.
    progress: Progress,
    /// Whether anything has been rendered: the next block is set apart from
    /// it by a blank line.
    started: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Progress {
    #[default]
    Nothing,
    /// The first items of a list.
    Items(usize),
    /// The first lines of a code block.
    CodeLines(usize),
}

impl MarkdownStream {
    /// Adds `chunk` and returns the lines that are now final.
    pub fn push(&mut self, chunk: &str, width: usize) -> Vec<Line> {
        let mid_line = !self.pending.is_empty() && !self.pending.ends_with('\n');
        self.pending.push_str(chunk);
        let mut lines = Vec::new();
        // More of a line that had already started changes nothing: neither
        // the whole lines, nor how the line after them starts.
        if mid_line && !chunk.contains('\n') {
            return lines;
        }
        let Some(whole) = self.pending.rfind('\n').map(|index| index + 1) else {
            return lines;
        };
        let nodes = parse(&self.pending[..whole]);
        let Some((last, complete)) = nodes.split_last() else {
            // Blank lines only.
            self.pending.drain(..whole);
            return lines;
        };
        for node in complete {
            self.finish_block(node, width, &mut lines);
        }
        let consumed = if is_final(last, &self.pending[..whole], &self.pending[whole..]) {
            self.finish_block(last, width, &mut lines);
            whole
        } else {
            self.advance_block(last, width, &mut lines);
            last.position().map_or(0, |position| position.start.offset)
        };
        self.pending.drain(..consumed);
        lines
    }

    /// Ends the text and returns the lines not rendered yet.
    pub fn finish(&mut self, width: usize) -> Vec<Line> {
        let mut lines = Vec::new();
        for node in &parse(&self.pending) {
            self.finish_block(node, width, &mut lines);
        }
        self.pending.clear();
        lines
    }

    /// The source that is not rendered yet, or only in part.
    pub fn pending(&self) -> &str {
        &self.pending
    }

    /// Renders what is left of a block that is complete.
    fn finish_block(&mut self, node: &Node, width: usize, lines: &mut Vec<Line>) {
        let rendered = match (node, self.progress) {
            (Node::List(list), Progress::Items(done)) => {
                list_items(list, done..list.children.len(), width)
            }
            (Node::Code(code), Progress::CodeLines(done)) => {
                code_lines(&code.value, done..usize::MAX)
            }
            _ => block(node, width),
        };
        let continued = self.progress != Progress::Nothing;
        self.progress = Progress::Nothing;
        self.emit(rendered, continued, lines);
    }

    /// Renders the parts of the block in progress that can no longer change.
    fn advance_block(&mut self, node: &Node, width: usize, lines: &mut Vec<Line>) {
        match node {
            // The last item is the one that may still grow.
            Node::List(list) => {
                let done = match self.progress {
                    Progress::Items(done) => done,
                    _ => 0,
                };
                let ready = list.children.len().saturating_sub(1);
                if ready > done {
                    self.emit(list_items(list, done..ready, width), done > 0, lines);
                    self.progress = Progress::Items(ready);
                }
            }
            // Only whole lines were read, so every line of code is final.
            Node::Code(code) => {
                let done = match self.progress {
                    Progress::CodeLines(done) => done,
                    _ => 0,
                };
                let ready = code_line_count(&code.value);
                if ready > done {
                    self.emit(code_lines(&code.value, done..ready), done > 0, lines);
                    self.progress = Progress::CodeLines(ready);
                }
            }
            _ => {}
        }
    }

    /// Adds `rendered` to `lines`, after a blank line unless it continues
    /// the block rendered before it.
    fn emit(&mut self, rendered: Vec<Line>, continued: bool, lines: &mut Vec<Line>) {
        if rendered.is_empty() {
            return;
        }
        if self.started && !continued {
            lines.push(Line::new());
        }
        lines.extend(rendered);
        self.started = true;
    }
}

/// Whether `last`, the last block of the whole lines read so far (`source`),
/// is complete whatever comes next. `partial` is the line that follows them,
/// still being written.
fn is_final(last: &Node, source: &str, partial: &str) -> bool {
    let after_blank_line = source
        .strip_suffix('\n')
        .and_then(|lines| lines.rsplit('\n').next())
        .is_some_and(|line| line.trim().is_empty());
    match last {
        // One line, and it is over.
        Node::Heading(_) | Node::ThematicBreak(_) => true,
        // A blank line ends them.
        Node::Paragraph(_) | Node::Table(_) | Node::Blockquote(_) | Node::Definition(_) => {
            after_blank_line
        }
        // A list goes on after a blank line, with another item or with
        // indented text. A line that starts as neither ends it.
        Node::List(_) => {
            let continues = |c: char| c.is_whitespace() || c.is_ascii_digit() || "-*+".contains(c);
            after_blank_line && partial.chars().next().is_some_and(|c| !continues(c))
        }
        // Code has blank lines of its own; the lines it has are given
        // anyway.
        _ => false,
    }
}

/// Renders `nodes` one under the other, with a blank line between them when
/// `spaced`.
fn blocks(nodes: &[Node], width: usize, spaced: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    for node in nodes {
        let rendered = block(node, width);
        if rendered.is_empty() {
            continue;
        }
        if spaced && !lines.is_empty() {
            lines.push(Line::new());
        }
        lines.extend(rendered);
    }
    lines
}

fn block(node: &Node, width: usize) -> Vec<Line> {
    match node {
        Node::Paragraph(paragraph) => wrap(&inlines(&paragraph.children, &Style::new()), width),
        Node::Heading(heading) => {
            let style = if heading.depth <= 2 {
                Style::color(HEADING_COLOR).bold()
            } else {
                Style::new().bold()
            };
            wrap(&inlines(&heading.children, &style), width)
        }
        Node::ThematicBreak(_) => vec![vec![Span::new("─".repeat(width), subtle())]],
        Node::Blockquote(quote) => blocks(&quote.children, width.saturating_sub(2), true)
            .into_iter()
            .map(|mut line| {
                line.insert(0, Span::new("│ ", subtle()));
                line
            })
            .collect(),
        Node::List(list) => list_items(list, 0..list.children.len(), width),
        Node::Code(code) => code_lines(&code.value, 0..usize::MAX),
        Node::Math(math) => code_lines(&math.value, 0..usize::MAX),
        Node::Table(table) => table_lines(table, width),
        Node::Html(html) => html
            .value
            .lines()
            .map(|line| vec![Span::plain(line)])
            .collect(),
        // Where a reference link points to: not part of the text.
        Node::Definition(_) => Vec::new(),
        Node::FootnoteDefinition(footnote) => {
            let marker = format!("[^{}]: ", footnote.identifier);
            hang(
                blocks(
                    &footnote.children,
                    width.saturating_sub(marker.width()),
                    true,
                ),
                &marker,
            )
        }
        other => wrap(&[Inline::Text(Span::plain(other.to_string()))], width),
    }
}

/// The items of `list` in `range`, each after its bullet, number or check
/// box. There are no blank lines between items, whatever the source has, so
/// that a list reads the same however much of it has arrived.
fn list_items(list: &List, range: Range<usize>, width: usize) -> Vec<Line> {
    let first_number = u64::from(list.start.unwrap_or(1));
    let mut lines = Vec::new();
    for index in range {
        let Some(Node::ListItem(item)) = list.children.get(index) else {
            continue;
        };
        let marker = match item.checked {
            Some(true) => "[x] ".to_owned(),
            Some(false) => "[ ] ".to_owned(),
            None if list.ordered => format!("{}. ", first_number + index as u64),
            None => "• ".to_owned(),
        };
        let body = blocks(
            &item.children,
            width.saturating_sub(marker.width()),
            item.spread,
        );
        lines.extend(hang(body, &marker));
    }
    lines
}

/// Puts `marker` before the first line of `body` and aligns the other lines
/// with the text that follows it.
fn hang(mut body: Vec<Line>, marker: &str) -> Vec<Line> {
    if body.is_empty() {
        body.push(Line::new());
    }
    let indent = " ".repeat(marker.width());
    for (index, line) in body.iter_mut().enumerate() {
        if index == 0 {
            line.insert(0, Span::plain(marker));
        } else if !line.is_empty() {
            line.insert(0, Span::plain(indent.as_str()));
        }
    }
    body
}

fn code_line_count(code: &str) -> usize {
    if code.is_empty() {
        0
    } else {
        code.split('\n').count()
    }
}

/// The lines of `code` in `range`, indented. They are not wrapped: code is
/// copied as it is.
fn code_lines(code: &str, range: Range<usize>) -> Vec<Line> {
    let end = range.end.min(code_line_count(code));
    code.split('\n')
        .take(end)
        .skip(range.start)
        .map(|line| {
            if line.is_empty() {
                Line::new()
            } else {
                vec![Span::new(format!("  {line}"), Style::color(CODE_COLOR))]
            }
        })
        .collect()
}

/// A table with borders. Columns are as wide as their widest cell; when the
/// table does not fit in `width`, the widest columns are narrowed and their
/// cells wrapped.
fn table_lines(table: &Table, width: usize) -> Vec<Line> {
    // The content of each cell, by row. The first row is the header.
    let rows: Vec<Vec<Vec<Inline>>> = table
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let Node::TableRow(row) = row else {
                return None;
            };
            let style = if index == 0 {
                Style::new().bold()
            } else {
                Style::new()
            };
            let cells = row.children.iter().map(|cell| match cell.children() {
                Some(children) => inlines(children, &style),
                None => Vec::new(),
            });
            Some(cells.collect())
        })
        .collect();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }

    let mut widths = vec![1; columns];
    for row in &rows {
        for (column, cell) in row.iter().enumerate() {
            let natural = wrap(cell, usize::MAX)
                .iter()
                .map(|line| line_width(line))
                .max();
            widths[column] = widths[column].max(natural.unwrap_or(0));
        }
    }
    // A column takes its width, a space on either side and a border; one
    // more border closes the row.
    let available = width.saturating_sub(3 * columns + 1);
    while widths.iter().sum::<usize>() > available {
        match widths.iter_mut().max_by_key(|column| **column) {
            Some(widest) if *widest > MIN_COLUMN_WIDTH => *widest -= 1,
            // As narrow as it gets: the terminal wraps the rest.
            _ => break,
        }
    }

    // The lines of each cell, by row.
    let rows: Vec<Vec<Vec<Line>>> = rows
        .iter()
        .map(|row| {
            let cell = |column: usize| row.get(column).map(|cell| wrap(cell, widths[column]));
            (0..columns)
                .map(|column| cell(column).unwrap_or_default())
                .collect()
        })
        .collect();
    // Rows of several lines need a border between them to be told apart.
    let tall = rows.iter().flatten().any(|cell| cell.len() > 1);

    let border = |left: char, middle: char, right: char| -> Line {
        let mut text = String::from(left);
        for (column, column_width) in widths.iter().enumerate() {
            if column > 0 {
                text.push(middle);
            }
            text.push_str(&"─".repeat(column_width + 2));
        }
        text.push(right);
        vec![Span::new(text, subtle())]
    };

    let mut lines = vec![border('┌', '┬', '┐')];
    for (index, row) in rows.iter().enumerate() {
        if index == 1 || (index > 1 && tall) {
            lines.push(border('├', '┼', '┤'));
        }
        let height = row.iter().map(Vec::len).max().unwrap_or(0).max(1);
        for line_index in 0..height {
            let mut line = Line::new();
            for (column, cell) in row.iter().enumerate() {
                let edge = if column == 0 { "│ " } else { " │ " };
                line.push(Span::new(edge, subtle()));

                let content = cell.get(line_index).cloned().unwrap_or_default();
                let space = widths[column].saturating_sub(line_width(&content));
                let before = match table.align.get(column) {
                    Some(AlignKind::Right) => space,
                    Some(AlignKind::Center) => space / 2,
                    _ => 0,
                };
                line.push(Span::plain(" ".repeat(before)));
                line.extend(content);
                line.push(Span::plain(" ".repeat(space - before)));
            }
            line.push(Span::new(" │", subtle()));
            line.retain(|span| !span.text.is_empty());
            lines.push(line);
        }
    }
    lines.push(border('└', '┴', '┘'));
    lines
}

fn line_width(line: &[Span]) -> usize {
    line.iter().map(Span::width).sum()
}

/// What a paragraph is made of, once its markup is turned into styles.
enum Inline {
    Text(Span),
    /// A line break that is part of the text.
    Break,
}

fn inlines(nodes: &[Node], style: &Style) -> Vec<Inline> {
    let mut out = Vec::new();
    collect_inlines(nodes, style, &mut out);
    out
}

fn collect_inlines(nodes: &[Node], style: &Style, out: &mut Vec<Inline>) {
    fn text(out: &mut Vec<Inline>, text: &str, style: Style) {
        out.push(Inline::Text(Span::new(text, style)));
    }

    for node in nodes {
        match node {
            Node::Text(node) => text(out, &node.value, style.clone()),
            Node::InlineCode(node) => text(out, &node.value, style.clone().with_color(CODE_COLOR)),
            Node::InlineMath(node) => text(out, &node.value, style.clone().with_color(CODE_COLOR)),
            Node::Html(node) => text(out, &node.value, style.clone()),
            Node::FootnoteReference(node) => {
                text(out, &format!("[^{}]", node.identifier), style.clone());
            }
            Node::Image(node) => {
                text(out, &format!("[{}]", node.alt), style.clone());
                text(out, &format!(" ({})", node.url), subtle());
            }
            Node::Break(_) => out.push(Inline::Break),
            Node::Strong(node) => collect_inlines(&node.children, &style.clone().bold(), out),
            Node::Emphasis(node) => collect_inlines(&node.children, &style.clone().italic(), out),
            Node::Delete(node) => collect_inlines(&node.children, &style.clone().strike(), out),
            Node::Link(node) => {
                collect_inlines(&node.children, &style.clone().underline(), out);
                // A bare address is its own label.
                let label: String = node.children.iter().map(ToString::to_string).collect();
                if label != node.url {
                    text(out, &format!(" ({})", node.url), subtle());
                }
            }
            // Reference links and images, and whatever else has text.
            other => match other.children() {
                Some(children) => collect_inlines(children, style, out),
                None => text(out, &other.to_string(), style.clone()),
            },
        }
    }
}

/// A run of text without spaces, in one or more styles.
#[derive(Default)]
struct Word {
    spans: Vec<Span>,
    width: usize,
    /// The style of the space before the word, when there is one.
    gap: Option<Style>,
}

enum Piece {
    Word(Word),
    Break,
}

/// Splits text at its spaces and line breaks, which are all the same in
/// Markdown: a place where a line may end.
fn words(inlines: &[Inline]) -> Vec<Piece> {
    fn end_word(word: &mut Word, gap: &mut Option<Style>, pieces: &mut Vec<Piece>) {
        if !word.spans.is_empty() {
            word.gap = gap.take();
            pieces.push(Piece::Word(std::mem::take(word)));
        }
    }

    let mut pieces = Vec::new();
    let mut word = Word::default();
    let mut gap = None;
    for inline in inlines {
        match inline {
            Inline::Break => {
                end_word(&mut word, &mut gap, &mut pieces);
                gap = None;
                pieces.push(Piece::Break);
            }
            Inline::Text(span) => {
                for c in span.text.chars() {
                    if c.is_whitespace() {
                        end_word(&mut word, &mut gap, &mut pieces);
                        gap.get_or_insert_with(|| span.style.clone());
                    } else {
                        push_char(&mut word.spans, c, &span.style);
                        word.width += c.width().unwrap_or(0);
                    }
                }
            }
        }
    }
    end_word(&mut word, &mut gap, &mut pieces);
    pieces
}

/// Adds `c` to the last span when it has the same style, else as a new one.
fn push_char(spans: &mut Vec<Span>, c: char, style: &Style) {
    match spans.last_mut() {
        Some(last) if last.style == *style => last.text.push(c),
        _ => spans.push(Span::new(c, style.clone())),
    }
}

/// Fills lines of at most `width` columns with the words of `inlines`. A
/// word longer than a line is cut at the edge.
fn wrap(inlines: &[Inline], width: usize) -> Vec<Line> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = Line::new();
    let mut used = 0;
    for piece in words(inlines) {
        let word = match piece {
            Piece::Word(word) => word,
            Piece::Break => {
                lines.push(std::mem::take(&mut line));
                used = 0;
                continue;
            }
        };
        if used > 0 {
            if used + 1 + word.width <= width {
                push_char(&mut line, ' ', &word.gap.unwrap_or_default());
                used += 1;
            } else {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
        }
        for span in &word.spans {
            for c in span.text.chars() {
                let c_width = c.width().unwrap_or(0);
                if used > 0 && used + c_width > width {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                push_char(&mut line, c, &span.style);
                used += c_width;
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use richrs::style::Style;

    use super::{CODE_COLOR, Line, MarkdownStream, render};

    /// The text of `lines`, without styles.
    fn plain(lines: &[Line]) -> String {
        let texts: Vec<String> = lines
            .iter()
            .map(|line| line.iter().map(|span| span.text.as_str()).collect())
            .collect();
        texts.join("\n")
    }

    fn rendered(source: &str, width: usize) -> String {
        plain(&render(source, width))
    }

    /// The text that streaming `chunks` gives, with a `|` line wherever a
    /// `push` returned lines: what was printed at each step.
    fn streamed(chunks: &[&str], width: usize) -> String {
        let mut stream = MarkdownStream::default();
        let mut steps: Vec<String> = chunks
            .iter()
            .map(|chunk| plain(&stream.push(chunk, width)))
            .collect();
        steps.push(plain(&stream.finish(width)));
        steps.retain(|step| !step.is_empty());
        steps.join("\n|\n")
    }

    #[test]
    fn paragraphs_are_wrapped_and_set_apart() {
        assert_eq!(
            rendered("Many nodes fail.\nStill, it holds.\n\nSecond.", 12),
            "Many nodes\nfail. Still,\nit holds.\n\nSecond."
        );
    }

    #[test]
    fn a_word_longer_than_the_line_is_cut() {
        assert_eq!(
            rendered("see abcdefghij now", 4),
            "see\nabcd\nefgh\nij\nnow"
        );
    }

    #[test]
    fn wide_characters_count_two_columns() {
        assert_eq!(rendered("日本語 のテキスト", 8), "日本語\nのテキス\nト");
    }

    #[test]
    fn inline_markup_becomes_styles() {
        let lines = render("a **b** *c* ~~d~~ `e` [f](http://g)", 80);
        let styles: Vec<(&str, Style)> = lines[0]
            .iter()
            .map(|span| (span.text.as_str(), span.style.clone()))
            .collect();
        assert_eq!(
            styles,
            [
                ("a ", Style::new()),
                ("b", Style::new().bold()),
                (" ", Style::new()),
                ("c", Style::new().italic()),
                (" ", Style::new()),
                ("d", Style::new().strike()),
                (" ", Style::new()),
                ("e", Style::color(CODE_COLOR)),
                (" ", Style::new()),
                ("f", Style::new().underline()),
                (" (http://g)", Style::new().dim()),
            ]
        );
    }

    #[test]
    fn a_bare_address_is_not_repeated() {
        assert_eq!(
            rendered("See <https://example.com>.", 80),
            "See https://example.com."
        );
    }

    #[test]
    fn lists_nest_and_number() {
        let source = "\
- one
  - inner item that is long
- [x] done
- [ ] to do

3. three
4. four";
        assert_eq!(
            rendered(source, 18),
            "\
• one
  • inner item
    that is long
[x] done
[ ] to do

3. three
4. four"
        );
    }

    #[test]
    fn quotes_code_and_rules() {
        let source =
            "> quoted text\n> goes on\n\n```rust\nfn main() {\n\n    run();\n}\n```\n\n---";
        assert_eq!(
            rendered(source, 10),
            "\
│ quoted
│ text
│ goes on

  fn main() {

      run();
  }

──────────"
        );
    }

    #[test]
    fn headings_keep_their_text() {
        assert_eq!(
            rendered("# Title\n\n### Sub *title*", 80),
            "Title\n\nSub title"
        );
    }

    #[test]
    fn tables_get_borders_and_alignment() {
        let source = "\
| Language | Year | Typing |
|:---------|-----:|:------:|
| Rust | 2015 | static |
| Python | 1991 | dynamic |";
        assert_eq!(
            rendered(source, 80),
            "\
┌──────────┬──────┬─────────┐
│ Language │ Year │ Typing  │
├──────────┼──────┼─────────┤
│ Rust     │ 2015 │ static  │
│ Python   │ 1991 │ dynamic │
└──────────┴──────┴─────────┘"
        );
    }

    #[test]
    fn a_table_too_wide_wraps_its_widest_cells() {
        let source = "\
| Name | Notes |
|------|-------|
| Rust | Fast and safe, without a garbage collector |
| Go | Simple |";
        assert_eq!(
            rendered(source, 30),
            "\
┌──────┬─────────────────────┐
│ Name │ Notes               │
├──────┼─────────────────────┤
│ Rust │ Fast and safe,      │
│      │ without a garbage   │
│      │ collector           │
├──────┼─────────────────────┤
│ Go   │ Simple              │
└──────┴─────────────────────┘"
        );
    }

    #[test]
    fn table_columns_fit_wide_characters_and_missing_cells() {
        let source = "| 名前 | n |\n|---|---|\n| 日本語 |\n| a | **1** |";
        assert_eq!(
            rendered(source, 80),
            "\
┌────────┬───┐
│ 名前   │ n │
├────────┼───┤
│ 日本語 │   │
│ a      │ 1 │
└────────┴───┘"
        );
    }

    #[test]
    fn a_paragraph_is_given_at_its_blank_line() {
        // Not before: its next line could go on, or make it a table.
        assert_eq!(
            streamed(
                &["First ", "paragraph.\n", "\n", "Sec", "ond.\n\n# End"],
                80
            ),
            "First paragraph.\n|\n\nSecond.\n|\n\nEnd"
        );
    }

    #[test]
    fn a_heading_is_given_at_the_end_of_its_line() {
        assert_eq!(
            streamed(&["# Ti", "tle\n", "Text right under\n"], 80),
            "Title\n|\n\nText right under"
        );
    }

    #[test]
    fn a_table_is_given_whole() {
        let chunks = [
            "Intro\n\n| a | b |\n",
            "|---|---|\n",
            "| 1 | 2 |\n| 3 ",
            "| 4 |\n",
            "\nDone",
        ];
        assert_eq!(
            streamed(&chunks, 80),
            "\
Intro
|

┌───┬───┐
│ a │ b │
├───┼───┤
│ 1 │ 2 │
│ 3 │ 4 │
└───┴───┘
|

Done"
        );
    }

    #[test]
    fn list_items_are_given_one_by_one() {
        let chunks = ["1. one\n", "2. two\n   more\n", "3. thr", "ee\n", "\nAfter"];
        assert_eq!(
            streamed(&chunks, 80),
            "1. one\n|\n2. two more\n|\n3. three\n|\n\nAfter"
        );
    }

    #[test]
    fn a_list_ends_when_other_text_starts_after_a_blank_line() {
        // The last item does not wait for the line after the list to end.
        assert_eq!(
            streamed(&["- a\n- b\n", "\n", "T", "hen more"], 80),
            "• a\n|\n• b\n|\n\nThen more"
        );
        // A digit could be the next item, so the list waits for its line.
        assert_eq!(
            streamed(&["1. a\n2. b\n", "\n", "3", "0 years\n"], 80),
            "1. a\n|\n2. b\n|\n\n30 years"
        );
    }

    #[test]
    fn half_a_line_is_not_read() {
        // "2." alone would be a second item.
        assert_eq!(
            streamed(&["1. about\n2.", "5 million\n"], 80),
            "1. about 2.5 million"
        );
    }

    #[test]
    fn code_lines_are_given_as_they_end() {
        let chunks = [
            "Run:\n\n```sh\n",
            "cargo ",
            "build\ncargo",
            " test\n``",
            "`\n",
            "\nDone",
        ];
        assert_eq!(
            streamed(&chunks, 80),
            "Run:\n|\n\n  cargo build\n|\n  cargo test\n|\n\nDone"
        );
    }

    #[test]
    fn an_unfinished_text_is_given_at_the_end() {
        assert_eq!(
            streamed(&["```\nlet a", " = 1;\nlet b"], 80),
            "  let a = 1;\n|\n  let b"
        );
        assert_eq!(streamed(&["  \n", "\n"], 80), "");
    }

    /// Streaming must give the same lines as rendering at once, however the
    /// text is cut into chunks.
    #[test]
    fn streaming_matches_rendering_at_once() {
        let source = "\
# Título con *énfasis*

Un párrafo con **negrita**, `código` y un [enlace](https://example.com) que
sigue en otra línea.

- uno
- dos
  - anidado con texto largo que hay que partir
  1. numerado
- tres

| Lenguaje | Año | Notas |
|----------|----:|-------|
| Rust | 2015 | Sin recolector de basura |
| 日本語 | 1 | ancho doble |

```rust
fn main() {

    println!(\"hola\");
}
```

> una cita
> en dos líneas

1. primero

   con dos párrafos
2. segundo

---

Final.";
        for width in [24, 40, 100] {
            let expected = rendered(source, width);
            for size in [1, 2, 3, 7, 16, 64] {
                let chars: Vec<char> = source.chars().collect();
                let chunks: Vec<String> = chars
                    .chunks(size)
                    .map(|chunk| chunk.iter().collect())
                    .collect();
                let mut stream = MarkdownStream::default();
                let mut lines = Vec::new();
                for chunk in &chunks {
                    lines.extend(stream.push(chunk, width));
                }
                lines.extend(stream.finish(width));
                assert_eq!(plain(&lines), expected, "width {width}, chunks of {size}");
            }
        }
    }
}
