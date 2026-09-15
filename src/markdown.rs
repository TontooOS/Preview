//! Markdown renderer for the Preview basis.
//!
//! Supports headings (`#`..`######`), blockquotes (`>`, nested `>>`),
//! unordered (`-`, `*`, `+`, nested by indent) and ordered (`1.`, `1)`)
//! lists, task lists (`- [ ]`, `- [x]`), tables (header plus rows, the
//! separator row is skipped), horizontal rules (`---`, `***`, `___`),
//! definition lines (`: ...`), footnote definitions (`[^id]: ...`),
//! fenced code blocks, backslash escapes (`\#`, `\*`, `\_`, `\|`, ...)
//! and inline `**bold**`, `__bold__`, `*italic*`, `_italic_`,
//! `***bold+italic***`, `~~strikethrough~~`, `` `code` ``, `[text](url)`,
//! `![alt](url)`, `<autolink>` and `[^id]` footnote refs. Renders into a
//! read-only `GtkTextView` with text tags (larger bold headings,
//! monospace code).

use gtk::prelude::*;

/// Build tags on a buffer and insert rendered markdown. The buffer is
/// cleared first. Used for the Markdown preview mode (read-only).
pub fn render_into(buffer: &gtk::TextBuffer, source: &str) {
  ensure_tags(buffer);
  buffer.set_text("");
  let mut iter = buffer.start_iter();
  let lines: Vec<&str> = source.lines().collect();
  let mut i = 0;
  let mut in_code_block = false;

  while i < lines.len() {
    let raw_line = lines[i];
    let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
    if line.trim_start().starts_with("```") {
      in_code_block = !in_code_block;
      i += 1;
      continue;
    }
    if in_code_block {
      insert_with(&mut iter, &format!("{line}\n"), &["mono", "code_bg"]);
      i += 1;
      continue;
    }
    // Table: header row plus separator row, then body rows.
    if let Some(header) = table_cells(line) {
      if i + 1 < lines.len() && is_table_sep(lines[i + 1]) {
        insert_table_row(&mut iter, &header, true);
        i += 2;
        while i < lines.len() {
          let body = lines[i].strip_suffix('\r').unwrap_or(lines[i]);
          match table_cells(body) {
            Some(cells) => {
              insert_table_row(&mut iter, &cells, false);
              i += 1;
            }
            None => break,
          }
        }
        continue;
      }
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
      insert_with(&mut iter, "\n", &[]);
      i += 1;
      continue;
    }
    // An escaped line (e.g. `\---`) is literal text, never a rule.
    if !trimmed.contains('\\') && is_hr(trimmed) {
      insert_with(&mut iter, "────────────────\n", &["dim"]);
      i += 1;
      continue;
    }
    if let Some((level, rest)) = heading(trimmed) {
      let tag: &'static str = match level {
        1 => "h1",
        2 => "h2",
        3 => "h3",
        4 => "h4",
        5 => "h5",
        _ => "h6",
      };
      insert_spans(&mut iter, &parse_inline(rest, &[tag]));
      insert_with(&mut iter, "\n", &[]);
      i += 1;
      continue;
    }
    let (_depth, rest) = strip_quotes(trimmed);
    if rest.len() != trimmed.len() {
      insert_spans(&mut iter, &parse_inline(rest, &["quote", "italic"]));
      insert_with(&mut iter, "\n", &[]);
      i += 1;
      continue;
    }
    if let Some(item) = list_item(line) {
      let indent: String = "  ".repeat(item.depth.min(8));
      match &item.marker {
        Marker::Bullet => insert_with(&mut iter, &format!("{indent}• "), &["dim"]),
        Marker::Ordered(num) => insert_with(&mut iter, &format!("{indent}{num}. "), &["dim"]),
        Marker::Task(done) => {
          let box_glyph = if *done { "☑" } else { "☐" };
          insert_with(&mut iter, &format!("{indent}{box_glyph} "), &["dim"]);
        }
      }
      insert_spans(&mut iter, &parse_inline(&item.content, &[]));
      insert_with(&mut iter, "\n", &[]);
      i += 1;
      continue;
    }
    if let Some(rest) = trimmed.strip_prefix(": ") {
      insert_with(&mut iter, "    ", &["dim"]);
      insert_spans(&mut iter, &parse_inline(rest, &[]));
      insert_with(&mut iter, "\n", &[]);
      i += 1;
      continue;
    }
    if let Some((id, rest)) = footnote_def(trimmed) {
      insert_with(&mut iter, &format!("[^{id}] "), &["dim"]);
      insert_spans(&mut iter, &parse_inline(rest, &[]));
      insert_with(&mut iter, "\n", &[]);
      i += 1;
      continue;
    }
    insert_spans(&mut iter, &parse_inline(trimmed, &[]));
    insert_with(&mut iter, "\n", &[]);
    i += 1;
  }
}

fn ensure_tags(buffer: &gtk::TextBuffer) {
  let table = buffer.tag_table();
  let ensure = |name: &str, props: &[(&str, glib::Value)]| {
    if table.lookup(name).is_none() {
      let tag = gtk::TextTag::new(Some(name));
      for (prop, value) in props {
        tag.set_property(*prop, value);
      }
      table.add(&tag);
    }
  };
  ensure("h1", &[("weight", 700.into()), ("scale", 1.6.into())]);
  ensure("h2", &[("weight", 700.into()), ("scale", 1.35.into())]);
  ensure("h3", &[("weight", 700.into()), ("scale", 1.15.into())]);
  ensure("h4", &[("weight", 700.into()), ("scale", 1.0.into())]);
  ensure("h5", &[("weight", 700.into()), ("scale", 0.9.into())]);
  ensure("h6", &[("weight", 700.into()), ("scale", 0.85.into())]);
  ensure("bold", &[("weight", 700.into())]);
  ensure("italic", &[("style", gtk::pango::Style::Italic.into())]);
  ensure("strike", &[("strikethrough", true.into())]);
  ensure(
    "link",
    &[("underline", gtk::pango::Underline::Single.into())],
  );
  ensure("mono", &[("family", "Monospace".into())]);
  ensure("code_bg", &[("background", "#00000022".into())]);
  ensure("dim", &[("foreground", "#888888".into())]);
  ensure(
    "quote",
    &[("foreground", "#9a9a9a".into()), ("left-margin", 16.into())],
  );
}

fn insert_with(iter: &mut gtk::TextIter, text: &str, tags: &[&str]) {
  let buffer = iter.buffer();
  if tags.is_empty() {
    buffer.insert(iter, text);
  } else {
    let owned: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    buffer.insert_with_tags_by_name(iter, text, &refs);
  }
}

fn insert_spans(iter: &mut gtk::TextIter, spans: &[Span<'_>]) {
  let buffer = iter.buffer();
  for span in spans {
    if span.tags.is_empty() {
      buffer.insert(iter, &span.text);
    } else {
      let refs: Vec<&str> = span.tags.iter().copied().collect();
      buffer.insert_with_tags_by_name(iter, &span.text, &refs);
    }
  }
}

fn insert_table_row(iter: &mut gtk::TextIter, cells: &[String], header: bool) {
  for (index, cell) in cells.iter().enumerate() {
    if index > 0 {
      insert_with(iter, " | ", &["dim", "mono"]);
    }
    if header {
      insert_spans(iter, &parse_inline(cell, &["mono", "bold"]));
    } else {
      insert_spans(iter, &parse_inline(cell, &["mono"]));
    }
  }
  insert_with(iter, "\n", &[]);
}

// ---------------------------------------------------------------------------
// Block helpers (pure, unit-tested).
// ---------------------------------------------------------------------------

/// `#`..`######` plus required space. An escaped `\#` never matches.
fn heading(line: &str) -> Option<(usize, &str)> {
  let mut level = 0;
  for c in line.chars() {
    if c == '#' && level < 6 {
      level += 1;
    } else {
      break;
    }
  }
  if level == 0 {
    return None;
  }
  let rest = &line[level..];
  if rest.is_empty() {
    return Some((level, ""));
  }
  let content = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('\t'))?;
  Some((level, content.trim_start()))
}

/// Strip leading `>` markers (each with one optional space). Returns
/// `(stripped_something, rest)`; callers compare lengths.
fn strip_quotes(line: &str) -> (usize, &str) {
  let mut rest = line.trim_start();
  let mut depth = 0;
  while let Some(after) = rest.strip_prefix('>') {
    depth += 1;
    rest = after.strip_prefix(' ').unwrap_or(after);
  }
  (depth, rest)
}

#[derive(Debug, PartialEq, Eq)]
enum Marker {
  Bullet,
  Ordered(String),
  Task(bool),
}

#[derive(Debug, PartialEq, Eq)]
struct ListItem {
  depth: usize,
  marker: Marker,
  content: String,
}

/// Unordered (`-`, `*`, `+`), ordered (`1.`, `1)`) and task (`- [ ]`,
/// `- [x]`) items. Two leading spaces add one nesting level. An escaped
/// marker (`\*`) never matches.
fn list_item(line: &str) -> Option<ListItem> {
  let mut width = 0;
  let mut bytes = 0;
  for c in line.chars() {
    if c == ' ' {
      width += 1;
      bytes += c.len_utf8();
    } else if c == '\t' {
      width += 4;
      bytes += c.len_utf8();
    } else {
      break;
    }
  }
  let rest = &line[bytes..];
  let depth = width / 2;
  for marker in ["- ", "* ", "+ "] {
    if let Some(after) = rest.strip_prefix(marker) {
      if let Some((done, content)) = parse_task(after) {
        return Some(ListItem {
          depth,
          marker: Marker::Task(done),
          content: content.to_string(),
        });
      }
      return Some(ListItem {
        depth,
        marker: Marker::Bullet,
        content: after.to_string(),
      });
    }
  }
  let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
  if !digits.is_empty() {
    let after = &rest[digits.len()..];
    if let Some(content) = after
      .strip_prefix(". ")
      .or_else(|| after.strip_prefix(") "))
    {
      if let Some((done, task_content)) = parse_task(content) {
        return Some(ListItem {
          depth,
          marker: Marker::Task(done),
          content: task_content.to_string(),
        });
      }
      return Some(ListItem {
        depth,
        marker: Marker::Ordered(digits),
        content: content.to_string(),
      });
    }
  }
  None
}

fn parse_task(text: &str) -> Option<(bool, &str)> {
  if let Some(after) = text.strip_prefix("[ ] ") {
    return Some((false, after));
  }
  if let Some(after) = text
    .strip_prefix("[x] ")
    .or_else(|| text.strip_prefix("[X] "))
  {
    return Some((true, after));
  }
  None
}

/// `---`, `***`, `___` (spaces allowed). Callers skip escaped lines.
fn is_hr(line: &str) -> bool {
  let compact: String = line.chars().filter(|c| *c != ' ').collect();
  compact.len() >= 3
    && compact
      .chars()
      .all(|c| c == '-' || c == '*' || c == '_')
}

/// Split a table row on unescaped pipes, unescaping cells. Returns `None`
/// when the line has no unescaped pipe.
fn table_cells(line: &str) -> Option<Vec<String>> {
  let chars: Vec<char> = line.chars().collect();
  if !chars
    .iter()
    .enumerate()
    .any(|(pos, c)| *c == '|' && !is_escaped(&chars, pos))
  {
    return None;
  }
  let mut cells = Vec::new();
  let mut current = String::new();
  let mut i = 0;
  while i < chars.len() {
    if chars[i] == '\\' && i + 1 < chars.len() && is_escapable(chars[i + 1]) {
      current.push(chars[i + 1]);
      i += 2;
      continue;
    }
    if chars[i] == '|' {
      cells.push(current.trim().to_string());
      current = String::new();
      i += 1;
      continue;
    }
    current.push(chars[i]);
    i += 1;
  }
  cells.push(current.trim().to_string());
  if cells.first().is_some_and(|s| s.is_empty()) {
    cells.remove(0);
  }
  if cells.last().is_some_and(|s| s.is_empty()) {
    cells.pop();
  }
  if cells.is_empty() {
    return None;
  }
  Some(cells)
}

/// Separator row: every cell is dashes with optional edge colons.
fn is_table_sep(line: &str) -> bool {
  let cells = match table_cells(line) {
    Some(cells) => cells,
    None => return false,
  };
  if cells.is_empty() {
    return false;
  }
  cells.iter().all(|cell| {
    let inner = cell.trim();
    let inner = inner.strip_prefix(':').unwrap_or(inner);
    let inner = inner.strip_suffix(':').unwrap_or(inner);
    !inner.is_empty() && inner.chars().all(|c| c == '-')
  })
}

/// `[^id]: text` footnote definition.
fn footnote_def(line: &str) -> Option<(&str, &str)> {
  let rest = line.strip_prefix("[^")?;
  let end = rest.find("]:")?;
  let (id, after) = rest.split_at(end);
  if id.is_empty() {
    return None;
  }
  Some((id, after[2..].trim_start()))
}

/// Odd number of preceding backslashes means `pos` is escaped.
fn is_escaped(chars: &[char], pos: usize) -> bool {
  let mut count = 0;
  let mut p = pos;
  while p > 0 {
    p -= 1;
    if chars[p] == '\\' {
      count += 1;
    } else {
      break;
    }
  }
  count % 2 == 1
}

fn is_escapable(c: char) -> bool {
  c.is_ascii_punctuation()
}

// ---------------------------------------------------------------------------
// Inline parser (pure, unit-tested).
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct Span<'a> {
  text: String,
  tags: Vec<&'a str>,
}

fn flush<'a>(out: &mut Vec<Span<'a>>, plain: &mut String, base: &[&'a str]) {
  if !plain.is_empty() {
    out.push(Span {
      text: std::mem::take(plain),
      tags: base.to_vec(),
    });
  }
}

fn with_tags<'a>(base: &[&'a str], extra: &[&'a str]) -> Vec<&'a str> {
  let mut tags = base.to_vec();
  tags.extend_from_slice(extra);
  tags
}

/// Inline spans: escapes, `` `code` ``, images, links, footnote refs,
/// autolinks, `***both***`, `**bold**`, `__bold__`, `~~strike~~`,
/// `*italic*`, `_italic_`. Unmatched markers stay literal.
fn parse_inline<'a>(line: &str, base: &[&'a str]) -> Vec<Span<'a>> {
  let chars: Vec<char> = line.chars().collect();
  let len = chars.len();
  let mut out: Vec<Span<'a>> = Vec::new();
  let mut plain = String::new();
  let mut i = 0;
  while i < len {
    let c = chars[i];
    // Backslash escape: `\<punct>` is literal.
    if c == '\\' && i + 1 < len && is_escapable(chars[i + 1]) {
      plain.push(chars[i + 1]);
      i += 2;
      continue;
    }
    // Code span (content stays literal).
    if c == '`' {
      if let Some(end) = find_single(&chars, i + 1, '`') {
        flush(&mut out, &mut plain, base);
        out.push(Span {
          text: chars[i + 1..end].iter().collect(),
          tags: with_tags(base, &["mono", "code_bg"]),
        });
        i = end + 1;
        continue;
      }
    }
    // Image `![alt](url)`.
    if c == '!' && i + 1 < len && chars[i + 1] == '[' {
      if let Some((label, url, next)) = read_link(&chars, i + 1) {
        flush(&mut out, &mut plain, base);
        out.extend(parse_inline(&label, &with_tags(base, &["bold"])));
        out.push(Span {
          text: format!(" ({url})"),
          tags: with_tags(base, &["dim"]),
        });
        i = next;
        continue;
      }
    }
    // Link `[text](url)` or footnote ref `[^id]`.
    if c == '[' {
      if chars.get(i + 1) == Some(&'^') {
        if let Some(end) = find_single(&chars, i + 2, ']') {
          if chars.get(end + 1) != Some(&'(') {
            flush(&mut out, &mut plain, base);
            out.push(Span {
              text: chars[i..=end].iter().collect(),
              tags: with_tags(base, &["dim"]),
            });
            i = end + 1;
            continue;
          }
        }
      }
      if let Some((label, url, next)) = read_link(&chars, i) {
        flush(&mut out, &mut plain, base);
        out.extend(parse_inline(&label, &with_tags(base, &["bold"])));
        out.push(Span {
          text: format!(" ({url})"),
          tags: with_tags(base, &["dim"]),
        });
        i = next;
        continue;
      }
    }
    // Autolink `<https://...>` / `<mail@host>`.
    if c == '<' {
      if let Some(end) = find_single(&chars, i + 1, '>') {
        let inner: String = chars[i + 1..end].iter().collect();
        if !inner.is_empty()
          && !inner.chars().any(|x| x.is_whitespace())
          && (inner.contains("://") || inner.contains('@'))
        {
          flush(&mut out, &mut plain, base);
          out.push(Span {
            text: inner,
            tags: with_tags(base, &["link"]),
          });
          i = end + 1;
          continue;
        }
      }
    }
    // `***both***` (opener without closer stays literal).
    if c == '*' && i + 2 < len && chars[i + 1] == '*' && chars[i + 2] == '*' {
      if let Some(end) = find_marker(&chars, i + 3, "***") {
        flush(&mut out, &mut plain, base);
        let inner: String = chars[i + 3..end].iter().collect();
        out.extend(parse_inline(&inner, &with_tags(base, &["bold", "italic"])));
        i = end + 3;
      } else {
        plain.push_str("***");
        i += 3;
      }
      continue;
    }
    // `**bold**` (opener without closer stays literal).
    if c == '*' && i + 1 < len && chars[i + 1] == '*' {
      if let Some(end) = find_marker(&chars, i + 2, "**") {
        flush(&mut out, &mut plain, base);
        let inner: String = chars[i + 2..end].iter().collect();
        out.extend(parse_inline(&inner, &with_tags(base, &["bold"])));
        i = end + 2;
      } else {
        plain.push_str("**");
        i += 2;
      }
      continue;
    }
    // `__bold__` (not inside words; opener without closer stays literal).
    if c == '_' && i + 1 < len && chars[i + 1] == '_' && underscore_opener(&chars, i) {
      if let Some(end) = find_marker(&chars, i + 2, "__") {
        flush(&mut out, &mut plain, base);
        let inner: String = chars[i + 2..end].iter().collect();
        out.extend(parse_inline(&inner, &with_tags(base, &["bold"])));
        i = end + 2;
      } else {
        plain.push_str("__");
        i += 2;
      }
      continue;
    }
    // `~~strike~~` (opener without closer stays literal).
    if c == '~' && i + 1 < len && chars[i + 1] == '~' {
      if let Some(end) = find_marker(&chars, i + 2, "~~") {
        flush(&mut out, &mut plain, base);
        let inner: String = chars[i + 2..end].iter().collect();
        out.extend(parse_inline(&inner, &with_tags(base, &["strike"])));
        i = end + 2;
      } else {
        plain.push_str("~~");
        i += 2;
      }
      continue;
    }
    // `*italic*`.
    if c == '*' {
      if let Some(end) = find_single_star(&chars, i + 1) {
        flush(&mut out, &mut plain, base);
        let inner: String = chars[i + 1..end].iter().collect();
        out.extend(parse_inline(&inner, &with_tags(base, &["italic"])));
        i = end + 1;
        continue;
      }
    }
    // `_italic_` (not inside words, so `say_hello` stays literal).
    if c == '_' && underscore_opener(&chars, i) {
      if let Some(end) = find_single_underscore(&chars, i + 1) {
        flush(&mut out, &mut plain, base);
        let inner: String = chars[i + 1..end].iter().collect();
        out.extend(parse_inline(&inner, &with_tags(base, &["italic"])));
        i = end + 1;
        continue;
      }
    }
    plain.push(c);
    i += 1;
  }
  flush(&mut out, &mut plain, base);
  out
}

/// Next unescaped single marker char.
fn find_single(chars: &[char], from: usize, marker: char) -> Option<usize> {
  let mut j = from;
  while j < chars.len() {
    if chars[j] == '\\' && j + 1 < chars.len() {
      j += 2;
      continue;
    }
    if chars[j] == marker {
      return Some(j);
    }
    j += 1;
  }
  None
}

/// Next unescaped multi-char marker (`**`, `__`, `~~`, `***`).
fn find_marker(chars: &[char], from: usize, marker: &str) -> Option<usize> {
  let wanted: Vec<char> = marker.chars().collect();
  let mut j = from;
  while j + wanted.len() <= chars.len() {
    if chars[j] == '\\' {
      j += 2;
      continue;
    }
    if chars[j..].starts_with(&wanted[..]) {
      return Some(j);
    }
    j += 1;
  }
  None
}

/// Next `*` that is not part of a `**` pair.
fn find_single_star(chars: &[char], from: usize) -> Option<usize> {
  let mut j = from;
  while j < chars.len() {
    if chars[j] == '\\' && j + 1 < chars.len() {
      j += 2;
      continue;
    }
    if chars[j] == '*' {
      if j + 1 < chars.len() && chars[j + 1] == '*' {
        j += 2;
        continue;
      }
      return Some(j);
    }
    j += 1;
  }
  None
}

/// Next `_` that can close italics: not part of `__`, next char is not
/// alphanumeric, previous char is not whitespace.
fn find_single_underscore(chars: &[char], from: usize) -> Option<usize> {
  let mut j = from;
  while j < chars.len() {
    if chars[j] == '\\' && j + 1 < chars.len() {
      j += 2;
      continue;
    }
    if chars[j] == '_' {
      if j + 1 < chars.len() && chars[j + 1] == '_' {
        j += 2;
        continue;
      }
      let next_ok = j + 1 >= chars.len() || !chars[j + 1].is_alphanumeric();
      let prev_ok = j > 0 && chars[j - 1] != ' ' && chars[j - 1] != '\t';
      if next_ok && prev_ok {
        return Some(j);
      }
      j += 1;
      continue;
    }
    j += 1;
  }
  None
}

/// `_`/`__` only opens outside words: previous char is not alphanumeric,
/// next char exists and is not whitespace.
fn underscore_opener(chars: &[char], pos: usize) -> bool {
  let prev_ok = pos == 0 || !chars[pos - 1].is_alphanumeric();
  let next_ok = pos + 1 < chars.len() && chars[pos + 1] != ' ' && chars[pos + 1] != '\t';
  prev_ok && next_ok
}

/// `[label](url)` starting at `bracket`. No nesting, no titles.
fn read_link(chars: &[char], bracket: usize) -> Option<(String, String, usize)> {
  let mut j = bracket + 1;
  let mut label = String::new();
  loop {
    if j >= chars.len() {
      return None;
    }
    if chars[j] == '\\' && j + 1 < chars.len() && is_escapable(chars[j + 1]) {
      label.push(chars[j + 1]);
      j += 2;
      continue;
    }
    if chars[j] == ']' {
      break;
    }
    if chars[j] == '[' {
      return None;
    }
    label.push(chars[j]);
    j += 1;
  }
  if label.is_empty() || j + 1 >= chars.len() || chars[j + 1] != '(' {
    return None;
  }
  let mut k = j + 2;
  let mut url = String::new();
  loop {
    if k >= chars.len() {
      return None;
    }
    if chars[k] == '\\' && k + 1 < chars.len() && is_escapable(chars[k + 1]) {
      url.push(chars[k + 1]);
      k += 2;
      continue;
    }
    if chars[k] == ')' {
      break;
    }
    if chars[k] == ' ' || chars[k] == '"' {
      return None;
    }
    url.push(chars[k]);
    k += 1;
  }
  if url.is_empty() {
    return None;
  }
  Some((label, url, k + 1))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn plain(text: &str) -> Span<'static> {
    Span {
      text: text.to_string(),
      tags: vec![],
    }
  }

  fn tagged(text: &str, tags: &[&'static str]) -> Span<'static> {
    Span {
      text: text.to_string(),
      tags: tags.to_vec(),
    }
  }

  #[test]
  fn headings_levels() {
    assert_eq!(heading("# Hi"), Some((1, "Hi")));
    assert_eq!(heading("### Hi"), Some((3, "Hi")));
    assert_eq!(heading("###### Deep"), Some((6, "Deep")));
    assert_eq!(heading("#nospace"), None);
    assert_eq!(heading("\\# escaped"), None);
    assert_eq!(heading("####### too many"), None);
  }

  #[test]
  fn quotes_nesting() {
    assert_eq!(strip_quotes("> hi"), (1, "hi"));
    assert_eq!(strip_quotes("> > nested"), (2, "nested"));
    assert_eq!(strip_quotes("\\> escaped"), (0, "\\> escaped"));
    assert_eq!(strip_quotes("plain"), (0, "plain"));
  }

  #[test]
  fn list_markers() {
    assert_eq!(
      list_item("- item"),
      Some(ListItem {
        depth: 0,
        marker: Marker::Bullet,
        content: "item".to_string(),
      })
    );
    assert_eq!(
      list_item("  * nested"),
      Some(ListItem {
        depth: 1,
        marker: Marker::Bullet,
        content: "nested".to_string(),
      })
    );
    assert_eq!(
      list_item("12. numbered"),
      Some(ListItem {
        depth: 0,
        marker: Marker::Ordered("12".to_string()),
        content: "numbered".to_string(),
      })
    );
    assert_eq!(
      list_item("- [x] done"),
      Some(ListItem {
        depth: 0,
        marker: Marker::Task(true),
        content: "done".to_string(),
      })
    );
    assert_eq!(
      list_item("- [ ] open"),
      Some(ListItem {
        depth: 0,
        marker: Marker::Task(false),
        content: "open".to_string(),
      })
    );
    assert_eq!(list_item("\\* escaped"), None);
    assert_eq!(list_item("*italic*"), None);
  }

  #[test]
  fn rules_and_tables() {
    assert!(is_hr("---"));
    assert!(is_hr("***"));
    assert!(is_hr("* * *"));
    assert!(!is_hr("--"));
    assert_eq!(
      table_cells("| a | b |"),
      Some(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(table_cells("\\| not a table"), None);
    assert!(is_table_sep("|---|---|"));
    assert!(is_table_sep("| :--- | ---: |"));
    assert!(!is_table_sep("| a | b |"));
  }

  #[test]
  fn footnote_definition() {
    assert_eq!(footnote_def("[^1]: note text"), Some(("1", "note text")));
    assert_eq!(footnote_def("[^1] no colon"), None);
  }

  #[test]
  fn inline_emphasis() {
    assert_eq!(parse_inline("**a**", &[]), vec![tagged("a", &["bold"])]);
    assert_eq!(parse_inline("__a__", &[]), vec![tagged("a", &["bold"])]);
    assert_eq!(parse_inline("*a*", &[]), vec![tagged("a", &["italic"])]);
    assert_eq!(parse_inline("_a_", &[]), vec![tagged("a", &["italic"])]);
    assert_eq!(
      parse_inline("***a***", &[]),
      vec![tagged("a", &["bold", "italic"])]
    );
    assert_eq!(
      parse_inline("~~a~~", &[]),
      vec![tagged("a", &["strike"])]
    );
    // Underscores inside words stay literal.
    assert_eq!(parse_inline("say_hello(x)", &[]), vec![plain("say_hello(x)")]);
    // Unmatched markers stay literal.
    assert_eq!(parse_inline("**open", &[]), vec![plain("**open")]);
  }

  #[test]
  fn inline_escapes() {
    assert_eq!(parse_inline("\\*x\\*", &[]), vec![plain("*x*")]);
    assert_eq!(parse_inline("\\# head", &[]), vec![plain("# head")]);
    assert_eq!(parse_inline("a \\| b", &[]), vec![plain("a | b")]);
  }

  #[test]
  fn inline_links_images_refs() {
    assert_eq!(
      parse_inline("[t](https://x)", &[]),
      vec![tagged("t", &["bold"]), tagged(" (https://x)", &["dim"])]
    );
    assert_eq!(
      parse_inline("![alt](https://img)", &[]),
      vec![tagged("alt", &["bold"]), tagged(" (https://img)", &["dim"])]
    );
    assert_eq!(
      parse_inline("[^1]", &[]),
      vec![tagged("[^1]", &["dim"])]
    );
    assert_eq!(
      parse_inline("<https://x>", &[]),
      vec![tagged("https://x", &["link"])]
    );
    assert_eq!(
      parse_inline("`c`", &[]),
      vec![tagged("c", &["mono", "code_bg"])]
    );
  }

  #[test]
  fn inline_nesting() {
    assert_eq!(
      parse_inline("**a *b* c**", &[]),
      vec![
        tagged("a ", &["bold"]),
        tagged("b", &["bold", "italic"]),
        tagged(" c", &["bold"]),
      ]
    );
  }
}
