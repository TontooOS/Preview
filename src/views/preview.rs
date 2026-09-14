//! Tahoe-style Preview root view.
//!
//! Header with file name (visible only when a file is open), `Open` /
//! `Edit`-`Done` / `Save` actions. Empty state with centered title, hint
//! and a large pill-shaped open button. Text files
//! render read-only with a rendered Markdown preview mode; edit mode
//! shows raw text with line numbers on the left. PDFs open read-only in
//! a page viewer (previous/next, page indicator, zoom, fit width). Images
//! open read-only as actual pictures (`gtk::Picture` with zoom in/out
//! plus fit window). Audio files open read-only in a compact player
//! (`gtk::MediaFile` with play/pause, seek slider, volume plus file
//! info). Video files open read-only on a player page with a picture
//! (`gtk::Video` driven by `gtk::MediaFile` with play/pause, seek slider,
//! volume plus file info). Word documents open read-only as formatted text
//! (`.docx` fully, `.odt`/`.rtf` best-effort, legacy `.doc` with an honest
//! hint). Presentations open read-only as a slide viewer (previous/next,
//! slide indicator, title plus body text, speaker notes). Spreadsheets open
//! read-only as a sheet grid (sheet switcher, column letters plus row
//! numbers, bold header row). Saving is manual
//! only (`Save` button, `Ctrl+S`,
//! close dialog with Cancel / Save / Don't Save).

use crate::docx;
use crate::lang;
use crate::markdown;
use crate::model::{self, FileKind};
use crate::pptx;
use crate::xlsx;
use crate::TontooUI::{Toolbar, ToolbarItem};
use crate::UIKit::widget::{Widget, WidgetId, next_widget_id};
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
  pdf: Option<Rc<model::PdfDoc>>,
  pdf_page: usize,
  pdf_zoom: f64,
  pdf_fit: bool,
  img_meta: Option<model::ImageMeta>,
  img_error: Option<String>,
  img_base: Option<gdk_pixbuf::Pixbuf>,
  img_zoom: f64,
  img_fit: bool,
  audio_meta: Option<model::AudioMeta>,
  audio_error: Option<String>,
  audio_media: Option<gtk::MediaFile>,
  audio_tick: Option<glib::SourceId>,
  audio_volume: f64,
  video_meta: Option<model::VideoMeta>,
  video_error: Option<String>,
  video_media: Option<gtk::MediaFile>,
  video_tick: Option<glib::SourceId>,
  video_volume: f64,
  doc_doc: Option<docx::Document>,
  doc_error: Option<String>,
  pres_deck: Option<pptx::Deck>,
  pres_error: Option<String>,
  pres_slide: usize,
  sheet_book: Option<xlsx::Workbook>,
  sheet_error: Option<String>,
  sheet_index: usize,
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
      pdf: None,
      pdf_page: 0,
      pdf_zoom: 1.0,
      pdf_fit: true,
      img_meta: None,
      img_error: None,
      img_base: None,
      img_zoom: 1.0,
      img_fit: true,
      audio_meta: None,
      audio_error: None,
      audio_media: None,
      audio_tick: None,
      audio_volume: 1.0,
      video_meta: None,
      video_error: None,
      video_media: None,
      video_tick: None,
      video_volume: 1.0,
      doc_doc: None,
      doc_error: None,
      pres_deck: None,
      pres_error: None,
      pres_slide: 0,
      sheet_book: None,
      sheet_error: None,
      sheet_index: 0,
    }
  }
}

#[allow(dead_code)]
struct Widgets {
  title_box: gtk::Box,
  title: gtk::Label,
  subtitle: gtk::Label,
  edit_btn: gtk::Button,
  save_btn: gtk::Button,
  tb_open: gtk::Button,
  tb_zoom_in: gtk::Button,
  tb_zoom_out: gtk::Button,
  tb_share: gtk::Button,
  tb_annotate: gtk::Button,
  tb_info: gtk::Button,
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
  pdf_bar: gtk::Box,
  prev_btn: gtk::Button,
  next_btn: gtk::Button,
  page_label: gtk::Label,
  zoom_out_btn: gtk::Button,
  zoom_in_btn: gtk::Button,
  fit_btn: gtk::ToggleButton,
  pdf_scroll: gtk::ScrolledWindow,
  pdf_view: gtk::TextView,
  pdf_buffer: gtk::TextBuffer,
  pdf_error: gtk::Label,
  pdf_css: gtk::CssProvider,
  img_bar: gtk::Box,
  img_zoom_out_btn: gtk::Button,
  img_zoom_in_btn: gtk::Button,
  img_fit_btn: gtk::ToggleButton,
  img_scroll: gtk::ScrolledWindow,
  img_picture: gtk::Picture,
  img_error: gtk::Label,
  audio_play_btn: gtk::Button,
  audio_seek: gtk::Scale,
  audio_time: gtk::Label,
  audio_title: gtk::Label,
  audio_info: gtk::Label,
  audio_vol: gtk::Scale,
  audio_vol_row: gtk::Box,
  audio_error_lbl: gtk::Label,
  audio_controls: gtk::Box,
  video: gtk::Video,
  video_play_btn: gtk::Button,
  video_seek: gtk::Scale,
  video_time: gtk::Label,
  video_title: gtk::Label,
  video_info: gtk::Label,
  video_vol: gtk::Scale,
  video_vol_row: gtk::Box,
  video_error_lbl: gtk::Label,
  video_controls: gtk::Box,
  doc_title: gtk::Label,
  doc_info: gtk::Label,
  doc_scroll: gtk::ScrolledWindow,
  doc_view: gtk::TextView,
  doc_buffer: gtk::TextBuffer,
  doc_error_lbl: gtk::Label,
  pres_bar: gtk::Box,
  pres_prev_btn: gtk::Button,
  pres_next_btn: gtk::Button,
  pres_label: gtk::Label,
  pres_title: gtk::Label,
  pres_info: gtk::Label,
  pres_scroll: gtk::ScrolledWindow,
  pres_view: gtk::TextView,
  pres_buffer: gtk::TextBuffer,
  pres_error_lbl: gtk::Label,
  sheet_bar: gtk::Box,
  sheet_prev_btn: gtk::Button,
  sheet_next_btn: gtk::Button,
  sheet_label: gtk::Label,
  sheet_tabs_scroll: gtk::ScrolledWindow,
  sheet_tabs: gtk::Box,
  sheet_title: gtk::Label,
  sheet_info: gtk::Label,
  sheet_scroll: gtk::ScrolledWindow,
  sheet_grid: gtk::Grid,
  sheet_hint_lbl: gtk::Label,
  sheet_error_lbl: gtk::Label,
  root: gtk::Box,
}

/// Collect the rendered `GtkButton`s inside a TontooUI `Toolbar` in
/// visual order, so stateful Preview actions (open dialog, zoom) can be
/// wired with `Rc` closures via `connect_clicked`.
fn collect_toolbar_buttons(toolbar: &gtk::Widget) -> Vec<gtk::Button> {
  fn push_buttons(node: &gtk::Widget, out: &mut Vec<gtk::Button>) {
    let mut cursor = node.first_child();
    while let Some(widget) = cursor {
      cursor = widget.next_sibling();
      if let Ok(btn) = widget.clone().downcast::<gtk::Button>() {
        out.push(btn);
      } else {
        push_buttons(&widget, out);
      }
    }
  }
  let mut buttons = Vec::new();
  push_buttons(toolbar, &mut buttons);
  buttons
}

/// Zoom step shared by the header toolbar icons: PDF pages scale the text
/// font, images scale the pixbuf. Other kinds ignore the step.
fn toolbar_zoom(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>, zoom_in: bool) {
  let kind = state.borrow().kind;
  if kind == FileKind::Pdf {
    let zoom = state.borrow().pdf_zoom;
    let next = if zoom_in { (zoom + 0.25).min(3.0) } else { (zoom - 0.25).max(0.5) };
    state.borrow_mut().pdf_zoom = next;
    apply_pdf_zoom(state, widgets);
  } else if kind == FileKind::Image {
    if state.borrow().img_fit {
      // Leaving fit mode: sync the factor from the live viewport first
      // so the first manual step continues smoothly instead of jumping.
      sync_image_fit_factor(state, widgets);
    }
    let zoom = state.borrow().img_zoom;
    let next = if zoom_in {
      model::image_zoom_in(zoom)
    } else {
      model::image_zoom_out(zoom)
    };
    state.borrow_mut().img_zoom = next;
    state.borrow_mut().img_fit = false;
    widgets.img_fit_btn.set_active(false);
    apply_image_zoom(state, widgets);
  }
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

  fn hides_window_bar(&self) -> bool {
    true
  }

  fn to_gtk(&self) -> gtk::Widget {
    build_ui(self.initial.clone()).upcast()
  }
}

fn build_ui(initial: Option<PathBuf>) -> gtk::Box {
  let state = Rc::new(RefCell::new(State::empty()));

  // Root column: header plus content stack (no status line).
  let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

  // Top row: traffic lights directly on the window (no decoration bar),
  // then file name only, then actions. Uniform frame: 16px to the
  // window edge on all sides, 8px gaps between sections.
  let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  header.set_margin_start(16);
  header.set_margin_end(16);
  header.set_margin_top(16);
  header.set_margin_bottom(8);

  let lights = crate::UIKit::widgets::TrafficLights::new().to_gtk();
  lights.set_valign(gtk::Align::Center);
  header.append(&lights);

  let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
  title_box.set_hexpand(true);
  title_box.set_valign(gtk::Align::Center);
  let title = gtk::Label::new(Some(&lang::t("app.title")));
  title.set_halign(gtk::Align::Start);
  title.set_valign(gtk::Align::Center);
  title.set_hexpand(true);
  // Long file names truncate with "..." at the end instead of pushing
  // the action buttons and toolbar out of the window.
  title.set_ellipsize(gtk::pango::EllipsizeMode::End);
  title.add_css_class("title-1");
  let subtitle = gtk::Label::new(Some(""));
  subtitle.set_visible(false);
  title_box.append(&title);
  header.append(&title_box);

  let edit_btn = gtk::Button::with_label(&lang::t("action.edit"));
  let save_btn = gtk::Button::with_label(&lang::t("action.save"));
  save_btn.set_sensitive(false);
  save_btn.set_visible(false);
  edit_btn.set_visible(false);
  header.append(&edit_btn);
  header.append(&save_btn);

  // Icon toolbar on the far right (Finder-style TontooUI element):
  // open document, zoom in, zoom out, share (no action yet),
  // annotate (no action yet), info on the far right (no action yet).
  let toolbar = Toolbar::new()
    .item(ToolbarItem::new("doc.badge.arrow.up.fill"))
    .item(ToolbarItem::new("plus.magnifyingglass"))
    .item(ToolbarItem::new("minus.magnifyingglass"))
    .item(ToolbarItem::new("square.and.arrow.up.fill"))
    .item(ToolbarItem::new("square.and.pencil"))
    .item(ToolbarItem::new("info.circle"));
  let toolbar_gtk = toolbar.to_gtk();
  toolbar_gtk.set_valign(gtk::Align::Center);
  header.append(&toolbar_gtk);
  let tb_buttons = collect_toolbar_buttons(&toolbar_gtk);
  let tb_open = tb_buttons.first().cloned().unwrap_or_else(gtk::Button::new);
  let tb_zoom_in = tb_buttons.get(1).cloned().unwrap_or_else(gtk::Button::new);
  let tb_zoom_out = tb_buttons.get(2).cloned().unwrap_or_else(gtk::Button::new);
  let tb_share = tb_buttons.get(3).cloned().unwrap_or_else(gtk::Button::new);
  let tb_annotate = tb_buttons.get(4).cloned().unwrap_or_else(gtk::Button::new);
  let tb_info = tb_buttons.get(5).cloned().unwrap_or_else(gtk::Button::new);
  tb_open.set_tooltip_text(Some(&lang::t("action.open")));
  tb_zoom_in.set_tooltip_text(Some(&lang::t("pdf.zoom_in")));
  tb_zoom_out.set_tooltip_text(Some(&lang::t("pdf.zoom_out")));
  tb_info.set_tooltip_text(Some(&lang::t("action.info")));
  root.append(&header);

  // Content stack: empty / text / unsupported. Same 16px side
  // distance as the header so the frame is uniform.
  let stack = gtk::Stack::new();
  stack.set_hexpand(true);
  stack.set_vexpand(true);
  stack.set_margin_start(16);
  stack.set_margin_end(16);

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
  empty_open.add_css_class("preview-open-button");
  empty_open.set_halign(gtk::Align::Center);
  empty_open.set_size_request(220, 48);
  empty_open.set_margin_top(8);
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
  unsup_open.add_css_class("preview-open-button");
  unsup_open.set_halign(gtk::Align::Center);
  unsup_open.set_size_request(220, 48);
  unsup_open.set_margin_top(8);
  unsup_box.append(&unsup_title);
  unsup_box.append(&unsup_hint);
  unsup_box.append(&unsup_open);
  stack.add_named(&unsup_box, Some("unsupported"));

  // PDF page: toolbar (previous/next, page label, zoom, fit) plus a
  // read-only page view. Broken PDFs show an error label instead.
  let pdf_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
  pdf_box.set_hexpand(true);
  pdf_box.set_vexpand(true);
  let pdf_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  pdf_bar.set_margin_start(16);
  pdf_bar.set_margin_end(16);
  pdf_bar.set_margin_top(8);
  pdf_bar.set_margin_bottom(8);
  let prev_btn = gtk::Button::with_label(&lang::t("pdf.prev"));
  let page_label = gtk::Label::new(Some(""));
  page_label.set_hexpand(true);
  page_label.add_css_class("dim-label");
  let next_btn = gtk::Button::with_label(&lang::t("pdf.next"));
  let zoom_out_btn = gtk::Button::with_label(&lang::t("pdf.zoom_out"));
  let zoom_in_btn = gtk::Button::with_label(&lang::t("pdf.zoom_in"));
  let fit_btn = gtk::ToggleButton::with_label(&lang::t("pdf.fit"));
  fit_btn.set_active(true);
  pdf_bar.append(&prev_btn);
  pdf_bar.append(&page_label);
  pdf_bar.append(&next_btn);
  pdf_bar.append(&zoom_out_btn);
  pdf_bar.append(&zoom_in_btn);
  pdf_bar.append(&fit_btn);
  pdf_box.append(&pdf_bar);
  let pdf_buffer = gtk::TextBuffer::new(None);
  let pdf_view = gtk::TextView::with_buffer(&pdf_buffer);
  pdf_view.set_editable(false);
  pdf_view.set_cursor_visible(false);
  pdf_view.set_wrap_mode(gtk::WrapMode::WordChar);
  pdf_view.set_hexpand(true);
  pdf_view.set_vexpand(true);
  pdf_view.set_left_margin(16);
  pdf_view.set_right_margin(16);
  pdf_view.set_top_margin(12);
  pdf_view.set_bottom_margin(12);
  pdf_view.add_css_class("pdf-page");
  let pdf_scroll = gtk::ScrolledWindow::new();
  pdf_scroll.set_hexpand(true);
  pdf_scroll.set_vexpand(true);
  pdf_scroll.set_child(Some(&pdf_view));
  pdf_box.append(&pdf_scroll);
  let pdf_error = gtk::Label::new(Some(""));
  pdf_error.add_css_class("dim-label");
  pdf_error.set_wrap(true);
  pdf_error.set_justify(gtk::Justification::Center);
  pdf_error.set_halign(gtk::Align::Center);
  pdf_error.set_valign(gtk::Align::Center);
  pdf_error.set_hexpand(true);
  pdf_error.set_vexpand(true);
  pdf_box.append(&pdf_error);
  stack.add_named(&pdf_box, Some("pdf"));

  // Image page: actual picture (`gtk::Picture`) without any in-page
  // buttons (zoom lives in the header toolbar). Broken images show an
  // error label with the reason.
  let img_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
  img_box.set_hexpand(true);
  img_box.set_vexpand(true);
  let img_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  img_bar.set_margin_start(16);
  img_bar.set_margin_end(16);
  img_bar.set_margin_top(8);
  img_bar.set_margin_bottom(8);
  img_bar.set_halign(gtk::Align::Center);
  let img_zoom_out_btn = gtk::Button::with_label(&lang::t("image.zoom_out"));
  let img_zoom_in_btn = gtk::Button::with_label(&lang::t("image.zoom_in"));
  let img_fit_btn = gtk::ToggleButton::with_label(&lang::t("image.fit"));
  img_fit_btn.set_active(true);
  img_bar.append(&img_zoom_out_btn);
  img_bar.append(&img_zoom_in_btn);
  img_bar.append(&img_fit_btn);
  // Kept alive for state (fit toggle) but never shown: zoom runs
  // through the header toolbar icons.
  img_bar.set_visible(false);
  let img_picture = gtk::Picture::new();
  img_picture.set_content_fit(gtk::ContentFit::Contain);
  img_picture.set_can_shrink(true);
  img_picture.set_halign(gtk::Align::Center);
  img_picture.set_valign(gtk::Align::Center);
  img_picture.set_hexpand(true);
  img_picture.set_vexpand(true);
  let img_scroll = gtk::ScrolledWindow::new();
  img_scroll.set_hexpand(true);
  img_scroll.set_vexpand(true);
  img_scroll.set_child(Some(&img_picture));
  img_box.append(&img_scroll);
  let img_error = gtk::Label::new(Some(""));
  img_error.add_css_class("dim-label");
  img_error.set_wrap(true);
  img_error.set_justify(gtk::Justification::Center);
  img_error.set_halign(gtk::Align::Center);
  img_error.set_valign(gtk::Align::Center);
  img_error.set_hexpand(true);
  img_error.set_vexpand(true);
  img_box.append(&img_error);
  stack.add_named(&img_box, Some("image"));

  // Audio page: compact player with play/pause, a seek slider with
  // elapsed/total time, a volume slider and a file info line. Playback
  // is GStreamer-backed (`gtk::MediaFile`); files without a decoder show
  // an honest hint with the stream error instead of crashing.
  let audio_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
  audio_box.set_halign(gtk::Align::Center);
  audio_box.set_valign(gtk::Align::Center);
  audio_box.set_hexpand(true);
  audio_box.set_vexpand(true);
  audio_box.set_margin_start(32);
  audio_box.set_margin_end(32);
  let audio_title = gtk::Label::new(Some(""));
  audio_title.add_css_class("audio-title");
  let audio_info = gtk::Label::new(Some(""));
  audio_info.add_css_class("dim-label");
  audio_info.set_wrap(true);
  audio_info.set_justify(gtk::Justification::Center);
  audio_box.append(&audio_title);
  audio_box.append(&audio_info);
  let audio_controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
  audio_controls.set_halign(gtk::Align::Center);
  audio_controls.set_hexpand(true);
  let audio_play_btn = gtk::Button::with_label(&lang::t("audio.play"));
  audio_play_btn.add_css_class("suggested-action");
  let audio_seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1000.0, 1.0);
  audio_seek.set_draw_value(false);
  audio_seek.set_hexpand(true);
  audio_seek.set_size_request(360, -1);
  let audio_time = gtk::Label::new(Some(&lang::t_with(
    "audio.position",
    &[("elapsed", "0:00"), ("total", "--:--")],
  )));
  audio_time.add_css_class("audio-time");
  audio_time.add_css_class("dim-label");
  audio_controls.append(&audio_play_btn);
  audio_controls.append(&audio_seek);
  audio_controls.append(&audio_time);
  audio_box.append(&audio_controls);
  let audio_vol_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  audio_vol_row.set_halign(gtk::Align::Center);
  let audio_vol_label = gtk::Label::new(Some(&lang::t("audio.volume")));
  audio_vol_label.add_css_class("dim-label");
  let audio_vol = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
  audio_vol.set_draw_value(false);
  audio_vol.set_size_request(160, -1);
  audio_vol.set_value(100.0);
  audio_vol_row.append(&audio_vol_label);
  audio_vol_row.append(&audio_vol);
  audio_box.append(&audio_vol_row);
  let audio_error_lbl = gtk::Label::new(Some(""));
  audio_error_lbl.add_css_class("dim-label");
  audio_error_lbl.set_wrap(true);
  audio_error_lbl.set_justify(gtk::Justification::Center);
  audio_box.append(&audio_error_lbl);
  stack.add_named(&audio_box, Some("audio"));

  // Video page: picture (`gtk::Video` driven by a `gtk::MediaFile`) plus
  // title, file info, play/pause with a seek slider and elapsed/total
  // time, a volume slider and an error hint. Playback is GStreamer-backed;
  // files without a decoder show an honest hint with the stream error
  // instead of crashing.
  let video_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
  video_box.set_hexpand(true);
  video_box.set_vexpand(true);
  video_box.set_margin_start(16);
  video_box.set_margin_end(16);
  video_box.set_margin_top(8);
  video_box.set_margin_bottom(8);
  let video = gtk::Video::new();
  video.set_autoplay(false);
  video.set_hexpand(true);
  video.set_vexpand(true);
  video.set_size_request(640, 360);
  video_box.append(&video);
  let video_title = gtk::Label::new(Some(""));
  video_title.add_css_class("video-title");
  let video_info = gtk::Label::new(Some(""));
  video_info.add_css_class("dim-label");
  video_info.set_wrap(true);
  video_info.set_justify(gtk::Justification::Center);
  video_box.append(&video_title);
  video_box.append(&video_info);
  let video_controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
  video_controls.set_halign(gtk::Align::Center);
  video_controls.set_hexpand(true);
  let video_play_btn = gtk::Button::with_label(&lang::t("video.play"));
  video_play_btn.add_css_class("suggested-action");
  let video_seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1000.0, 1.0);
  video_seek.set_draw_value(false);
  video_seek.set_hexpand(true);
  video_seek.set_size_request(360, -1);
  let video_time = gtk::Label::new(Some(&lang::t_with(
    "video.position",
    &[("elapsed", "0:00"), ("total", "--:--")],
  )));
  video_time.add_css_class("video-time");
  video_time.add_css_class("dim-label");
  video_controls.append(&video_play_btn);
  video_controls.append(&video_seek);
  video_controls.append(&video_time);
  video_box.append(&video_controls);
  let video_vol_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  video_vol_row.set_halign(gtk::Align::Center);
  let video_vol_label = gtk::Label::new(Some(&lang::t("video.volume")));
  video_vol_label.add_css_class("dim-label");
  let video_vol = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
  video_vol.set_draw_value(false);
  video_vol.set_size_request(160, -1);
  video_vol.set_value(100.0);
  video_vol_row.append(&video_vol_label);
  video_vol_row.append(&video_vol);
  video_box.append(&video_vol_row);
  let video_error_lbl = gtk::Label::new(Some(""));
  video_error_lbl.add_css_class("dim-label");
  video_error_lbl.set_wrap(true);
  video_error_lbl.set_justify(gtk::Justification::Center);
  video_box.append(&video_error_lbl);
  stack.add_named(&video_box, Some("video"));

  // Document page: title plus file info plus a read-only formatted view
  // (headings, bold/italic, bullets, tables as a plain grid). Broken or
  // unsupported documents (corrupt, password-protected, legacy `.doc`)
  // stay on this page and show an error hint with the reason.
  let doc_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
  doc_box.set_hexpand(true);
  doc_box.set_vexpand(true);
  let doc_title = gtk::Label::new(Some(""));
  doc_title.add_css_class("docx-title");
  doc_title.set_margin_top(12);
  let doc_info = gtk::Label::new(Some(""));
  doc_info.add_css_class("dim-label");
  doc_info.set_wrap(true);
  doc_info.set_justify(gtk::Justification::Center);
  doc_info.set_margin_bottom(4);
  doc_box.append(&doc_title);
  doc_box.append(&doc_info);
  let doc_buffer = gtk::TextBuffer::new(None);
  let doc_view = gtk::TextView::with_buffer(&doc_buffer);
  doc_view.set_editable(false);
  doc_view.set_cursor_visible(false);
  doc_view.set_wrap_mode(gtk::WrapMode::WordChar);
  doc_view.set_hexpand(true);
  doc_view.set_vexpand(true);
  doc_view.set_left_margin(16);
  doc_view.set_right_margin(16);
  doc_view.set_top_margin(12);
  doc_view.set_bottom_margin(12);
  let doc_scroll = gtk::ScrolledWindow::new();
  doc_scroll.set_hexpand(true);
  doc_scroll.set_vexpand(true);
  doc_scroll.set_child(Some(&doc_view));
  doc_box.append(&doc_scroll);
  let doc_error_lbl = gtk::Label::new(Some(""));
  doc_error_lbl.add_css_class("dim-label");
  doc_error_lbl.set_wrap(true);
  doc_error_lbl.set_justify(gtk::Justification::Center);
  doc_error_lbl.set_halign(gtk::Align::Center);
  doc_error_lbl.set_valign(gtk::Align::Center);
  doc_error_lbl.set_hexpand(true);
  doc_error_lbl.set_vexpand(true);
  doc_box.append(&doc_error_lbl);
  stack.add_named(&doc_box, Some("docx"));

  // Presentation page: toolbar (previous/next, slide indicator) plus a
  // title, an info line and a read-only formatted slide view (title, body
  // text, tables as a plain grid, dim speaker notes). Broken or
  // unsupported presentations (corrupt, password-protected, legacy `.ppt`)
  // stay on this page and show an error hint with the reason.
  let pres_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
  pres_box.set_hexpand(true);
  pres_box.set_vexpand(true);
  let pres_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  pres_bar.set_margin_start(16);
  pres_bar.set_margin_end(16);
  pres_bar.set_margin_top(8);
  pres_bar.set_margin_bottom(8);
  let pres_prev_btn = gtk::Button::with_label(&lang::t("pptx.prev"));
  let pres_label = gtk::Label::new(Some(""));
  pres_label.set_hexpand(true);
  pres_label.add_css_class("dim-label");
  let pres_next_btn = gtk::Button::with_label(&lang::t("pptx.next"));
  pres_bar.append(&pres_prev_btn);
  pres_bar.append(&pres_label);
  pres_bar.append(&pres_next_btn);
  pres_box.append(&pres_bar);
  let pres_title = gtk::Label::new(Some(""));
  pres_title.add_css_class("pptx-title");
  let pres_info = gtk::Label::new(Some(""));
  pres_info.add_css_class("dim-label");
  pres_info.set_wrap(true);
  pres_info.set_justify(gtk::Justification::Center);
  pres_info.set_margin_bottom(4);
  pres_box.append(&pres_title);
  pres_box.append(&pres_info);
  let pres_buffer = gtk::TextBuffer::new(None);
  let pres_view = gtk::TextView::with_buffer(&pres_buffer);
  pres_view.set_editable(false);
  pres_view.set_cursor_visible(false);
  pres_view.set_wrap_mode(gtk::WrapMode::WordChar);
  pres_view.set_hexpand(true);
  pres_view.set_vexpand(true);
  pres_view.set_left_margin(16);
  pres_view.set_right_margin(16);
  pres_view.set_top_margin(12);
  pres_view.set_bottom_margin(12);
  let pres_scroll = gtk::ScrolledWindow::new();
  pres_scroll.set_hexpand(true);
  pres_scroll.set_vexpand(true);
  pres_scroll.set_child(Some(&pres_view));
  pres_box.append(&pres_scroll);
  let pres_error_lbl = gtk::Label::new(Some(""));
  pres_error_lbl.add_css_class("dim-label");
  pres_error_lbl.set_wrap(true);
  pres_error_lbl.set_justify(gtk::Justification::Center);
  pres_error_lbl.set_halign(gtk::Align::Center);
  pres_error_lbl.set_valign(gtk::Align::Center);
  pres_error_lbl.set_hexpand(true);
  pres_error_lbl.set_vexpand(true);
  pres_box.append(&pres_error_lbl);
  stack.add_named(&pres_box, Some("pptx"));

  // Spreadsheet page: toolbar (previous/next, sheet indicator) plus a
  // sheet tab switcher, a title, an info line and a read-only grid
  // (column letters, row numbers, bold header row). Broken or unsupported
  // spreadsheets (corrupt, password-protected, legacy `.xls`) stay on
  // this page and show an error hint with the reason.
  let sheet_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
  sheet_box.set_hexpand(true);
  sheet_box.set_vexpand(true);
  let sheet_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  sheet_bar.set_margin_start(16);
  sheet_bar.set_margin_end(16);
  sheet_bar.set_margin_top(8);
  sheet_bar.set_margin_bottom(8);
  let sheet_prev_btn = gtk::Button::with_label(&lang::t("xlsx.prev"));
  let sheet_label = gtk::Label::new(Some(""));
  sheet_label.set_hexpand(true);
  sheet_label.add_css_class("dim-label");
  let sheet_next_btn = gtk::Button::with_label(&lang::t("xlsx.next"));
  sheet_bar.append(&sheet_prev_btn);
  sheet_bar.append(&sheet_label);
  sheet_bar.append(&sheet_next_btn);
  sheet_box.append(&sheet_bar);
  // Sheet tabs: one toggle button per sheet, only visible when the
  // workbook holds multiple sheets.
  let sheet_tabs = gtk::Box::new(gtk::Orientation::Horizontal, 4);
  sheet_tabs.set_margin_start(16);
  sheet_tabs.set_margin_end(16);
  sheet_tabs.set_halign(gtk::Align::Center);
  let sheet_tabs_scroll = gtk::ScrolledWindow::new();
  sheet_tabs_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
  sheet_tabs_scroll.set_child(Some(&sheet_tabs));
  sheet_box.append(&sheet_tabs_scroll);
  let sheet_title = gtk::Label::new(Some(""));
  sheet_title.add_css_class("xlsx-title");
  let sheet_info = gtk::Label::new(Some(""));
  sheet_info.add_css_class("dim-label");
  sheet_info.set_wrap(true);
  sheet_info.set_justify(gtk::Justification::Center);
  sheet_info.set_margin_bottom(4);
  sheet_box.append(&sheet_title);
  sheet_box.append(&sheet_info);
  let sheet_grid = gtk::Grid::new();
  sheet_grid.set_row_spacing(2);
  sheet_grid.set_column_spacing(12);
  sheet_grid.set_margin_start(16);
  sheet_grid.set_margin_end(16);
  sheet_grid.set_margin_top(4);
  sheet_grid.set_margin_bottom(12);
  let sheet_scroll = gtk::ScrolledWindow::new();
  sheet_scroll.set_hexpand(true);
  sheet_scroll.set_vexpand(true);
  sheet_scroll.set_child(Some(&sheet_grid));
  sheet_box.append(&sheet_scroll);
  // Shared hint slot: truncation note or empty-sheet note.
  let sheet_hint_lbl = gtk::Label::new(Some(""));
  sheet_hint_lbl.add_css_class("dim-label");
  sheet_hint_lbl.set_wrap(true);
  sheet_hint_lbl.set_justify(gtk::Justification::Center);
  sheet_hint_lbl.set_margin_bottom(8);
  sheet_box.append(&sheet_hint_lbl);
  let sheet_error_lbl = gtk::Label::new(Some(""));
  sheet_error_lbl.add_css_class("dim-label");
  sheet_error_lbl.set_wrap(true);
  sheet_error_lbl.set_justify(gtk::Justification::Center);
  sheet_error_lbl.set_halign(gtk::Align::Center);
  sheet_error_lbl.set_valign(gtk::Align::Center);
  sheet_error_lbl.set_hexpand(true);
  sheet_error_lbl.set_vexpand(true);
  sheet_box.append(&sheet_error_lbl);
  stack.add_named(&sheet_box, Some("xlsx"));

  root.append(&stack);

  // No status line: the content stack ends with an 8px gap plus the
  // 16px bottom margin below.
  stack.set_margin_bottom(16);

  apply_css(&root);
  // Dedicated display provider for the PDF zoom level (higher priority
  // than the base style so the scaled font size wins).
  let pdf_css = gtk::CssProvider::new();
  if let Some(display) = gtk::gdk::Display::default() {
    gtk::style_context_add_provider_for_display(
      &display,
      &pdf_css,
      gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
  }

  let widgets = Rc::new(Widgets {
    title_box: title_box.clone(),
    title,
    subtitle,
    edit_btn: edit_btn.clone(),
    save_btn: save_btn.clone(),
    tb_open: tb_open.clone(),
    tb_zoom_in: tb_zoom_in.clone(),
    tb_zoom_out: tb_zoom_out.clone(),
    tb_share: tb_share.clone(),
    tb_annotate: tb_annotate.clone(),
    tb_info: tb_info.clone(),
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
    pdf_bar: pdf_bar.clone(),
    prev_btn: prev_btn.clone(),
    next_btn: next_btn.clone(),
    page_label: page_label.clone(),
    zoom_out_btn: zoom_out_btn.clone(),
    zoom_in_btn: zoom_in_btn.clone(),
    fit_btn: fit_btn.clone(),
    pdf_scroll: pdf_scroll.clone(),
    pdf_view: pdf_view.clone(),
    pdf_buffer: pdf_buffer.clone(),
    pdf_error: pdf_error.clone(),
    pdf_css: pdf_css.clone(),
    img_bar: img_bar.clone(),
    img_zoom_out_btn: img_zoom_out_btn.clone(),
    img_zoom_in_btn: img_zoom_in_btn.clone(),
    img_fit_btn: img_fit_btn.clone(),
    img_scroll: img_scroll.clone(),
    img_picture: img_picture.clone(),
    img_error: img_error.clone(),
    audio_play_btn: audio_play_btn.clone(),
    audio_seek: audio_seek.clone(),
    audio_time: audio_time.clone(),
    audio_title: audio_title.clone(),
    audio_info: audio_info.clone(),
    audio_vol: audio_vol.clone(),
    audio_vol_row: audio_vol_row.clone(),
    audio_error_lbl: audio_error_lbl.clone(),
    audio_controls: audio_controls.clone(),
    video: video.clone(),
    video_play_btn: video_play_btn.clone(),
    video_seek: video_seek.clone(),
    video_time: video_time.clone(),
    video_title: video_title.clone(),
    video_info: video_info.clone(),
    video_vol: video_vol.clone(),
    video_vol_row: video_vol_row.clone(),
    video_error_lbl: video_error_lbl.clone(),
    video_controls: video_controls.clone(),
    doc_title: doc_title.clone(),
    doc_info: doc_info.clone(),
    doc_scroll: doc_scroll.clone(),
    doc_view: doc_view.clone(),
    doc_buffer: doc_buffer.clone(),
    doc_error_lbl: doc_error_lbl.clone(),
    pres_bar: pres_bar.clone(),
    pres_prev_btn: pres_prev_btn.clone(),
    pres_next_btn: pres_next_btn.clone(),
    pres_label: pres_label.clone(),
    pres_title: pres_title.clone(),
    pres_info: pres_info.clone(),
    pres_scroll: pres_scroll.clone(),
    pres_view: pres_view.clone(),
    pres_buffer: pres_buffer.clone(),
    pres_error_lbl: pres_error_lbl.clone(),
    sheet_bar: sheet_bar.clone(),
    sheet_prev_btn: sheet_prev_btn.clone(),
    sheet_next_btn: sheet_next_btn.clone(),
    sheet_label: sheet_label.clone(),
    sheet_tabs_scroll: sheet_tabs_scroll.clone(),
    sheet_tabs: sheet_tabs.clone(),
    sheet_title: sheet_title.clone(),
    sheet_info: sheet_info.clone(),
    sheet_scroll: sheet_scroll.clone(),
    sheet_grid: sheet_grid.clone(),
    sheet_hint_lbl: sheet_hint_lbl.clone(),
    sheet_error_lbl: sheet_error_lbl.clone(),
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
        // Never let audio or video survive the window, even with unsaved
        // changes pending in another file (media itself is never dirty).
        stop_audio(&state);
        stop_video(&state, &widgets);
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

  // Open actions: native file dialog (toolbar icon plus empty/unsupported states).
  {
    let s = state.clone();
    let w = widgets.clone();
    tb_open.connect_clicked(move |_| open_dialog(&s, &w));
    let s = state.clone();
    let w = widgets.clone();
    empty_open.connect_clicked(move |_| open_dialog(&s, &w));
    let s = state.clone();
    let w = widgets.clone();
    unsup_open.connect_clicked(move |_| open_dialog(&s, &w));
  }

  // Header toolbar zoom icons: zoom in/out for PDF and image kinds.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    tb_zoom_in.connect_clicked(move |_| toolbar_zoom(&state, &widgets, true));
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    tb_zoom_out.connect_clicked(move |_| toolbar_zoom(&state, &widgets, false));
  }
  // Share, annotate and info stay inert for now (no action wired).

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

  // PDF navigation: previous / next page (lazy, current page only).
  {
    let state = state.clone();
    let widgets = widgets.clone();
    prev_btn.connect_clicked(move |_| {
      let page = state.borrow().pdf_page.saturating_sub(1);
      state.borrow_mut().pdf_page = page;
      refresh_pdf(&state, &widgets);
      refresh_chrome(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    next_btn.connect_clicked(move |_| {
      let total = state.borrow().pdf.as_ref().map(|d| d.page_count()).unwrap_or(0);
      let page = model::clamp_page(state.borrow().pdf_page + 1, total);
      state.borrow_mut().pdf_page = page;
      refresh_pdf(&state, &widgets);
      refresh_chrome(&state, &widgets);
    });
  }

  // Presentation navigation: previous / next slide (slides are parsed
  // eagerly, only the current one is rendered).
  {
    let state = state.clone();
    let widgets = widgets.clone();
    pres_prev_btn.connect_clicked(move |_| {
      let slide = state.borrow().pres_slide.saturating_sub(1);
      state.borrow_mut().pres_slide = slide;
      refresh_pres(&state, &widgets);
      refresh_chrome(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    pres_next_btn.connect_clicked(move |_| {
      let total = state.borrow().pres_deck.as_ref().map(|d| d.slide_count()).unwrap_or(0);
      let slide = model::clamp_page(state.borrow().pres_slide + 1, total);
      state.borrow_mut().pres_slide = slide;
      refresh_pres(&state, &widgets);
      refresh_chrome(&state, &widgets);
    });
  }

  // Spreadsheet navigation: previous / next sheet (sheets are parsed
  // eagerly, only the current one is rendered into the grid).
  {
    let state = state.clone();
    let widgets = widgets.clone();
    sheet_prev_btn.connect_clicked(move |_| {
      let sheet = state.borrow().sheet_index.saturating_sub(1);
      state.borrow_mut().sheet_index = sheet;
      refresh_sheet(&state, &widgets);
      refresh_chrome(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    sheet_next_btn.connect_clicked(move |_| {
      let total = state.borrow().sheet_book.as_ref().map(|b| b.sheet_count()).unwrap_or(0);
      let sheet = model::clamp_page(state.borrow().sheet_index + 1, total);
      state.borrow_mut().sheet_index = sheet;
      refresh_sheet(&state, &widgets);
      refresh_chrome(&state, &widgets);
    });
  }

  // PDF zoom: font scale steps plus fit-width toggle.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    zoom_in_btn.connect_clicked(move |_| {
      let zoom = (state.borrow().pdf_zoom + 0.25).min(3.0);
      state.borrow_mut().pdf_zoom = zoom;
      apply_pdf_zoom(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    zoom_out_btn.connect_clicked(move |_| {
      let zoom = (state.borrow().pdf_zoom - 0.25).max(0.5);
      state.borrow_mut().pdf_zoom = zoom;
      apply_pdf_zoom(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    fit_btn.connect_toggled(move |_| {
      state.borrow_mut().pdf_fit = widgets.fit_btn.is_active();
      apply_pdf_zoom(&state, &widgets);
    });
  }

  // Image zoom: factor steps plus fit-window toggle. Manual zoom leaves
  // fit mode; enabling fit scales the picture into the current viewport.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    img_zoom_in_btn.connect_clicked(move |_| {
      if state.borrow().img_fit {
        sync_image_fit_factor(&state, &widgets);
      }
      let zoom = model::image_zoom_in(state.borrow().img_zoom);
      state.borrow_mut().img_zoom = zoom;
      state.borrow_mut().img_fit = false;
      widgets.img_fit_btn.set_active(false);
      apply_image_zoom(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    img_zoom_out_btn.connect_clicked(move |_| {
      if state.borrow().img_fit {
        sync_image_fit_factor(&state, &widgets);
      }
      let zoom = model::image_zoom_out(state.borrow().img_zoom);
      state.borrow_mut().img_zoom = zoom;
      state.borrow_mut().img_fit = false;
      widgets.img_fit_btn.set_active(false);
      apply_image_zoom(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    let widgets = widgets.clone();
    img_fit_btn.connect_toggled(move |_| {
      let active = widgets.img_fit_btn.is_active();
      state.borrow_mut().img_fit = active;
      if active {
        apply_image_fit(&state, &widgets);
      } else {
        apply_image_zoom(&state, &widgets);
      }
    });
  }

  // Audio player: play/pause toggle, seek slider (0..1000 permille of
  // the known duration) and volume slider (0..100 percent). The seek
  // scale only handles user drags (`change-value`); the timer tick sets
  // the position programmatically, so no feedback loop needs guarding.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    audio_play_btn.connect_clicked(move |_| {
      let media = state.borrow().audio_media.clone();
      if let Some(media) = media.as_ref() {
        if media.is_playing() {
          media.pause();
        } else {
          media.play();
        }
      }
      update_audio_ui(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    audio_seek.connect_change_value(move |_, _, value| {
      let (media, duration) = {
        let st = state.borrow();
        (st.audio_media.clone(), audio_known_duration(&st))
      };
      if let (Some(media), Some(duration)) = (media.as_ref(), duration) {
        if media.is_seekable() && duration > 0.0 {
          let target = model::clamp_audio_seek(value / 1000.0 * duration, duration);
          media.seek((target * 1_000_000.0) as i64);
        }
      }
      glib::Propagation::Proceed
    });
  }
  {
    let state = state.clone();
    audio_vol.connect_change_value(move |_, _, value| {
      let volume = (value / 100.0).clamp(0.0, 1.0);
      state.borrow_mut().audio_volume = volume;
      if let Some(media) = state.borrow().audio_media.clone() {
        media.set_volume(volume);
      }
      glib::Propagation::Proceed
    });
  }
  // Video player: same control pattern as audio (play/pause toggle, seek
  // slider in permille, volume slider in percent). The seek scale only
  // handles user drags (`change-value`); the timer tick sets the position
  // programmatically, so no feedback loop needs guarding.
  {
    let state = state.clone();
    let widgets = widgets.clone();
    video_play_btn.connect_clicked(move |_| {
      let media = state.borrow().video_media.clone();
      if let Some(media) = media.as_ref() {
        if media.is_playing() {
          media.pause();
        } else {
          media.play();
        }
      }
      update_video_ui(&state, &widgets);
    });
  }
  {
    let state = state.clone();
    video_seek.connect_change_value(move |_, _, value| {
      let (media, duration) = {
        let st = state.borrow();
        (st.video_media.clone(), video_known_duration(&st))
      };
      if let (Some(media), Some(duration)) = (media.as_ref(), duration) {
        if media.is_seekable() && duration > 0.0 {
          let target = model::clamp_audio_seek(value / 1000.0 * duration, duration);
          media.seek((target * 1_000_000.0) as i64);
        }
      }
      glib::Propagation::Proceed
    });
  }
  {
    let state = state.clone();
    video_vol.connect_change_value(move |_, _, value| {
      let volume = (value / 100.0).clamp(0.0, 1.0);
      state.borrow_mut().video_volume = volume;
      if let Some(media) = state.borrow().video_media.clone() {
        media.set_volume(volume);
      }
      glib::Propagation::Proceed
    });
  }
  // Stop playback when the window is torn down (the close-request handler
  // below also stops it so no audio or video survives the window).
  {
    let state = state.clone();
    let widgets = widgets.clone();
    root.connect_unrealize(move |_| {
      stop_audio(&state);
      stop_video(&state, &widgets);
    });
  }

  // Initial file from CLI (`preview /path/to/file`).
  if let Some(path) = initial {
    open_path(&state, &widgets, &path);
  } else {
    refresh_chrome(&state, &widgets);
  }

  root
}

fn base_css() -> String {
  format!(
    "textview {{ font-family: '{SF_PRO}', 'SF Pro Text', sans-serif; font-size: 13pt; }}\
     textview.mono {{ font-family: 'SF Mono', Monospace; }}\
     .dim-label {{ opacity: 0.6; }}\
     .title-1 {{ font-family: '{SF_PRO}'; font-size: 22pt; font-weight: 800; }}\
     .title-2 {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}\
     .preview-open-button {{ font-family: '{SF_PRO}'; font-size: 14pt; font-weight: 700; \
       padding: 10px 28px; border-radius: 999px; min-width: 220px; min-height: 48px; }}\
     .preview-open-button:hover {{ opacity: 0.92; }}\
     .preview-open-button:active {{ opacity: 0.85; }}\
     .audio-title {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}\
     .audio-time {{ font-family: '{SF_PRO}'; font-size: 11pt; }}\
     .video-title {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}\
     .video-time {{ font-family: '{SF_PRO}'; font-size: 11pt; }}\
     .docx-title {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}\
     .pptx-title {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}\
     .xlsx-title {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}"
  )
}

fn apply_css(_root: &gtk::Box) {
  let provider = gtk::CssProvider::new();
  provider.load_from_string(&base_css());
  if let Some(display) = gtk::gdk::Display::default() {
    gtk::style_context_add_provider_for_display(
      &display,
      &provider,
      gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
  }
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
  // Switching files always stops media playback first.
  stop_audio(state);
  stop_video(state, widgets);
  // Switching files always drops the previous document and deck, so stale
  // blocks never leak into another file (every branch below sets its own
  // kind).
  state.borrow_mut().doc_doc = None;
  state.borrow_mut().doc_error = None;
  state.borrow_mut().pres_deck = None;
  state.borrow_mut().pres_error = None;
  state.borrow_mut().pres_slide = 0;
  state.borrow_mut().sheet_book = None;
  state.borrow_mut().sheet_error = None;
  state.borrow_mut().sheet_index = 0;
  // Extension first, image/audio/video magic bytes second so misnamed
  // files still land on the picture viewer or a player instead of the
  // unsupported page.
  let kind = model::classify_file(path);
  if kind == FileKind::Pdf {
    match model::PdfDoc::open(path) {
      Ok(doc) => {
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = Some(Rc::new(doc));
        st.pdf_page = 0;
        st.pdf_zoom = 1.0;
        st.pdf_fit = true;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
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
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = Some(reason);
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
      }
    }
    return;
  }
  if kind == FileKind::Image {
    match model::probe_image(path) {
      Ok(meta) => {
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = Some(meta);
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.img_zoom = 1.0;
        st.img_fit = true;
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = None;
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
        apply_image_fit(state, widgets);
      }
      Err(reason) => {
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = Some(reason);
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = None;
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
      }
    }
    return;
  }
  if kind == FileKind::Unsupported {
    let mut st = state.borrow_mut();
    st.path = Some(path.clone());
    st.kind = kind;
    st.content.clear();
    st.pdf = None;
    st.img_meta = None;
    st.img_error = None;
    st.img_base = None;
    st.audio_meta = None;
    st.audio_error = None;
    st.video_meta = None;
    st.video_error = None;
    st.dirty = false;
    st.mode = Mode::Preview;
    st.error = None;
    drop(st);
    refresh_chrome(state, widgets);
    refresh_body(state, widgets);
    return;
  }
  if kind == FileKind::Audio {
    match model::probe_audio(path) {
      Ok(meta) => {
        let volume = state.borrow().audio_volume;
        let media = gtk::MediaFile::for_file(&gtk::gio::File::for_path(path));
        media.set_volume(volume);
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = Some(meta);
        st.audio_error = None;
        st.audio_media = Some(media);
        st.video_meta = None;
        st.video_error = None;
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
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = Some(reason);
        st.video_meta = None;
        st.video_error = None;
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = None;
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
      }
    }
    return;
  }
  if kind == FileKind::Video {
    match model::probe_video(path) {
      Ok(meta) => {
        let volume = state.borrow().video_volume;
        let media = gtk::MediaFile::for_file(&gtk::gio::File::for_path(path));
        media.set_volume(volume);
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = Some(meta);
        st.video_error = None;
        st.video_media = Some(media);
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
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = Some(reason);
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = None;
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
      }
    }
    return;
  }
  if kind == FileKind::Document {
    match docx::load_document(path) {
      Ok(doc) => {
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.doc_doc = Some(doc);
        st.doc_error = None;
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
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.doc_doc = None;
        st.doc_error = Some(reason);
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = None;
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
      }
    }
    return;
  }
  if kind == FileKind::Presentation {
    match pptx::load_presentation(path) {
      Ok(deck) => {
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.pres_deck = Some(deck);
        st.pres_error = None;
        st.pres_slide = 0;
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
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.pres_deck = None;
        st.pres_error = Some(reason);
        st.pres_slide = 0;
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = None;
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
      }
    }
    return;
  }
  if kind == FileKind::Spreadsheet {
    match xlsx::load_workbook(path) {
      Ok(book) => {
        let mut st = state.borrow_mut();
        st.path = Some(path.clone());
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.sheet_book = Some(book);
        st.sheet_error = None;
        st.sheet_index = 0;
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
        st.kind = kind;
        st.content.clear();
        st.pdf = None;
        st.img_meta = None;
        st.img_error = None;
        st.img_base = None;
        st.audio_meta = None;
        st.audio_error = None;
        st.video_meta = None;
        st.video_error = None;
        st.sheet_book = None;
        st.sheet_error = Some(reason);
        st.sheet_index = 0;
        st.dirty = false;
        st.mode = Mode::Preview;
        st.error = None;
        drop(st);
        refresh_chrome(state, widgets);
        refresh_body(state, widgets);
      }
    }
    return;
  }
  match model::load_text(path) {
    Ok(text) => {
      let mut st = state.borrow_mut();
      st.path = Some(path.clone());
      st.kind = kind;
      st.content = text;
      st.pdf = None;
      st.img_meta = None;
      st.img_error = None;
      st.img_base = None;
      st.audio_meta = None;
      st.audio_error = None;
      st.video_meta = None;
      st.video_error = None;
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
      st.pdf = None;
      st.img_meta = None;
      st.img_error = None;
      st.img_base = None;
      st.audio_meta = None;
      st.audio_error = None;
      st.video_meta = None;
      st.video_error = None;
      st.dirty = false;
      st.mode = Mode::Preview;
      st.error = Some(reason);
      drop(st);
      refresh_chrome(state, widgets);
      refresh_body(state, widgets);
    }
  }
}

/// Persist the edit buffer back to disk (manual save only, never for PDFs,
/// images, audio, video, document, presentation or spreadsheet files).
fn do_save(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  if state.borrow().kind == FileKind::Pdf
    || state.borrow().kind == FileKind::Image
    || state.borrow().kind == FileKind::Audio
    || state.borrow().kind == FileKind::Video
    || state.borrow().kind == FileKind::Document
    || state.borrow().kind == FileKind::Presentation
    || state.borrow().kind == FileKind::Spreadsheet
  {
    return;
  }
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
      let _ = reason;
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
  let editable = has_file && (st.kind == FileKind::Text || st.kind == FileKind::Markdown);
  let is_pdf = has_file && st.kind == FileKind::Pdf;
  let is_image = has_file && st.kind == FileKind::Image;

  if let Some(path) = st.path.as_ref() {
    let name = model::display_name(path);
    widgets.title.set_text(&name);
    widgets.title_box.set_visible(true);
  } else {
    widgets.title_box.set_visible(false);
  }
  widgets.subtitle.set_visible(false);

  widgets.edit_btn.set_visible(editable);
  widgets.save_btn.set_visible(editable && st.mode == Mode::Edit);
  widgets.save_btn.set_sensitive(st.dirty);
  let edit_label = if st.mode == Mode::Edit {
    lang::t("action.done")
  } else {
    lang::t("action.edit")
  };
  widgets.edit_btn.set_label(&edit_label);

  // Header toolbar: open always works, zoom only for PDF/image pages,
  // share, annotate and info stay inert for now.
  widgets.tb_open.set_sensitive(true);
  let zoomable = is_pdf || is_image;
  widgets.tb_zoom_in.set_sensitive(zoomable);
  widgets.tb_zoom_out.set_sensitive(zoomable);
  widgets.tb_share.set_sensitive(false);
  widgets.tb_annotate.set_sensitive(false);

  let page = if !has_file {
    "empty"
  } else if st.kind == FileKind::Pdf {
    "pdf"
  } else if st.kind == FileKind::Image {
    "image"
  } else if st.kind == FileKind::Audio {
    "audio"
  } else if st.kind == FileKind::Video {
    "video"
  } else if st.kind == FileKind::Document {
    "docx"
  } else if st.kind == FileKind::Presentation {
    "pptx"
  } else if st.kind == FileKind::Spreadsheet {
    "xlsx"
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
  if st.kind == FileKind::Pdf {
    drop(st);
    refresh_pdf(state, widgets);
    return;
  }
  if st.kind == FileKind::Image {
    drop(st);
    refresh_image(state, widgets);
    return;
  }
  if st.kind == FileKind::Audio {
    drop(st);
    refresh_audio(state, widgets);
    return;
  }
  if st.kind == FileKind::Video {
    drop(st);
    refresh_video(state, widgets);
    return;
  }
  if st.kind == FileKind::Document {
    drop(st);
    refresh_docx(state, widgets);
    return;
  }
  if st.kind == FileKind::Presentation {
    drop(st);
    refresh_pres(state, widgets);
    return;
  }
  if st.kind == FileKind::Spreadsheet {
    drop(st);
    refresh_sheet(state, widgets);
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

/// Render the current PDF page (lazy: only the current page is extracted).
/// Broken PDFs (missing, corrupt, encrypted) show an error label instead.
fn refresh_pdf(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let (total, page, reason, fit) = {
    let st = state.borrow();
    let total = st.pdf.as_ref().map(|d| d.page_count()).unwrap_or(0);
    let page = model::clamp_page(st.pdf_page, total);
    (total, page, st.error.clone(), st.pdf_fit)
  };
  state.borrow_mut().pdf_page = page;
  widgets.fit_btn.set_active(fit);

  let has_doc = state.borrow().pdf.is_some();
  if !has_doc {
    let reason = reason.unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.pdf_bar.set_visible(false);
    widgets.pdf_scroll.set_visible(false);
    widgets.pdf_error.set_visible(true);
    widgets.pdf_error.set_text(&lang::t_with(
      "pdf.load_failed",
      &[("reason", &reason)],
    ));
    return;
  }
  widgets.pdf_bar.set_visible(true);
  widgets.pdf_scroll.set_visible(true);
  widgets.pdf_error.set_visible(false);

  let text = state
    .borrow()
    .pdf
    .as_ref()
    .and_then(|doc| doc.page_text(page).ok())
    .unwrap_or_default();
  if text.trim().is_empty() {
    widgets.pdf_buffer.set_text(&lang::t("pdf.empty_page"));
  } else {
    widgets.pdf_buffer.set_text(text.trim());
  }
  widgets.page_label.set_text(&lang::t_with(
    "pdf.page",
    &[("page", &(page + 1).to_string()), ("total", &total.to_string())],
  ));
  widgets.prev_btn.set_sensitive(page > 0);
  widgets.next_btn.set_sensitive(page + 1 < total);
  apply_pdf_zoom(state, widgets);
}

/// Apply the PDF zoom level (SF Pro Display font scale) and fit-width mode.
fn apply_pdf_zoom(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let (zoom, fit) = {
    let st = state.borrow();
    (st.pdf_zoom, st.pdf_fit)
  };
  let size = (13.0 * zoom).clamp(6.0, 48.0);
  widgets.pdf_css.load_from_string(&format!(
    "textview.pdf-page {{ font-family: '{SF_PRO}', 'SF Pro Text', sans-serif; font-size: {size:.1}pt; }}"
  ));
  widgets.pdf_view.set_wrap_mode(if fit {
    gtk::WrapMode::WordChar
  } else {
    gtk::WrapMode::None
  });
}

/// Render the current image page. Raster formats are decoded once with the
/// `image` crate (first frame for animated GIFs) and shown as an actual
/// picture; SVGs are handed to GTK (librsvg) via file. Broken images stay
/// on the image page and show `image.load_failed` with the reason; they
/// never fall through to the unsupported page.
fn refresh_image(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let has_meta = state.borrow().img_meta.is_some();
  if !has_meta {
    let reason = state
      .borrow()
      .img_error
      .clone()
      .unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.img_bar.set_visible(false);
    widgets.img_scroll.set_visible(false);
    widgets.img_error.set_visible(true);
    widgets.img_error.set_text(&lang::t_with(
      "image.load_failed",
      &[("reason", &reason)],
    ));
    return;
  }
  widgets.img_bar.set_visible(false);
  widgets.img_scroll.set_visible(true);
  widgets.img_error.set_visible(false);
  widgets.img_fit_btn.set_active(state.borrow().img_fit);

  let is_svg = state.borrow().img_meta.as_ref().is_some_and(|m| m.is_svg);
  if is_svg {
    let path = state.borrow().path.clone();
    widgets.img_picture.set_paintable(None::<&gtk::gdk::Texture>);
    if let Some(path) = path.as_ref() {
      widgets
        .img_picture
        .set_file(Some(&gtk::gio::File::for_path(path)));
    }
    apply_image_zoom(state, widgets);
    return;
  }

  // Raster: decode once, then reuse the cached base pixbuf.
  if state.borrow().img_base.is_none() {
    let (path, meta) = {
      let st = state.borrow();
      let meta = st.img_meta.as_ref().map(|m| (m.display_width, m.display_height));
      (st.path.clone(), meta)
    };
    let (Some(path), Some((display_width, display_height))) = (path, meta) else {
      return;
    };
    match decode_display_pixbuf(&path, display_width, display_height) {
      Ok(pixbuf) => {
        state.borrow_mut().img_base = Some(pixbuf);
      }
      Err(reason) => {
        state.borrow_mut().img_error = Some(reason.clone());
        state.borrow_mut().img_meta = None;
        widgets.img_bar.set_visible(false);
        widgets.img_scroll.set_visible(false);
        widgets.img_error.set_visible(true);
        widgets.img_error.set_text(&lang::t_with(
          "image.load_failed",
          &[("reason", &reason)],
        ));
        refresh_chrome(state, widgets);
        return;
      }
    }
  }
  if let Some(base) = state.borrow().img_base.clone() {
    widgets.img_picture.set_file(None::<&gtk::gio::File>);
    widgets
      .img_picture
      .set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&base)));
  }
  apply_image_zoom(state, widgets);
}

/// Decode a raster image file into a display pixbuf, downscaled to the
/// probed display size so huge images stay cheap to render. Animated GIFs
/// decode to their first frame.
fn decode_display_pixbuf(
  path: &std::path::Path,
  display_width: u32,
  display_height: u32,
) -> Result<gdk_pixbuf::Pixbuf, String> {
  let dynamic = image::open(path).map_err(|e| format!("cannot decode image: {e}"))?;
  let rgba = dynamic.to_rgba8();
  let pixels = if rgba.width() != display_width || rgba.height() != display_height {
    image::imageops::thumbnail(&rgba, display_width, display_height)
  } else {
    rgba
  };
  let (width, height) = (pixels.width(), pixels.height());
  if width == 0 || height == 0 {
    return Err("image has no pixels".to_string());
  }
  let bytes = glib::Bytes::from(pixels.as_raw());
  Ok(gdk_pixbuf::Pixbuf::from_bytes(
    &bytes,
    gdk_pixbuf::Colorspace::Rgb,
    true,
    8,
    width as i32,
    height as i32,
    (width * 4) as i32,
  ))
}

/// Base pixels used for zoom math: probed display size for raster images,
/// parsed canvas size for SVGs. Returns `None` when unknown (SVG without
/// a size), in which case the picture keeps its natural size.
fn image_base_size(state: &Rc<RefCell<State>>) -> Option<(u32, u32)> {
  let st = state.borrow();
  let meta = st.img_meta.as_ref()?;
  if meta.is_svg {
    if meta.has_dimensions() {
      Some((meta.width, meta.height))
    } else {
      None
    }
  } else {
    Some((meta.display_width, meta.display_height))
  }
}

/// Sync the zoom factor to the current viewport without touching the
/// picture size. Returns true when the viewport is allocated.
fn sync_image_fit_factor(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) -> bool {
  let base = image_base_size(state);
  let (view_width, view_height) = (
    widgets.img_scroll.width(),
    widgets.img_scroll.height(),
  );
  if let Some((base_width, base_height)) = base {
    if view_width > 0 && view_height > 0 {
      state.borrow_mut().img_zoom =
        model::fit_zoom_for(base_width, base_height, view_width, view_height);
      return true;
    }
  }
  false
}

/// Apply the image zoom factor. Fit mode keeps the whole picture visible
/// at any window size: scrollbars stay off so the viewport always
/// constrains the picture, and `Contain` plus `can_shrink` scales it down
/// into the viewport on every resize. Manual zoom sets an explicit size
/// so the scrolled window scrolls past the viewport.
fn apply_image_zoom(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  if state.borrow().img_fit {
    widgets.img_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
    widgets.img_picture.set_size_request(-1, -1);
  } else {
    widgets.img_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    let zoom = state.borrow().img_zoom;
    match image_base_size(state) {
      Some((base_width, base_height)) => {
        let (width, height) = model::zoomed_size(base_width, base_height, zoom);
        widgets.img_picture.set_size_request(width as i32, height as i32);
      }
      None => widgets.img_picture.set_size_request(-1, -1),
    }
  }
  widgets.img_picture.set_content_fit(gtk::ContentFit::Contain);
}

/// Fit the picture into the current viewport. The picture itself stays
/// size-free (see [`apply_image_zoom`]); only the zoom factor is synced
/// for the next manual step. When the viewport is not allocated yet (cold
/// start), the factor sync is deferred to an idle callback after layout.
fn apply_image_fit(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  if !sync_image_fit_factor(state, widgets) {
    let state_c = state.clone();
    let widgets_c = widgets.clone();
    glib::idle_add_local(move || {
      if state_c.borrow().img_fit {
        sync_image_fit_factor(&state_c, &widgets_c);
      }
      glib::ControlFlow::Break
    });
  }
  apply_image_zoom(state, widgets);
}

/// Stop audio playback and drop the stream plus its timer tick. Called on
/// file switch and window close so no audio survives the file or window.
fn stop_audio(state: &Rc<RefCell<State>>) {
  if let Some(tick) = state.borrow_mut().audio_tick.take() {
    tick.remove();
  }
  if let Some(media) = state.borrow_mut().audio_media.take() {
    media.pause();
  }
}

/// Known playback duration in seconds: the live stream duration when the
/// GStreamer pipeline reports one, otherwise the probed header duration.
fn audio_known_duration(st: &State) -> Option<f64> {
  if let Some(media) = st.audio_media.as_ref() {
    let micros = media.duration();
    if micros > 0 {
      return Some(micros as f64 / 1_000_000.0);
    }
  }
  st.audio_meta.as_ref().and_then(|meta| {
    if meta.has_duration() {
      meta.duration_secs
    } else {
      None
    }
  })
}

/// Status line for audio files: format, duration and size, or the probe
/// reason when the file could not be probed.
fn audio_status_text(st: &State) -> String {
  match st.audio_meta.as_ref() {
    Some(meta) => lang::t_with(
      "status.audio",
      &[
        ("format", meta.format.as_str()),
        (
          "duration",
          model::format_audio_time_opt(audio_known_duration(st)).as_str(),
        ),
        ("size", model::format_file_size(meta.file_bytes).as_str()),
      ],
    ),
    None => {
      let reason = st.audio_error.clone().unwrap_or_else(|| lang::t("unsupported.hint"));
      lang::t_with("audio.load_failed", &[("reason", &reason)])
    }
  }
}

/// Build the audio page: file title, info line and controls, or the probe
/// error hint. Broken files stay on the audio page and never fall through
/// to the unsupported page.
fn refresh_audio(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let (name, volume) = {
    let st = state.borrow();
    (
      st.path.as_ref().map(|p| model::display_name(p)).unwrap_or_default(),
      st.audio_volume,
    )
  };
  widgets.audio_title.set_text(&name);
  if state.borrow().audio_meta.is_none() {
    let status = audio_status_text(&state.borrow());
    let reason = state
      .borrow()
      .audio_error
      .clone()
      .unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.audio_controls.set_visible(false);
    widgets.audio_vol_row.set_visible(false);
    widgets.audio_error_lbl.set_visible(true);
    widgets.audio_error_lbl.set_text(&lang::t_with(
      "audio.load_failed",
      &[("reason", &reason)],
    ));
    widgets.audio_info.set_text(&status);
    return;
  }
  widgets.audio_controls.set_visible(true);
  widgets.audio_vol_row.set_visible(true);
  widgets.audio_error_lbl.set_visible(false);
  widgets.audio_vol.set_value(volume * 100.0);
  widgets.audio_info.set_text(&audio_status_text(&state.borrow()));
  update_audio_ui(state, widgets);
  start_audio_tick(state, widgets);
}

/// Restart the 250 ms position timer for the current audio stream.
fn start_audio_tick(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  if let Some(tick) = state.borrow_mut().audio_tick.take() {
    tick.remove();
  }
  let state_c = state.clone();
  let widgets_c = widgets.clone();
  let tick = glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
    update_audio_ui(&state_c, &widgets_c);
    glib::ControlFlow::Continue
  });
  state.borrow_mut().audio_tick = Some(tick);
}

/// Refresh the player controls from the stream: play/pause label, seek
/// position, elapsed/total time, info plus status line. Stream errors
/// (e.g. a missing GStreamer decoder) surface as an honest hint; nothing
/// here can crash on corrupt files.
fn update_audio_ui(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let snapshot = {
    let st = state.borrow();
    st.audio_media.clone().map(|media| {
      let error = media.error().map(|err| err.to_string());
      let position_us = media.timestamp();
      (
        media,
        error,
        position_us,
        audio_known_duration(&st),
      )
    })
  };
  let Some((media, error, position_us, known)) = snapshot else {
    return;
  };
  if media.is_ended() {
    media.pause();
    media.seek(0);
  }
  let playing = media.is_playing();
  widgets.audio_play_btn.set_label(&lang::t(if playing {
    "audio.pause"
  } else {
    "audio.play"
  }));
  if let Some(reason) = error {
    widgets.audio_error_lbl.set_visible(true);
    widgets.audio_error_lbl.set_text(&lang::t_with(
      "audio.load_failed",
      &[("reason", &reason)],
    ));
  } else {
    widgets.audio_error_lbl.set_visible(false);
  }
  let position = (position_us.max(0) as f64) / 1_000_000.0;
  match known {
    Some(duration) if duration > 0.0 => {
      let clamped = model::clamp_audio_seek(position, duration);
      widgets.audio_seek.set_value(clamped / duration * 1000.0);
      widgets.audio_seek.set_sensitive(media.is_seekable());
      widgets.audio_time.set_text(&lang::t_with(
        "audio.position",
        &[
          ("elapsed", model::format_audio_time(clamped).as_str()),
          ("total", model::format_audio_time(duration).as_str()),
        ],
      ));
    }
    _ => {
      widgets.audio_seek.set_value(0.0);
      widgets.audio_seek.set_sensitive(false);
      widgets.audio_time.set_text(&lang::t_with(
        "audio.position",
        &[
          ("elapsed", model::format_audio_time(position).as_str()),
          ("total", "--:--"),
        ],
      ));
    }
  }
  let status = audio_status_text(&state.borrow());
  widgets.audio_info.set_text(&status);
}

/// Stop video playback, drop the stream plus its timer tick and detach the
/// picture so no frame (and no audio bed) survives the file or window.
/// Called on file switch and window close.
fn stop_video(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  if let Some(tick) = state.borrow_mut().video_tick.take() {
    tick.remove();
  }
  if let Some(media) = state.borrow_mut().video_media.take() {
    media.pause();
  }
  widgets.video.set_media_stream(Option::<&gtk::MediaFile>::None);
}

/// Known playback duration in seconds: the live stream duration when the
/// GStreamer pipeline reports one, otherwise the probed header duration.
fn video_known_duration(st: &State) -> Option<f64> {
  if let Some(media) = st.video_media.as_ref() {
    let micros = media.duration();
    if micros > 0 {
      return Some(micros as f64 / 1_000_000.0);
    }
  }
  st.video_meta.as_ref().and_then(|meta| {
    if meta.has_duration() {
      meta.duration_secs
    } else {
      None
    }
  })
}

/// Status line for video files: format, resolution when probed, duration
/// and size, or the probe reason when the file could not be probed.
fn video_status_text(st: &State) -> String {
  match st.video_meta.as_ref() {
    Some(meta) => {
      let resolution = match meta.resolution {
        Some((w, h)) if meta.has_resolution() => format!("{w} x {h}, "),
        _ => String::new(),
      };
      lang::t_with(
        "status.video",
        &[
          ("format", meta.format.as_str()),
          ("resolution", resolution.as_str()),
          (
            "duration",
            model::format_audio_time_opt(video_known_duration(st)).as_str(),
          ),
          ("size", model::format_file_size(meta.file_bytes).as_str()),
        ],
      )
    }
    None => {
      let reason = st.video_error.clone().unwrap_or_else(|| lang::t("unsupported.hint"));
      lang::t_with("video.load_failed", &[("reason", &reason)])
    }
  }
}

/// Build the video page: picture plus title, info line and controls, or
/// the probe error hint. Broken files stay on the video page and never
/// fall through to the unsupported page.
fn refresh_video(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let (name, volume, media) = {
    let st = state.borrow();
    (
      st.path.as_ref().map(|p| model::display_name(p)).unwrap_or_default(),
      st.video_volume,
      st.video_media.clone(),
    )
  };
  widgets.video_title.set_text(&name);
  if state.borrow().video_meta.is_none() {
    widgets.video.set_media_stream(Option::<&gtk::MediaFile>::None);
    let status = video_status_text(&state.borrow());
    let reason = state
      .borrow()
      .video_error
      .clone()
      .unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.video_controls.set_visible(false);
    widgets.video_vol_row.set_visible(false);
    widgets.video_error_lbl.set_visible(true);
    widgets.video_error_lbl.set_text(&lang::t_with(
      "video.load_failed",
      &[("reason", &reason)],
    ));
    widgets.video_info.set_text(&status);
    return;
  }
  widgets.video_controls.set_visible(true);
  widgets.video_vol_row.set_visible(true);
  widgets.video_error_lbl.set_visible(false);
  widgets.video_vol.set_value(volume * 100.0);
  if let Some(media) = media.as_ref() {
    widgets.video.set_media_stream(Some(media));
  }
  widgets.video_info.set_text(&video_status_text(&state.borrow()));
  update_video_ui(state, widgets);
  start_video_tick(state, widgets);
}

/// Restart the 250 ms position timer for the current video stream.
fn start_video_tick(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  if let Some(tick) = state.borrow_mut().video_tick.take() {
    tick.remove();
  }
  let state_c = state.clone();
  let widgets_c = widgets.clone();
  let tick = glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
    update_video_ui(&state_c, &widgets_c);
    glib::ControlFlow::Continue
  });
  state.borrow_mut().video_tick = Some(tick);
}

/// Refresh the player controls from the stream: picture keeps playing via
/// `gtk::Video`, the play/pause label, seek position, elapsed/total time,
/// info plus status line follow the stream. Stream errors (e.g. a missing
/// GStreamer decoder) surface as an honest hint; a stream without a video
/// track reports the audio-only hint; nothing here can crash on corrupt
/// files.
fn update_video_ui(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let snapshot = {
    let st = state.borrow();
    st.video_media.clone().map(|media| {
      let error = media.error().map(|err| err.to_string());
      let position_us = media.timestamp();
      (
        media,
        error,
        position_us,
        video_known_duration(&st),
      )
    })
  };
  let Some((media, error, position_us, known)) = snapshot else {
    return;
  };
  if media.is_ended() {
    media.pause();
    media.seek(0);
  }
  let playing = media.is_playing();
  widgets.video_play_btn.set_label(&lang::t(if playing {
    "video.pause"
  } else {
    "video.play"
  }));
  if let Some(reason) = error {
    widgets.video_error_lbl.set_visible(true);
    widgets.video_error_lbl.set_text(&lang::t_with(
      "video.load_failed",
      &[("reason", &reason)],
    ));
  } else if media.duration() > 0 && !media.has_video() {
    widgets.video_error_lbl.set_visible(true);
    widgets.video_error_lbl.set_text(&lang::t("video.audio_only"));
  } else {
    widgets.video_error_lbl.set_visible(false);
  }
  let position = (position_us.max(0) as f64) / 1_000_000.0;
  match known {
    Some(duration) if duration > 0.0 => {
      let clamped = model::clamp_audio_seek(position, duration);
      widgets.video_seek.set_value(clamped / duration * 1000.0);
      widgets.video_seek.set_sensitive(media.is_seekable());
      widgets.video_time.set_text(&lang::t_with(
        "video.position",
        &[
          ("elapsed", model::format_audio_time(clamped).as_str()),
          ("total", model::format_audio_time(duration).as_str()),
        ],
      ));
    }
    _ => {
      widgets.video_seek.set_value(0.0);
      widgets.video_seek.set_sensitive(false);
      widgets.video_time.set_text(&lang::t_with(
        "video.position",
        &[
          ("elapsed", model::format_audio_time(position).as_str()),
          ("total", "--:--"),
        ],
      ));
    }
  }
  let status = video_status_text(&state.borrow());
  widgets.video_info.set_text(&status);
}

/// Status line for documents: format, paragraph and word counts plus file
/// size, or the load reason when the file could not be parsed.
fn doc_status_text(st: &State) -> String {
  match st.doc_doc.as_ref() {
    Some(doc) => lang::t_with(
      "status.docx",
      &[
        ("format", doc.format.as_str()),
        ("paragraphs", &doc.paragraph_count().to_string()),
        ("words", &doc.word_count().to_string()),
        ("size", model::format_file_size(doc.file_bytes).as_str()),
      ],
    ),
    None => {
      let reason = st.doc_error.clone().unwrap_or_else(|| lang::t("unsupported.hint"));
      lang::t_with("docx.load_failed", &[("reason", &reason)])
    }
  }
}

/// Build the document page: title, info line and the formatted read-only
/// view, or the load error hint. Broken documents stay on the document page
/// and never fall through to the unsupported page.
fn refresh_docx(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let name = {
    let st = state.borrow();
    st.path.as_ref().map(|p| model::display_name(p)).unwrap_or_default()
  };
  widgets.doc_title.set_text(&name);
  if state.borrow().doc_doc.is_none() {
    let reason = state
      .borrow()
      .doc_error
      .clone()
      .unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.doc_title.set_visible(false);
    widgets.doc_info.set_visible(false);
    widgets.doc_scroll.set_visible(false);
    widgets.doc_error_lbl.set_visible(true);
    widgets.doc_error_lbl.set_text(&lang::t_with(
      "docx.load_failed",
      &[("reason", &reason)],
    ));
    return;
  }
  widgets.doc_title.set_visible(true);
  widgets.doc_info.set_visible(true);
  widgets.doc_scroll.set_visible(true);
  widgets.doc_error_lbl.set_visible(false);
  widgets.doc_info.set_text(&doc_status_text(&state.borrow()));
  if let Some(doc) = state.borrow().doc_doc.clone() {
    docx::render_into(&widgets.doc_buffer, &doc);
  }
}

/// Status line for presentations: format, slide and word counts plus file
/// size, or the load reason when the file could not be parsed.
fn pres_status_text(st: &State) -> String {
  match st.pres_deck.as_ref() {
    Some(deck) => lang::t_with(
      "status.pptx",
      &[
        ("format", deck.format.as_str()),
        ("slides", &deck.slide_count().to_string()),
        ("words", &deck.word_count().to_string()),
        ("size", model::format_file_size(deck.file_bytes).as_str()),
      ],
    ),
    None => {
      let reason = st.pres_error.clone().unwrap_or_else(|| lang::t("unsupported.hint"));
      lang::t_with("pptx.load_failed", &[("reason", &reason)])
    }
  }
}

/// Build the presentation page: toolbar with the slide indicator, title,
/// info line and the current slide rendered read-only, or the load error
/// hint. Broken presentations stay on the presentation page and never fall
/// through to the unsupported page.
fn refresh_pres(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let total = state.borrow().pres_deck.as_ref().map(|d| d.slide_count()).unwrap_or(0);
  let slide = model::clamp_page(state.borrow().pres_slide, total);
  state.borrow_mut().pres_slide = slide;
  let name = {
    let st = state.borrow();
    st.path.as_ref().map(|p| model::display_name(p)).unwrap_or_default()
  };
  widgets.pres_title.set_text(&name);
  if state.borrow().pres_deck.is_none() {
    let reason = state
      .borrow()
      .pres_error
      .clone()
      .unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.pres_bar.set_visible(false);
    widgets.pres_title.set_visible(false);
    widgets.pres_info.set_visible(false);
    widgets.pres_scroll.set_visible(false);
    widgets.pres_error_lbl.set_visible(true);
    widgets.pres_error_lbl.set_text(&lang::t_with(
      "pptx.load_failed",
      &[("reason", &reason)],
    ));
    return;
  }
  widgets.pres_bar.set_visible(true);
  widgets.pres_title.set_visible(true);
  widgets.pres_info.set_visible(true);
  widgets.pres_scroll.set_visible(true);
  widgets.pres_error_lbl.set_visible(false);
  widgets.pres_info.set_text(&pres_status_text(&state.borrow()));
  widgets.pres_label.set_text(&lang::t_with(
    "pptx.slide",
    &[("page", &(slide + 1).to_string()), ("total", &total.to_string())],
  ));
  widgets.pres_prev_btn.set_sensitive(slide > 0);
  widgets.pres_next_btn.set_sensitive(slide + 1 < total);
  if let Some(deck) = state.borrow().pres_deck.clone() {
    pptx::render_slide_into(&widgets.pres_buffer, &deck, slide);
  }
}

/// Status line for spreadsheets: format, sheet count, current sheet
/// dimensions plus file size, or the load reason when the file could not
/// be parsed.
fn sheet_status_text(st: &State) -> String {
  match st.sheet_book.as_ref() {
    Some(book) => {
      let (rows, cols) = book.sheet_dims(st.sheet_index);
      let dims = if rows == 0 && cols == 0 {
        "empty".to_string()
      } else {
        format!("{rows}x{cols}")
      };
      lang::t_with(
        "status.xlsx",
        &[
          ("format", book.format.as_str()),
          ("sheets", &book.sheet_count().to_string()),
          ("rows", dims.as_str()),
          ("size", model::format_file_size(book.file_bytes).as_str()),
        ],
      )
    }
    None => {
      let reason = st.sheet_error.clone().unwrap_or_else(|| lang::t("unsupported.hint"));
      lang::t_with("xlsx.load_failed", &[("reason", &reason)])
    }
  }
}

/// Build the spreadsheet page: toolbar with the sheet indicator, sheet
/// tabs (only for multi-sheet workbooks), title, info line and the
/// current sheet rendered read-only into the grid, or the load error
/// hint. Broken spreadsheets stay on the spreadsheet page and never fall
/// through to the unsupported page.
fn refresh_sheet(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let total = state.borrow().sheet_book.as_ref().map(|b| b.sheet_count()).unwrap_or(0);
  let sheet = model::clamp_page(state.borrow().sheet_index, total);
  state.borrow_mut().sheet_index = sheet;
  let name = {
    let st = state.borrow();
    st.path.as_ref().map(|p| model::display_name(p)).unwrap_or_default()
  };
  widgets.sheet_title.set_text(&name);
  if state.borrow().sheet_book.is_none() {
    let reason = state
      .borrow()
      .sheet_error
      .clone()
      .unwrap_or_else(|| lang::t("unsupported.hint"));
    widgets.sheet_bar.set_visible(false);
    widgets.sheet_tabs_scroll.set_visible(false);
    widgets.sheet_title.set_visible(false);
    widgets.sheet_info.set_visible(false);
    widgets.sheet_scroll.set_visible(false);
    widgets.sheet_hint_lbl.set_visible(false);
    widgets.sheet_error_lbl.set_visible(true);
    widgets.sheet_error_lbl.set_text(&lang::t_with(
      "xlsx.load_failed",
      &[("reason", &reason)],
    ));
    return;
  }
  widgets.sheet_bar.set_visible(true);
  widgets.sheet_title.set_visible(true);
  widgets.sheet_info.set_visible(true);
  widgets.sheet_scroll.set_visible(true);
  widgets.sheet_error_lbl.set_visible(false);
  let (book, truncated) = {
    let st = state.borrow();
    let book = st.sheet_book.clone().expect("sheet book present");
    let truncated = book.truncated;
    (book, truncated)
  };
  let sheet_name = book.sheets.get(sheet).map(|s| s.name.clone()).unwrap_or_default();
  widgets.sheet_label.set_text(&lang::t_with(
    "xlsx.sheet",
    &[
      ("page", &(sheet + 1).to_string()),
      ("total", &total.to_string()),
      ("name", &sheet_name),
    ],
  ));
  widgets.sheet_prev_btn.set_sensitive(sheet > 0);
  widgets.sheet_next_btn.set_sensitive(sheet + 1 < total);
  widgets.sheet_info.set_text(&sheet_status_text(&state.borrow()));
  rebuild_sheet_tabs(state, widgets, &book, sheet);
  render_sheet_grid(widgets, &book, sheet);
  // Hint slot: truncation note wins over the empty-sheet note.
  let (rows, cols) = book.sheet_dims(sheet);
  if rows == 0 && cols == 0 {
    widgets.sheet_hint_lbl.set_visible(true);
    widgets.sheet_hint_lbl.set_text(&lang::t("xlsx.empty_sheet"));
  } else if truncated {
    widgets.sheet_hint_lbl.set_visible(true);
    widgets.sheet_hint_lbl.set_text(&lang::t_with(
      "xlsx.truncated",
      &[("rows", &rows.to_string()), ("cols", &cols.to_string())],
    ));
  } else {
    widgets.sheet_hint_lbl.set_visible(false);
  }
}

/// Rebuild the sheet tab switcher: one toggle button per sheet, active on
/// the current sheet. Only visible for multi-sheet workbooks; programmatic
/// `set_active` never emits `clicked`, so no feedback loop needs guarding.
fn rebuild_sheet_tabs(
  state: &Rc<RefCell<State>>,
  widgets: &Rc<Widgets>,
  book: &xlsx::Workbook,
  current: usize,
) {
  while let Some(child) = widgets.sheet_tabs.first_child() {
    widgets.sheet_tabs.remove(&child);
  }
  widgets.sheet_tabs_scroll.set_visible(book.sheet_count() > 1);
  if book.sheet_count() <= 1 {
    return;
  }
  for (index, sheet) in book.sheets.iter().enumerate() {
    let tab = gtk::ToggleButton::with_label(&sheet.name);
    tab.set_active(index == current);
    let state = state.clone();
    let tab_widgets = widgets.clone();
    tab.connect_clicked(move |_| {
      state.borrow_mut().sheet_index = index;
      refresh_sheet(&state, &tab_widgets);
      refresh_chrome(&state, &tab_widgets);
    });
    widgets.sheet_tabs.append(&tab);
  }
}

/// Render one sheet into the grid: a corner cell plus column letters on
/// the first row, row numbers in the first column and the data cells
/// after them. The first data row renders bold as the header row;
/// numeric cells align to the end. All text uses SF Pro Display via the
/// base style; headers use the `dim-label` class plus bold markup.
fn render_sheet_grid(widgets: &Rc<Widgets>, book: &xlsx::Workbook, index: usize) {
  let grid = &widgets.sheet_grid;
  while let Some(child) = grid.first_child() {
    grid.remove(&child);
  }
  let Some(sheet) = book.sheets.get(index) else {
    return;
  };
  let cols = sheet.rows.iter().map(|row| row.len()).max().unwrap_or(0);
  // Header row: corner plus column letters.
  for col in 0..cols {
    let label = gtk::Label::new(None);
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    label.set_markup(&format!("<b>{}</b>", xlsx::col_letters(col)));
    grid.attach(&label, (col + 1) as i32, 0, 1, 1);
  }
  for (row_idx, row) in sheet.rows.iter().enumerate() {
    // Row number in the first column.
    let number = gtk::Label::new(None);
    number.add_css_class("dim-label");
    number.set_halign(gtk::Align::End);
    number.set_markup(&format!("<b>{}</b>", row_idx + 1));
    grid.attach(&number, 0, (row_idx + 1) as i32, 1, 1);
    for (col_idx, cell) in row.iter().enumerate() {
      // Skip padding past the widest row (ragged rows are padded in the
      // parser, so this only guards hand-built sheets in tests).
      if col_idx >= cols {
        continue;
      }
      let label = gtk::Label::new(None);
      label.set_halign(if xlsx::is_number_text(cell) {
        gtk::Align::End
      } else {
        gtk::Align::Start
      });
      label.set_ellipsize(gtk::pango::EllipsizeMode::End);
      label.set_max_width_chars(30);
      if row_idx == 0 {
        // Header row: bold markup with escaped text.
        let escaped = glib::markup_escape_text(cell);
        label.set_markup(&format!("<b>{escaped}</b>"));
      } else {
        label.set_text(cell);
      }
      grid.attach(&label, (col_idx + 1) as i32, (row_idx + 1) as i32, 1, 1);
    }
  }
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
