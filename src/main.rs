#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    RustImageData, common::RustImage,
};
use eframe::egui::{self, ImageSource, load::Bytes};
use std::{
    fmt::format,
    ops::Deref,
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 500.0]),
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

            Ok(Box::new(ClipboardApp::default(
                rx,
                watcher_shutdown,
                &cc.egui_ctx,
            )))
        }),
    )
}
struct Data {
    clip: Vec<Clip>,
    /// 用于触发重绘
    repaint: egui::Context,
}
struct ClipboardApp {
    data: Arc<Mutex<Data>>,
    ctx: ClipboardContext,
    is_top: bool,
    _shutdown: clipboard_rs::WatcherShutdown,
}

impl ClipboardApp {
    fn default(
        rx: Receiver<Clip>,
        shutdown: clipboard_rs::WatcherShutdown,
        cc: &egui::Context,
    ) -> Self {
        let c = Arc::new(Mutex::new(Data {
            clip: Vec::new(),
            repaint: cc.clone(),
        }));
        // v.start(rx);
        let t = Arc::clone(&c);
        let res = Self {
            data: Arc::clone(&c),
            ctx: ClipboardContext::new().unwrap(),
            _shutdown: shutdown,
            is_top: false,
        };
        thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(r) => {
                        println!("收到消息");
                        match t.lock() {
                            Ok(mut s) => {
                                if !s.clip.iter().any(|f| r == f) {
                                    s.clip.push(r);
                                    println!("修改");
                                    s.repaint.request_repaint();
                                }
                            }
                            Err(_) => {
                                eprintln!("lock 失败");
                            }
                        }
                    }
                    Err(e) => {
                        // 退出时一定会有一条
                        eprintln!("recv : {:?}", e);
                        break;
                    }
                }
            }
        });
        res.add_font(cc);
        res
    }

    fn add_font(&self, cc: &egui::Context) {
        let fs = font_kit::source::SystemSource::new();

        // 遍历所有系统字体
        for handle in fs.all_families().unwrap() {
            if let Ok(f) = fs.select_family_by_name(&handle)
                && let Ok(font) = f.fonts()[0].load()
            {
                // 检查是否包含中文字符（CJK Unified Ideographs范围）
                if let Some(_) = font.glyph_for_char('中')
                    && let Some(data) = font.copy_font_data()
                {
                    let name = font.full_name();
                    println!(
                        "支持中文的字体: {} : {}",
                        match &f.fonts()[0] {
                            font_kit::handle::Handle::Path {
                                path,
                                font_index: _,
                            } => format!("{}", path.display()),
                            font_kit::handle::Handle::Memory {
                                bytes: _,
                                font_index: _,
                            } => String::new(),
                        },
                        name
                    );
                    cc.add_font(egui::epaint::text::FontInsert::new(
                        name.as_str(),
                        egui::FontData::from_owned(data.deref().clone()),
                        // egui::FontData::from_static(include_bytes!("../wqy-zenhei.ttc")),
                        vec![
                            egui::epaint::text::InsertFontFamily {
                                family: egui::FontFamily::Proportional,
                                priority: egui::epaint::text::FontPriority::Highest,
                            },
                            egui::epaint::text::InsertFontFamily {
                                family: egui::FontFamily::Monospace,
                                priority: egui::epaint::text::FontPriority::Lowest,
                            },
                        ],
                    ));
                    break;
                }
            }
        }

        if let Ok(h) = fs.select_best_match(
            &[
                font_kit::family_name::FamilyName::SansSerif,
                font_kit::family_name::FamilyName::Serif,
            ],
            &font_kit::properties::Properties::new(),
        ) && let Ok(f) = h.load()
            && let Some(data) = f.copy_font_data()
        {
            // println!("找到字体,{}",f.);
            cc.add_font(egui::epaint::text::FontInsert::new(
                "my_font",
                egui::FontData::from_owned(data.deref().clone()),
                // egui::FontData::from_static(include_bytes!("../wqy-zenhei.ttc")),
                vec![
                    egui::epaint::text::InsertFontFamily {
                        family: egui::FontFamily::Proportional,
                        priority: egui::epaint::text::FontPriority::Highest,
                    },
                    egui::epaint::text::InsertFontFamily {
                        family: egui::FontFamily::Monospace,
                        priority: egui::epaint::text::FontPriority::Lowest,
                    },
                ],
            ));
        }
    }
}

impl eframe::App for ClipboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.data.lock() {
                Ok(mut data) => {
                    ui.horizontal(|ui| {
                        ui.heading("Clipboard");
                        if ui.button("top").clicked() {
                            let mut flag = self.is_top;
                            flag = !flag;
                            self.is_top = flag;
                            if flag {
                                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                    egui::WindowLevel::AlwaysOnTop,
                                ));
                            } else {
                                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                    egui::WindowLevel::Normal,
                                ));
                            }
                        }
                    });
                    let mut removed_index = None;
                    for (index, ele) in data.clip.iter().enumerate().rev() {
                        match ele {
                            Clip::Text(t) => {
                                ui.horizontal(|ui| {
                                    if ui.button("Copy").clicked() {
                                        println!("copy {}", t);
                                        let _ = self.ctx.set_text(t.clone());
                                    }
                                    if ui.link("rm").clicked() {
                                        removed_index = Some(index);
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
                                    if ui.link("rm").clicked() {
                                        removed_index = Some(index);
                                    }
                                    ui.image(ImageSource::Bytes {
                                        uri: std::borrow::Cow::Borrowed("bytes://1.jpg"),
                                        bytes: Bytes::from(d.clone()),
                                    });
                                });
                            }
                        }
                    }
                    if let Some(index) = removed_index {
                        data.clip.remove(index);
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
