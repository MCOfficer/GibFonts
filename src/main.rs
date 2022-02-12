#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]

extern crate native_windows_derive as nwd;
extern crate native_windows_gui as nwg;

use std::cell::RefCell;
use std::sync::mpsc::Receiver;

use anyhow::Result;
use human_format::{Formatter, Scales};
use nwd::NwgUi;
use nwg::stretch::{
    geometry::Size,
    style::{Dimension as D, FlexDirection},
};
use nwg::{CheckBox, CheckBoxState, Icon, NativeUi, ProgressBarState};
use simplog::simplog::SimpleLogger;

use crate::github::ReleaseAsset;

mod github;

static WINDOW_WIDTH: i32 = 800;
static WINDOW_HEIGHT: i32 = 900;

#[derive(Debug)]
pub enum ProgressEvent {
    Downloading { name: String, done: u32, total: u32 },
    Installing(String),
    Error(String),
    Finished,
}

#[derive(Default, NwgUi)]
pub struct App {
    #[nwg_control(size: (WINDOW_WIDTH, WINDOW_HEIGHT), position: (300, 50), title: "GibFonts", flags: "WINDOW|VISIBLE")]
    #[nwg_events( OnInit: [App::setup], OnWindowClose: [App::exit])]
    window: nwg::Window,

    #[nwg_layout(parent: window, flex_direction: FlexDirection::Column)]
    flex: nwg::FlexboxLayout,

    #[nwg_control(parent: window, flags: "VISIBLE", text: "Loading available fonts...")]
    #[nwg_layout_item(
        layout: flex,
        size: Size { width: D::Auto, height: D::Points(30.0) }
    )]
    label: nwg::Label,

    #[nwg_control(flags: "VISIBLE")]
    #[nwg_layout_item(
        layout: flex,
        size: Size{ width: D::Auto, height: D::Points(50.0) }
    )]
    control: nwg::Frame,

    #[nwg_layout(parent: control, flex_direction: FlexDirection::Row)]
    control_flex: nwg::FlexboxLayout,

    #[nwg_control(parent: control, text: "All", flags: "VISIBLE|DISABLED")]
    #[nwg_layout_item(layout: control_flex)]
    #[nwg_events(OnButtonClick: [App::check_all])]
    check_all: nwg::Button,

    #[nwg_control(parent: control, text: "None", flags: "VISIBLE|DISABLED")]
    #[nwg_layout_item(layout: control_flex)]
    #[nwg_events(OnButtonClick: [App::uncheck_all])]
    uncheck_all: nwg::Button,

    #[nwg_control()]
    #[nwg_layout_item(
        layout: flex,
        flex_grow: 2.0,
        size: Size { width: D::Auto, height: D::Auto }
    )]
    frame: nwg::Frame,

    #[nwg_layout(parent: frame)]
    frame_flex: nwg::GridLayout,

    assets: RefCell<Vec<(CheckBox, ReleaseAsset)>>,

    #[nwg_control(parent: window, v_align: nwg::VTextAlign::Center, flags: "VISIBLE", text: "")]
    #[nwg_layout_item(
        layout: flex,
        size: Size { width: D::Auto, height: D::Points(20.0) },

    )]
    progress_label: nwg::Label,

    #[nwg_control(flags: "VISIBLE")]
    #[nwg_layout_item(
    layout: flex,
    size: Size { width: D::Auto, height: D::Points(15.0) }
    )]
    download_progress: nwg::ProgressBar,

    #[nwg_control(flags: "VISIBLE")]
    #[nwg_layout_item(
        layout: flex,
        size: Size { width: D::Auto, height: D::Points(20.0) }
    )]
    main_progress: nwg::ProgressBar,

    #[nwg_control]
    #[nwg_events(OnNotice: [App::update_progress])]
    update_progress: nwg::Notice,

    progress_receiver: RefCell<Option<Receiver<ProgressEvent>>>,

    #[nwg_control(text: "Install", flags: "VISIBLE|DISABLED")]
    #[nwg_layout_item(
        layout: flex,
        size: Size { width: D::Auto, height: D::Points(30.0) }
    )]
    #[nwg_events(OnButtonClick: [App::install])]
    install: nwg::Button,

    #[nwg_control]
    #[nwg_events(OnNotice: [App::update_assets])]
    update_assets: nwg::Notice,

    asset_receiver: RefCell<Option<Receiver<Result<Vec<ReleaseAsset>>>>>,
}

impl App {
    fn setup(&self) {
        let mut icon = Default::default();
        Icon::builder()
            .source_bin(Some(include_bytes!("../gibfonts.ico")))
            .build(&mut icon)
            .unwrap();
        self.window.set_icon(Some(&icon));
        self.disable_controls();

        let (sender, receiver) = std::sync::mpsc::channel();
        let notice_sender = self.update_assets.sender();

        std::thread::spawn(move || {
            sender.send(github::available_fonts()).unwrap();
            notice_sender.notice();
        });
        *self.asset_receiver.borrow_mut() = Some(receiver);
    }

    fn update_assets(&self) {
        let mut receiver_ref = self.asset_receiver.borrow_mut();
        let receiver = receiver_ref.as_mut().unwrap();
        while let Ok(result) = receiver.try_recv() {
            match result {
                Ok(mut assets) => {
                    assets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                    let assets = assets
                        .drain(..)
                        .enumerate()
                        .map(|(i, asset)| {
                            let mut checkbox = Default::default();
                            CheckBox::builder()
                                .text(&asset.display_name())
                                .parent(&self.frame)
                                .build(&mut checkbox)
                                .unwrap();

                            let modu = 20;
                            self.frame_flex.add_child(
                                (i / modu) as u32,
                                (i % modu) as u32,
                                &checkbox,
                            );
                            (checkbox, asset)
                        })
                        .collect();

                    self.label.set_text("Select fonts to install:");
                    self.enable_controls();
                    self.assets.replace(assets);
                    self.check_all();
                }
                Err(e) => nwg::modal_fatal_message(
                    self.window.handle,
                    "Error",
                    &format!("Failed to fetch release assets:\n{:#?}", e),
                ),
            }
        }
    }

    fn install(&self) {
        self.disable_controls();
        let (sender, receiver) = std::sync::mpsc::channel();
        let notice_sender = self.update_progress.sender();

        let assets = self.assets.borrow();
        let selected: Vec<ReleaseAsset> = assets
            .iter()
            .filter(|(c, _)| CheckBoxState::Checked == c.check_state())
            .map(|(_, a)| a.clone())
            .collect();

        self.main_progress.set_pos(0);
        self.main_progress.set_state(ProgressBarState::Normal);
        self.main_progress.set_range(0..selected.len() as u32);

        std::thread::spawn(move || {
            for a in selected {
                if let Err(e) = a.install(&sender, &notice_sender) {
                    sender
                        .send(ProgressEvent::Error(format!("{:#}", e)))
                        .unwrap();
                    notice_sender.notice();
                    return;
                };
            }
            sender.send(ProgressEvent::Finished).unwrap();
            notice_sender.notice();
        });
        *self.progress_receiver.borrow_mut() = Some(receiver);
        self.enable_controls();
    }

    fn update_progress(&self) {
        let mut receiver_ref = self.progress_receiver.borrow_mut();
        let receiver = receiver_ref.as_mut().unwrap();
        match receiver.try_recv().unwrap() {
            ProgressEvent::Downloading { name, done, total } => {
                let mut new_formatter = Formatter::new();
                let formatter = new_formatter
                    .with_scales(Scales::Binary())
                    .with_decimals(0)
                    .with_units("B");
                self.progress_label.set_text(&format!(
                    "Downloading {} ({}/{})",
                    name,
                    formatter.format(done as f64),
                    formatter.format(total as f64),
                ));
                self.download_progress.set_range(0..total);
                self.download_progress.set_pos(done);
            }
            ProgressEvent::Installing(name) => {
                self.download_progress
                    .set_pos(self.download_progress.range().end);
                self.main_progress.advance();
                self.progress_label
                    .set_text(&format!("Installing {}", name));
            }
            ProgressEvent::Error(e) => {
                nwg::modal_error_message(
                    self.window.handle,
                    "Error",
                    &format!("Failed to install:\n{:#?}", e),
                );
                self.progress_label
                    .set_text(&format!("{} ...Error!", self.progress_label.text()));
                self.main_progress.set_state(ProgressBarState::Error);
            }
            ProgressEvent::Finished => {
                self.enable_controls();
                self.download_progress
                    .set_pos(self.download_progress.range().end);
                self.main_progress
                    .set_pos(self.download_progress.range().end);
                self.progress_label
                    .set_text("Done! Reboot to finish installation");
            }
        }
    }

    fn disable_controls(&self) {
        self.check_all.set_enabled(false);
        self.uncheck_all.set_enabled(false);
        self.install.set_enabled(false);
        for (c, _) in self.assets.borrow().iter() {
            c.set_enabled(false);
        }
    }

    fn enable_controls(&self) {
        self.check_all.set_enabled(true);
        self.uncheck_all.set_enabled(true);
        self.install.set_enabled(true);
        for (c, _) in self.assets.borrow().iter() {
            c.set_enabled(true);
        }
    }
    fn check_all(&self) {
        for (c, _) in self.assets.borrow().iter() {
            c.set_check_state(CheckBoxState::Checked);
        }
    }

    fn uncheck_all(&self) {
        for (c, _) in self.assets.borrow().iter() {
            c.set_check_state(CheckBoxState::Unchecked);
        }
    }

    fn exit(&self) {
        nwg::stop_thread_dispatch();
    }
}

fn main() {
    SimpleLogger::init(Some("Debug"));
    nwg::init().expect("Failed to init Native Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    let _app = App::build_ui(Default::default()).expect("Failed to build UI");

    nwg::dispatch_thread_events();
}
