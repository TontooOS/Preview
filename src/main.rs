//! Preview: TontooOS document viewer built with TontooUI.
//!
//! Empty state with a centered open button; text files (`.txt`, `.py`,
//! `.json`, ...) open as editable text with line numbers; Markdown gets
//! a rendered preview plus raw edit mode; PDFs open read-only with page
//! navigation, page indicator, zoom and fit width; images open read-only
//! as pictures with zoom and fit window; audio files open read-only in a
//! compact player with play/pause, seek and volume. Saving is manual only
//! (`Save`, `Ctrl+S`, close dialog). Opens via CLI (`preview /path/to/file`)
//! or the native file dialog. Follows the live system color scheme
//! (Dark `#1d1d1d`, Light `#ececec`).

mod docx;
mod lang;
mod markdown;
mod model;
mod views;

sdk::preinclude!();

use UIKit::prelude::*;

struct PreviewDelegate {
  initial: Option<std::path::PathBuf>,
}

impl AppDelegate for PreviewDelegate {
  fn view(&self) -> Box<dyn Widget> {
    Box::new(views::preview::PreviewRoot::new(self.initial.clone()))
  }
}

fn main() {
  lang::init();
  let args: Vec<String> = std::env::args().collect();
  let initial = model::initial_file_from_args(&args);
  let title = match initial.as_ref() {
    Some(path) => model::display_name(path),
    None => lang::t("app.title"),
  };
  let mut app = App::with_delegate(title, 960, 640, PreviewDelegate { initial });
  app.auto_color_scheme();
  app.run();
}
