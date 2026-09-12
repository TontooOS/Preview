//! Minimal Markdown renderer for the Preview basis.
//!
//! Supports headings (`#`..`###`), unordered lists (`-`, `*`), quotes
//! (`>`), code spans, fenced code blocks, bold, italic, links and
//! horizontal rules. Renders into a read-only `GtkTextView` with text
//! tags (SF Pro Display, larger bold headings, monospace code).

use gtk::prelude::*;

/// Build tags on a buffer and insert rendered markdown. The buffer is
/// cleared first. Used for the Markdown preview mode (read-only).
pub fn render_into(buffer: &gtk::TextBuffer, source: &str) {
  ensure_tags(buffer);
  buffer.set_text("");
  let mut iter = buffer.start_iter();
  let mut in_code_block = false;

  for raw_line in source.lines() {
    let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
    if line.trim_start().starts_with("```") {
      in_code_block = !in_code_block;
      continue;
    }
    if in_code_block {
      insert_with(&mut iter, &format!("{line}\n"), &["mono", "code_bg"]);
      continue;
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
      insert_with(&mut iter, "\n", &[]);
      continue;
    }
    if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_' || c == ' ') && trimmed.len() >= 3 {
      insert_with(&mut iter, "────────────────\n", &["dim"]);
      continue;
    }
    if let Some(rest) = trimmed.strip_prefix("### ") {
      insert_inline(&mut iter, rest, &["h3"]);
      insert_with(&mut iter, "\n", &[]);
      continue;
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
      insert_inline(&mut iter, rest, &["h2"]);
      insert_with(&mut iter, "\n", &[]);
      continue;
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
      insert_inline(&mut iter, rest, &["h1"]);
      insert_with(&mut iter, "\n", &[]);
      continue;
    }
    if let Some(rest) = trimmed.strip_prefix("> ") {
      insert_inline(&mut iter, rest, &["quote", "italic"]);
      insert_with(&mut iter, "\n", &[]);
      continue;
    }
    if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
      insert_with(&mut iter, "• ", &["dim"]);
      insert_inline(&mut iter, rest, &[]);
      insert_with(&mut iter, "\n", &[]);
      continue;
    }
    // Ordered list: `1. item`.
    let mut chars = trimmed.chars();
    let mut digits = String::new();
    for c in chars.by_ref() {
      if c.is_ascii_digit() {
        digits.push(c);
      } else {
        break;
      }
    }
    if !digits.is_empty() {
      let rest: String = chars.collect();
      if rest.starts_with(". ") {
        insert_with(&mut iter, &format!("{digits}. "), &["dim"]);
        insert_inline(&mut iter, rest[2..].trim_start(), &[]);
        insert_with(&mut iter, "\n", &[]);
        continue;
      }
    }
    insert_inline(&mut iter, trimmed, &[]);
    insert_with(&mut iter, "\n", &[]);
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
  ensure("bold", &[("weight", 700.into())]);
  ensure("italic", &[("style", 2.into())]);
  ensure("mono", &[("family", "Monospace".into())]);
  ensure("code_bg", &[("background", "#00000022".into())]);
  ensure("dim", &[("foreground", "#888888".into())]);
  ensure("quote", &[("foreground", "#9a9a9a".into()), ("left-margin", 16.into())]);
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

/// Inline spans: `**bold**`, `*italic*`, `` `code` ``, `[text](url)`.
fn insert_inline(iter: &mut gtk::TextIter, line: &str, base: &[&str]) {
  let bytes: Vec<char> = line.chars().collect();
  let mut i = 0;
  let mut plain = String::new();
  let flush = |iter: &mut gtk::TextIter, plain: &mut String| {
    if !plain.is_empty() {
      insert_with(iter, plain, base);
      plain.clear();
    }
  };
  while i < bytes.len() {
    // Link: [text](url).
    if bytes[i] == '[' {
      if let Some(end) = bytes[i..].iter().position(|&c| c == ']') {
        let after = i + end + 1;
        if after < bytes.len() && bytes[after] == '(' {
          if let Some(close) = bytes[after..].iter().position(|&c| c == ')') {
            let label: String = bytes[i + 1..i + end].iter().collect();
            let url: String = bytes[after + 1..after + close].iter().collect();
            flush(iter, &mut plain);
            let mut tags: Vec<&str> = base.to_vec();
            tags.push("bold");
            insert_with(iter, &label, &tags);
            insert_with(iter, &format!(" ({url})"), &["dim"]);
            i = after + close + 1;
            continue;
          }
        }
      }
    }
    // Bold **x**.
    if bytes[i] == '*' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
      if let Some(rel) = find_double(&bytes[i + 2..]) {
        flush(iter, &mut plain);
        let inner: String = bytes[i + 2..i + 2 + rel].iter().collect();
        let mut tags: Vec<&str> = base.to_vec();
        tags.push("bold");
        insert_with(iter, &inner, &tags);
        i += 2 + rel + 2;
        continue;
      }
    }
    // Italic *x*.
    if bytes[i] == '*' {
      if let Some(rel) = bytes[i + 1..].iter().position(|&c| c == '*') {
        flush(iter, &mut plain);
        let inner: String = bytes[i + 1..i + 1 + rel].iter().collect();
        let mut tags: Vec<&str> = base.to_vec();
        tags.push("italic");
        insert_with(iter, &inner, &tags);
        i += 1 + rel + 1;
        continue;
      }
    }
    // Code `x`.
    if bytes[i] == '`' {
      if let Some(rel) = bytes[i + 1..].iter().position(|&c| c == '`') {
        flush(iter, &mut plain);
        let inner: String = bytes[i + 1..i + 1 + rel].iter().collect();
        let mut tags: Vec<&str> = base.to_vec();
        tags.push("mono");
        tags.push("code_bg");
        insert_with(iter, &inner, &tags);
        i += 1 + rel + 1;
        continue;
      }
    }
    plain.push(bytes[i]);
    i += 1;
  }
  flush(iter, &mut plain);
}

fn find_double(slice: &[char]) -> Option<usize> {
  let mut i = 0;
  while i + 1 < slice.len() {
    if slice[i] == '*' && slice[i + 1] == '*' {
      return Some(i);
    }
    i += 1;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn double_star_search() {
    assert_eq!(find_double(&['a', '*', '*']), Some(1));
    assert_eq!(find_double(&['a', 'b']), None);
  }
}
