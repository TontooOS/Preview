//! Tahoe-style Preview root view.
//!
//! Header with `Preview` title plus file name, `Open` / `Edit`-`Done` /
//! `Save` actions. Empty state with a centered open button. Text files
//! render read-only with a rendered Markdown preview mode; edit mode
//! shows raw text with line numbers on the left. PDFs open read-only in
//! a page viewer (previous/next, page indicator, zoom, fit width). Images
//! open read-only as actual pictures (`gtk::Picture` with zoom in/out
//! plus fit window). Audio files open read-only in a compact player
//! (`gtk::MediaFile` with play/pause, seek slider, volume plus file
//! info). Video files open read-only on a player page with a picture
//! (`gtk::Video` driven by `gtk::MediaFile` with play/pause, seek slider,
//! volume plus file info). Saving is manual only (`Save` button, `Ctrl+S`,
//! close dialog with Cancel / Save / Don't Save).

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

  // Image page: toolbar (zoom out/in, fit window) plus an actual picture
  // (`gtk::Picture`). Broken images show an error label with the reason.
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
  img_box.append(&img_bar);
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
     .audio-title {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}\
     .audio-time {{ font-family: '{SF_PRO}'; font-size: 11pt; }}\
     .video-title {{ font-family: '{SF_PRO}'; font-size: 16pt; font-weight: 700; }}\
     .video-time {{ font-family: '{SF_PRO}'; font-size: 11pt; }}"
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
/// images, audio or video files).
fn do_save(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  if state.borrow().kind == FileKind::Pdf
    || state.borrow().kind == FileKind::Image
    || state.borrow().kind == FileKind::Audio
    || state.borrow().kind == FileKind::Video
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
  let editable = has_file && (st.kind == FileKind::Text || st.kind == FileKind::Markdown);
  let is_pdf = has_file && st.kind == FileKind::Pdf;
  let is_image = has_file && st.kind == FileKind::Image;
  let is_audio = has_file && st.kind == FileKind::Audio;
  let is_video = has_file && st.kind == FileKind::Video;

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
  } else if is_pdf {
    match st.pdf.as_ref() {
      Some(doc) => widgets.status.set_text(&lang::t_with(
        "status.pdf",
        &[("count", &doc.page_count().to_string())],
      )),
      None => widgets.status.set_text(&lang::t("status.unsupported")),
    }
  } else if is_image {
    match st.img_meta.as_ref() {
      Some(meta) if meta.has_dimensions() => {
        let mut text = lang::t_with(
          "status.image",
          &[
            ("width", &meta.width.to_string()),
            ("height", &meta.height.to_string()),
            ("size", &model::format_file_size(meta.file_bytes)),
          ],
        );
        if meta.downscaled {
          text.push_str(&format!(" ({})", lang::t("image.downscaled")));
        }
        widgets.status.set_text(&text);
      }
      Some(meta) => widgets.status.set_text(&lang::t_with(
        "status.image_unknown",
        &[("size", &model::format_file_size(meta.file_bytes))],
      )),
      None => {
        let reason = st.img_error.clone().unwrap_or_else(|| lang::t("unsupported.hint"));
        widgets.status.set_text(&lang::t_with(
          "image.load_failed",
          &[("reason", &reason)],
        ));
      }
    }
  } else if is_audio {
    widgets.status.set_text(&audio_status_text(&st));
  } else if is_video {
    widgets.status.set_text(&video_status_text(&st));
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
  } else if st.kind == FileKind::Pdf {
    "pdf"
  } else if st.kind == FileKind::Image {
    "image"
  } else if st.kind == FileKind::Audio {
    "audio"
  } else if st.kind == FileKind::Video {
    "video"
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
  widgets.img_bar.set_visible(true);
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

/// Apply the image zoom factor via an explicit picture size so the aspect
/// ratio is preserved and the scrolled window scrolls past the viewport.
fn apply_image_zoom(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let zoom = state.borrow().img_zoom;
  match image_base_size(state) {
    Some((base_width, base_height)) => {
      let (width, height) = model::zoomed_size(base_width, base_height, zoom);
      widgets.img_picture.set_size_request(width as i32, height as i32);
    }
    None => widgets.img_picture.set_size_request(-1, -1),
  }
  widgets.img_picture.set_content_fit(gtk::ContentFit::Contain);
}

/// Fit the picture into the current viewport (cheap: computed once from
/// the current allocation; press Fit again after resizes). Falls back to
/// the current zoom when the viewport is not allocated yet.
fn apply_image_fit(state: &Rc<RefCell<State>>, widgets: &Rc<Widgets>) {
  let base = image_base_size(state);
  let (view_width, view_height) = (
    widgets.img_scroll.width(),
    widgets.img_scroll.height(),
  );
  if let Some((base_width, base_height)) = base {
    if view_width > 0 && view_height > 0 {
      state.borrow_mut().img_zoom =
        model::fit_zoom_for(base_width, base_height, view_width, view_height);
    }
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
  widgets.status.set_text(&status);
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
  widgets.status.set_text(&status);
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
