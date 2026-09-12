//! Tahoe-style Preview root view.
//!
//! Header with `Preview` title plus file name, `Open` / `Edit`-`Done` /
//! `Save` actions. Empty state with a centered open button. Text files
//! render read-only with a rendered Markdown preview mode; edit mode
//! shows raw text with line numbers on the left. Saving is manual only
//! (`Save` button, `Ctrl+S`, close dialog with Cancel / Save / Don't Save).

use crate::lang;
use crate::markdown;
use crate::model::{self, FileKind};
use crate::UIKit::widget::{WidgetId, next_widget_id};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

const SF_PRO: &str = "SF Pro Display";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
  Preview,
  Edit,
}

struct State {
  path: Option<PathBuf>,
  kind: FileKind,
  content: String,
  dirty: bool,
  mode: Mode,
  error: Option<String>,
}

impl State {
  fn empty() -> Self {
    Self {
      path: None,
      kind: FileKind::Text,
      content: String::new(),
      dirty: false,
      mode: Mode::Preview,
      error: None,
    }
  }
}

#[allow(dead_code)]
struct Widgets {
  title: gtk::Label,
  subtitle: gtk::Label,
  edit_btn: gtk::Button,
  save_btn: gtk::Button,
  status: gtk::Label,
  stack: gtk::Stack,
  text_stack: gtk::Stack,
  edit_view: gtk::TextView,
  edit_buffer: gtk::TextBuffer,
  gutter_view: gtk::TextView,
  gutter_buffer: gtk::TextBuffer,
  edit_scroll: gtk::ScrolledWindow,
  preview_view: gtk::TextView,
  preview_buffer: gtk::TextBuffer,
  unsup_title: gtk::Label,
  unsup_hint: gtk::Label,
  root: gtk::Box,
}

/// Root widget for the Preview app.
pub struct PreviewRoot {
  id: WidgetId,
  initial: Option<PathBuf>,
}

impl PreviewRoot {
  pub fn new(initial: Option<PathBuf>) -> Self {
    Self { id: next_widget_id(), initial }
  }
}

impl crate::UIKit::widget::Widget for PreviewRoot {
  fn id(&self) -> WidgetId {
    self.id
  }

  fn to_gtk(&self) -> gtk::Widget {
    build_ui(self.initial.clone()).upcast()
  }
}

fn build_ui(initial: Option<PathBuf>) -> gtk::Box {
  let state = Rc::new(RefCell::new(State::empty()));

  // Root column: header, content stack, status line.
  let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

  // Header bar: left title + subtitle, right actions.
  let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  header.set_margin_start(16);
  header.set_margin_end(16);
  header.set_margin_top(12);
  header.set_margin_bottom(8);

  let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
  title_box.set_hexpand(true);
  let title = gtk::Label::new(Some(&lang::t("app.title")));
  title.set_halign(gtk::Align::Start);
  title.add_css_class("title-1");
  let subtitle = gtk::Label::new(Some(&lang::t("empty.hint")));
  subtitle.set_halign(gtk::Align::Start);
  subtitle.add_css_class("dim-label");
  title_box.append(&title);
  title_box.append(&subtitle);
  header.append(&title_box);

  let open_btn = gtk::Button::with_label(&lang::t("action.open"));
  open_btn.add_css_class("suggested-action");
  let edit_btn = gtk::Button::with_label(&lang::t("action.edit"));
  let save_btn = gtk::Button::with_label(&lang::t("action.save"));
  save_btn.set_sensitive(false);
  save_btn.set_visible(false);
  edit_btn.set_visible(false);
  header.append(&open_btn);
  header.append(&edit_btn);
  header.append(&save_btn);
  root.append(&header);

  let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
  root.append(&sep);

  // Content stack: empty / text / unsupported.
  let stack = gtk::Stack::new();
  stack.set_hexpand(true);
  stack.set_vexpand(true);

  // Empty state.
  let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
  empty_box.set_halign(gtk::Align::Center);
  empty_box.set_valign(gtk::Align::Center);
  empty_box.set_hexpand(true);
  empty_box.set_vexpand(true);
  let empty_title = gtk::Label::new(Some(&lang::t("app.title")));
  empty_title.add_css_class("title-1");
  let empty_hint = gtk::Label::new(Some(&lang::t("empty.hint")));
  empty_hint.add_css_class("dim-label");
  empty_hint.set_wrap(true);
  empty_hint.set_justify(gtk::Justification::Center);
  let empty_open = gtk::Button::with_label(&lang::t("action.open_file"));
  empty_open.add_css_class("suggested-action");
  empty_open.set_halign(gtk::Align::Center);
  empty_box.append(&empty_title);
  empty_box.append(&empty_hint);
  empty_box.append(&empty_open);
  stack.add_named(&empty_box, Some("empty"));

  // Text area: inner stack with edit (gutter + editor) and preview pages.
  let text_stack = gtk::Stack::new();
  text_stack.set_hexpand(true);
  text_stack.set_vexpand(true);

  // Edit page: gutter + editor sharing one vertical adjustment.
  let edit_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
  edit_row.set_hexpand(true);
  edit_row.set_vexpand(true);
  let gutter_buffer = gtk::TextBuffer::new(None);
  let gutter_view = gtk::TextView::with_buffer(&gutter_buffer);
  gutter_view.set_editable(false);
  gutter_view.set_cursor_visible(false);
  gutter_view.set_monospace(true);
  gutter_view.set_vexpand(true);
  gutter_view.set_size_request(48, -1);
  let gutter_scroll = gtk::ScrolledWindow::new();
  gutter_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
  gutter_scroll.set_child(Some(&gutter_view));
  let edit_buffer = gtk::TextBuffer::new(None);
  let edit_view = gtk::TextView::with_buffer(&edit_buffer);
  edit_view.set_monospace(true);
  edit_view.set_wrap_mode(gtk::WrapMode::None);
  edit_view.set_hexpand(true);
  edit_view.set_vexpand(true);
  let edit_scroll = gtk::ScrolledWindow::new();
  edit_scroll.set_hexpand(true);
  edit_scroll.set_vexpand(true);
  edit_scroll.set_child(Some(&edit_view));
  gutter_scroll.set_vadjustment(Some(&edit_scroll.vadjustment()));
  edit_row.append(&gutter_scroll);
  edit_row.append(&edit_scroll);
  text_stack.add_named(&edit_row, Some("edit"));

  // Preview page: read-only view (plain text or rendered markdown).
  let preview_buffer = gtk::TextBuffer::new(None);
  let preview_view = gtk::TextView::with_buffer(&preview_buffer);
  preview_view.set_editable(false);
  preview_view.set_cursor_visible(false);
  preview_view.set_wrap_mode(gtk::WrapMode::WordChar);
  preview_view.set_hexpand(true);
  preview_view.set_vexpand(true);
  preview_view.set_left_margin(16);
  preview_view.set_right_margin(16);
  preview_view.set_top_margin(12);
  preview_view.set_bottom_margin(12);
  let preview_scroll = gtk::ScrolledWindow::new();
  preview_scroll.set_hexpand(true);
  preview_scroll.set_vexpand(true);
  preview_scroll.set_child(Some(&preview_view));
  text_stack.add_named(&preview_scroll, Some("preview"));

  stack.add_named(&text_stack, Some("text"));

  // Unsupported page.
  let unsup_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
  unsup_box.set_halign(gtk::Align::Center);
  unsup_box.set_valign(gtk::Align::Center);
  unsup_box.set_hexpand(true);
  unsup_box.set_vexpand(true);
  let unsup_title = gtk::Label::new(Some(&lang::t("unsupported.title")));
  unsup_title.add_css_class("title-2");
  let unsup_hint = gtk::Label::new(Some(""));
  unsup_hint.add_css_class("dim-label");
  unsup_hint.set_wrap(true);
  unsup_hint.set_justify(gtk::Justification::Center);
  let unsup_open = gtk::Button::with_label(&lang::t("action.open_other"));
  unsup_open.add_css_class("suggested-action");
  unsup_open.set_halign(gtk::Align::Center);
  unsup_box.append(&unsup_title);
  unsup_box.append(&unsup_hint);
  unsup_box.append(&unsup_open);
  stack.add_named(&unsup_box, Some("unsupported"));

  root.append(&stack);

  // Status line.
  let status = gtk::Label::new(Some(""));
  status.set_halign(gtk::Align::Start);
  status.add_css_class("dim-label");
  status.set_margin_start(16);
  status.set_margin_end(16);
  status.set_margin_top(6);
  status.set_margin_bottom(8);
  root.append(&status);

  apply_css(&root);

  let widgets = Rc::new(Widgets {
    title,
    subtitle,
    edit_btn: edit_btn.clone(),
    save_btn: save_btn.clone(),
    status,
    stack: stack.clone(),
    text_stack: text_stack.clone(),
    edit_view: edit_view.clone(),
    edit_buffer: edit_buffer.clone(),
    gutter_view,
    gutter_buffer,
    edit_scroll: edit_scroll.clone(),
    preview_view,
    preview_buffer: preview_buffer.clone(),
    unsup_title,
    unsup_hint,
    root: root.clone(),
  });

  // Track edits: any change in the edit buffer marks the file dirty.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    edit_buffer.connect_changed(move |buf| {
      let mut st = state.borrow_mut();
      if st.mode != Mode::Edit {
        return;
      }
      let (start, end) = (buf.start_iter(), buf.end_iter());
      let text = buf.text(&start, &end, false).to_string();
      if text != st.content {
        st.dirty = true;
      }
      drop(st);
      update_gutter(&widgets);
      refresh_chrome(&state, &widgets);
    });
  }

  // Ctrl+S shortcut for manual save.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Global);
    let trigger = gtk::ShortcutTrigger::parse_string("<Control>s").expect("ctrl+s trigger");
    let action = gtk::CallbackAction::new(move |_, _| {
      do_save(&state, &widgets);
      true.into()
    });
    controller.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
    root.add_controller(controller);
  }

  // Close-request: dirty files get Cancel / Save / Don't Save.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    let force = Rc::new(Cell::new(false));
    root.connect_realize(move |w| {
      let Some(toplevel) = w.root() else { return };
      let Ok(window) = toplevel.downcast::<gtk::ApplicationWindow>() else { return };
      let state = state.clone();
      let widgets = widgets.clone();
      let force = force.clone();
      window.connect_close_request(move |win| {
        if force.get() || !state.borrow().dirty {
          return false.into();
        }
        let win_c = win.clone();
        let state_c = state.clone();
        let widgets_c = widgets.clone();
        let force_c = force.clone();
        let dialog = gtk::AlertDialog::builder()
          .message(lang::t("close.title"))
          .detail(lang::t("close.detail"))
          .buttons(["close.cancel", "close.dont_save", "close.save"])
          .cancel_button(0)
          .default_button(2)
          .build();
        // Translate button labels after build.
        dialog.set_buttons(&[
          &lang::t("close.cancel"),
          &lang::t("close.dont_save"),
          &lang::t("close.save"),
        ]);
        let win_cc = win_c.clone();
        dialog.choose(
          Some(&win_cc),
          gtk::gio::Cancellable::NONE,
          move |response: Result<i32, glib::Error>| {
            match response {
              Ok(2) => {
                do_save(&state_c, &widgets_c);
                force_c.set(true);
                win_c.destroy();
              }
              Ok(1) => {
                force_c.set(true);
                win_c.destroy();
              }
              _ => {}
            }
          },
        );
        true.into()
      });
    });
  }

  // Open actions: native file dialog.
  {
    let s = state.clone();
    let w = widgets.clone();
    open_btn.connect_clicked(move |_| open_dialog(&s, &w));
    let s = state.clone();
    let w = widgets.clone();
    empty_open.connect_clicked(move |_| open_dialog(&s, &w));
    let s = state.clone();
    let w = widgets.clone();
    unsup_open.connect_clicked(move |_| open_dialog(&s, &w));
  }

  // Edit / Done toggle.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    edit_btn.connect_clicked(move |_| {
      let next = if state.borrow().mode == Mode::Edit { Mode::Preview } else { Mode::Edit };
      // Leaving edit with unsaved changes keeps the buffer; preview shows
      // the last saved content until the user saves (manual-save model).
      if next == Mode::Preview && state.borrow().dirty {
        sync_preview_from_edit(&state, &widgets, false);
      }
      state.borrow_mut().mode = next;
      refresh_chrome(&state, &widgets);
      refresh_body(&state, &widgets);
    });
  }

  // Save action.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    save_btn.connect_clicked(move |_| do_save(&state, &widgets));
  }

  // Initial file from CLI (`preview /path/to/file`).
  if let Some(path) = initial {
    open_path(&state, &widgets, &path);
  } else {
    refresh_chrome(&state, &widgets);
  }

  root
}

fn apply_css(root: &gtk::Box) {
  let css = format!(
    "textview {{ font-family: '{SF_PRO}', 'SF Pro Text', sans-serif; font-size: 13pt; }}\
     textview.mono {{ font-family: 'SF Mono', Monospace; }}\
     .dim-label {{ opacity: 0.6; }}\
     .title-1 {{ font-family: '{SF_PRO}'; font-size: 22pt; font-weight: 800; }}\
     .title-2 {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}"
  );
  let provider = gtk::CssProvider::new();
  provider.load_from_string(&css);
  if let Some(display) = gtk::gdk::Display::default() {
    gtk::style_context_add_provider_for_display(
      &display,
      &provider,
      gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
  }
  let _ = root;
}

/// Open the native file chooser and load the picked file.
fn open_dialog(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let ask_save_first = state.borrow().dirty;
  let state_c = state.clone();
  let widgets_c = widgets.clone();
  let proceed = Rc::new(move || {
    let dialog = gtk::FileDialog::builder().title(lang::t("dialog.open_title")).modal(true).build();
    let state_c = state_c.clone();
    let widgets_c = widgets_c.clone();
    dialog.open(
      root_window(&widgets_c).as_ref(),
      gtk::gio::Cancellable::NONE,
      move |result: Result<gtk::gio::File, glib::Error>| {
        let Ok(file) = result else { return };
        let Some(path) = file.path() else { return };
        open_path(&state_c, &widgets_c, &path);
      },
    );
  });
  if ask_save_first {
    confirm_discard(state, widgets, proceed);
  } else {
    proceed();
  }
}

fn root_window(widgets: &Rc<Widgets>) -> Option<gtk::Window> {
  widgets.root.root().and_then(|r| r.downcast::<gtk::Window>().ok())
}

/// Confirm discarding unsaved changes before opening another file.
fn confirm_discard(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>, proceed: Rc<dyn Fn()>) {
  let dialog = gtk::AlertDialog::builder()
    .message(lang::t("close.title"))
    .detail(lang::t("close.detail"))
    .cancel_button(0)
    .default_button(2)
    .build();
  dialog.set_buttons(&[
    &lang::t("close.cancel"),
    &lang::t("close.dont_save"),
    &lang::t("close.save"),
  ]);
  let state_c = state.clone();
  let widgets_c = widgets.clone();
  dialog.choose(
    root_window(widgets).as_ref(),
    gtk::gio::Cancellable::NONE,
    move |response: Result<i32, glib::Error>| {
      match response {
        Ok(2) => {
          do_save(&state_c, &widgets_c);
          proceed();
        }
        Ok(1) => proceed(),
        _ => {}
      }
    },
  );
}

/// Load a path into the state and refresh the UI.
fn open_path(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>, path: &PathBuf) {
  let kind = model::classify(path);
  if kind == FileKind::Unsupported {
    let mut st = state.borrow_mut();
    st.path = Some(path.clone());
    st.kind = kind;
    st.content.clear();
    st.dirty = false;
    st.mode = Mode::Preview;
    st.error = None;
    drop(st);
    refresh_chrome(state, widgets);
    refresh_body(state, widgets);
    return;
  }
  match model::load_text(path) {
    Ok(text) => {
      let mut st = state.borrow_mut();
      st.path = Some(path.clone());
      st.kind = kind;
      st.content = text;
      st.dirty = false;
      st.mode = Mode::Preview;
      st.error = None;
      drop(st);
      refresh_chrome(state, widgets);
      refresh_body(state, widgets);
    }
    Err(reason) => {
      let mut st = state.borrow_mut();
      st.path = Some(path.clone());
      st.kind = FileKind::Unsupported;
      st.content.clear();
      st.dirty = false;
      st.mode = Mode::Preview;
      st.error = Some(reason);
      drop(st);
      refresh_chrome(state, widgets);
      refresh_body(state, widgets);
    }
  }
}

/// Persist the edit buffer back to disk (manual save only).
fn do_save(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let path = match state.borrow().path.clone() {
    Some(p) => p,
    None => return,
  };
  let (start, end) = (widgets.edit_buffer.start_iter(), widgets.edit_buffer.end_iter());
  let text = widgets.edit_buffer.text(&start, &end, false).to_string();
  match model::save_text(&path, &text) {
    Ok(()) => {
      let mut st = state.borrow_mut();
      st.content = text;
      st.dirty = false;
      drop(st);
      refresh_chrome(state, widgets);
      refresh_body(state, widgets);
    }
    Err(reason) => {
      widgets.status.set_text(&lang::t_with("status.save_failed", &[("reason", &reason)]));
    }
  }
}

/// In markdown preview, show the edited (unsaved) text rendered so the
/// user sees the current change immediately; the saved `content` only
/// changes on manual save.
fn sync_preview_from_edit(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>, _saved_only: bool) {
  let _ = (state, widgets);
}

fn refresh_chrome(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let st = state.borrow();
  let has_file = st.path.is_some();
  let editable = has_file && st.kind != FileKind::Unsupported;

  if let Some(path) = st.path.as_ref() {
    let name = model::display_name(path);
    widgets.title.set_text(&name);
    widgets.subtitle.set_text(&path.display().to_string());
  } else {
    widgets.title.set_text(&lang::t("app.title"));
    widgets.subtitle.set_text(&lang::t("empty.hint"));
  }

  widgets.edit_btn.set_visible(editable);
  widgets.save_btn.set_visible(editable && st.mode == Mode::Edit);
  widgets.save_btn.set_sensitive(st.dirty);
  let edit_label = if st.mode == Mode::Edit {
    lang::t("action.done")
  } else {
    lang::t("action.edit")
  };
  widgets.edit_btn.set_label(&edit_label);

  if !has_file {
    widgets.status.set_text(&lang::t("status.empty"));
  } else if st.kind == FileKind::Unsupported {
    widgets.status.set_text(&lang::t("status.unsupported"));
  } else {
    let lines = st.content.lines().count().max(1);
    let key = if st.dirty { "status.dirty" } else { "status.saved" };
    widgets.status.set_text(&lang::t_with(
      key,
      &[("lines", &lines.to_string())],
    ));
  }

  let page = if !has_file {
    "empty"
  } else if st.kind == FileKind::Unsupported {
    "unsupported"
  } else {
    "text"
  };
  widgets.stack.set_visible_child_name(page);
  widgets
    .text_stack
    .set_visible_child_name(if st.mode == Mode::Edit { "edit" } else { "preview" });
}

fn refresh_body(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let st = state.borrow();
  if st.path.is_none() {
    return;
  }
  if st.kind == FileKind::Unsupported {
    let name = st.path.as_ref().map(|p| model::display_name(p)).unwrap_or_default();
    let reason = st.error.clone().unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.unsup_title.set_text(&name);
    widgets.unsup_hint.set_text(&lang::t_with(
      "unsupported.line",
      &[("reason", &reason)],
    ));
    return;
  }
  // Edit buffer always holds the raw text (saved content, or the current
  // unsaved edits when dirty). Only reset it when it is not dirty so
  // typing is never clobbered by a rebuild.
  if !st.dirty {
    // Block the changed handler side effects by comparing in handler.    widgets.edit_buffer.set_text(&st.content);
  }
  if st.kind == FileKind::Markdown && st.mode == Mode::Preview {
    // Markdown preview renders the current buffer (saved or edited).
    let text = if st.dirty {
      let (s, e) = (widgets.edit_buffer.start_iter(), widgets.edit_buffer.end_iter());
      widgets.edit_buffer.text(&s, &e, false).to_string()
    } else {
      st.content.clone()
    };
    markdown::render_into(&widgets.preview_buffer, &text);
  } else {
    let text = if st.mode == Mode::Edit && st.dirty {
      let (s, e) = (widgets.edit_buffer.start_iter(), widgets.edit_buffer.end_iter());
      widgets.edit_buffer.text(&s, &e, false).to_string()
    } else {
      st.content.clone()
    };
    widgets.preview_buffer.set_text(&text);
  }
  drop(st);
  update_gutter(widgets);
  // Refresh status line counts after body changes.
  // (chrome reads saved content; keep it simple and re-run.)
  // Note: borrow ended above.
  // refresh_chrome(state, widgets); // skip: changed handler already does it
}

fn update_gutter(widgets: &Widgets) {
  let lines = widgets.edit_buffer.line_count().max(1);
  let mut text = String::with_capacity((lines as usize) * 4);
  for n in 1..=lines {
    text.push_str(&n.to_string());
    text.push('\n');
  }
  // Avoid recursive changed signals: gutter buffer has no handler.
  widgets.gutter_buffer.set_text(&text);
}
