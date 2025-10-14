#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    RustImageData, common::RustImage,
};
use eframe::{
    egui::{self, ImageSource, load::Bytes},
};
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender},
    },
    thread::{self},
};

enum Clip {
    Text(String),
    Img(Vec<u8>),
}

impl PartialEq<&Clip> for Clip {
    fn eq(&self, other: &&Clip) -> bool {
        match self {
            Clip::Text(t) => {
                if let Clip::Text(o) = other {
                    o == t
                } else {
                    false
                }
            }
            Clip::Img(_) => {
                return false;
            }
        }
    }
}

impl PartialEq<Clip> for Clip {
    fn eq(&self, other: &Clip) -> bool {
        match self {
            Clip::Text(t) => {
                if let Clip::Text(o) = other {
                    o == t
                } else {
                    false
                }
            }
            Clip::Img(_) => {
                return false;
            }
        }
    }
}

struct Manager {
    ctx: ClipboardContext,
    tx: Sender<Clip>,
}

impl Manager {
    pub fn new(tx: Sender<Clip>) -> Self {
        let ctx = ClipboardContext::new().unwrap();
        Manager { ctx, tx }
    }
}
impl ClipboardHandler for Manager {
    fn on_clipboard_change(&mut self) {
        println!("{:?}", self.ctx.available_formats().unwrap());

        if let Ok(t) = self.ctx.get_text()
            && !t.is_empty()
        {
            println!("on_clipboard_change, txt = {}", t);
            match self.tx.send(Clip::Text(t)) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("send fail {:?}", e)
                }
            }
        }

        if let Ok(img) = self.ctx.get_image()
            && !img.is_empty()
        {
            let mut w = std::io::Cursor::new(Vec::new());
            if let Ok(_) = img.get_dynamic_image().inspect(|f| {
                let _ = f.write_to(&mut w, image::ImageFormat::Jpeg);
            }) {
                let data = w.into_inner();
                match self.tx.send(Clip::Img(data)) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("send fail {:?}", e)
                    }
                }
            };
        }
    }
}

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    // 消息
    let (tx, rx) = std::sync::mpsc::channel();

    let manager = Manager::new(tx);

    let mut watcher = ClipboardWatcherContext::new().unwrap();

    let watcher_shutdown: clipboard_rs::WatcherShutdown =
        watcher.add_handler(manager).get_shutdown_channel();

    thread::spawn(move || {
        watcher.start_watch();
    });

    println!("egui");
    eframe::run_native(
        "Clip",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(ClipboardApp::default(rx, watcher_shutdown)))
        }),
    )
}
struct Data {
    clip: Vec<Clip>,
}
struct ClipboardApp {
    data: Arc<Mutex<Data>>,
    ctx: ClipboardContext,

    _shutdown: clipboard_rs::WatcherShutdown,
}

impl ClipboardApp {
    fn default(rx: Receiver<Clip>, shutdown: clipboard_rs::WatcherShutdown) -> Self {
        let c = Arc::new(Mutex::new(Data { clip: Vec::new() }));
        // v.start(rx);
        let t = Arc::clone(&c);
        let res = Self {
            data: Arc::clone(&c),
            ctx: ClipboardContext::new().unwrap(),
            _shutdown:shutdown,
        };
        thread::spawn(move || {
            loop {
                let r = rx.recv().unwrap();
                println!("收到消息");
                match t.lock() {
                    Ok(mut s) => {
                        if !s.clip.iter().any(|f| r == f) {
                            s.clip.push(r);
                            println!("修改");
                        }
                    }
                    Err(_) => {
                        println!("lock 失败");
                    }
                }
            }
            // self.name= "OK".to_string();
        });
        res
    }
}

impl eframe::App for ClipboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.data.lock() {
                Ok(data) => {
                    ui.heading("Clipboard");

                    for ele in data.clip.iter().rev() {
                        match ele {
                            Clip::Text(t) => {
                                ui.horizontal(|ui| {
                                    if ui.button("Copy").clicked() {
                                        println!("copy {}", t);
                                        let _ = self.ctx.set_text(t.clone());
                                    }
                                    ui.label(format!("{}", t));
                                });
                            }
                            Clip::Img(d) => {
                                ui.horizontal(|ui| {
                                    if ui.button("Copy").clicked() {
                                        println!("copy img",);
                                        let _ = self.ctx.set_image(
                                            RustImageData::from_bytes(d.as_slice()).unwrap(),
                                        );
                                    }
                                    ui.image(ImageSource::Bytes {
                                        uri: std::borrow::Cow::Borrowed("bytes://1.jpg"),
                                        bytes: Bytes::from(d.clone()),
                                    });
                                });
                            }
                        }
                    }
                }
                Err(_) => {
                    println!("update fial");
                }
            }
            // ui.image(egui::include_image!(
            //     "../../../crates/egui/assets/ferris.png"
            // ));
        });
    }
}
