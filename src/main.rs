#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    RustImageData, common::RustImage,
};
use eframe::egui::{self, ImageSource, ScrollArea, load::Bytes};
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
    Quit,
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
            Clip::Quit => {
                if let Clip::Quit = other {
                    true
                } else {
                    false
                }
            }
        }
    }
}

impl PartialEq<Clip> for Clip {
    fn eq(&self, other: &Clip) -> bool {
        self == &other
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

    fn start(self) -> clipboard_rs::WatcherShutdown {
        let mut watcher = ClipboardWatcherContext::new().unwrap();

        let watcher_shutdown: clipboard_rs::WatcherShutdown =
            watcher.add_handler(self).get_shutdown_channel();

        thread::spawn(move || {
            watcher.start_watch();
        });
        watcher_shutdown
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
            && let Ok(data) = img.to_jpeg()
        {
            match self.tx.send(Clip::Img(data.get_bytes().to_vec())) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("send fail {:?}", e)
                }
            };
        }
    }
}
fn load_icon() -> tray_icon::Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let b = include_bytes!("favicon.ico");
        let image = image::load_from_memory(b.as_slice())
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    tray_icon::Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}
fn main() -> eframe::Result {
    use tray_icon::{
        TrayIconBuilder,
        menu::{Menu, Submenu},
    };

    let icon = load_icon();
    #[cfg(not(target_os = "linux"))]
    let mut _tray_icon = std::rc::Rc::new(std::cell::RefCell::new(None));
    #[cfg(not(target_os = "linux"))]
    let tray_c = _tray_icon.clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 500.0]),
        ..Default::default()
    };

    // 消息
    let (tx, rx) = std::sync::mpsc::channel();

    let manager = Manager::new(tx.clone());
    let watcher_shutdown = manager.start();
    eframe::run_native(
        "Clip",
        options,
        Box::new(|cc| {
            #[cfg(not(target_os = "linux"))]
            {
                tray_c
                    .borrow_mut()
                    .replace(TrayIconBuilder::new().with_icon(icon).build().unwrap());
            }
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(ClipboardApp::default(
                rx,
                watcher_shutdown,
                &cc.egui_ctx,
                tx,
            )))
        }),
    )
}
struct Data {
    clip: Vec<Clip>,
    window_visble: bool,
    is_top: bool,
    /// 用于操作窗口
    ctx: egui::Context,
}

impl Data {
    fn switch_visible(&mut self) {
        self.window_visble = !self.window_visble;
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(self.window_visble));
    }
    fn switch_top(&mut self) {
        let mut flag = self.is_top;
        flag = !flag;
        self.is_top = flag;
        if flag {
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::AlwaysOnTop,
                ));
        } else {
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::Normal,
                ));
        }
    }
}

struct ClipboardApp {
    data: Arc<Mutex<Data>>,
    ctx: ClipboardContext,
    _shutdown: clipboard_rs::WatcherShutdown,
    sender: Sender<Clip>,
}

impl ClipboardApp {
    fn default(
        rx: Receiver<Clip>,
        shutdown: clipboard_rs::WatcherShutdown,
        cc: &egui::Context,
        sender: Sender<Clip>,
    ) -> Self {
        let c = Arc::new(Mutex::new(Data {
            window_visble: true,
            clip: Vec::new(),
            ctx: cc.clone(),
            is_top: false,
        }));
        // v.start(rx);
        let res = Self {
            data: Arc::clone(&c),
            ctx: ClipboardContext::new().unwrap(),
            _shutdown: shutdown,
            sender,
        };

        res.add_font(cc);
        res.clip_msg_listen(rx, Arc::clone(&c));
        res.tray_listen(Arc::clone(&c));
        res
    }

    fn clip_msg_listen(&self, rx: Receiver<Clip>, data: Arc<Mutex<Data>>) {
        thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(Clip::Quit) => {
                        // 退出
                        println!("quit msg listen");
                        break;
                    }
                    Ok(r) => {
                        println!("收到消息");
                        match data.lock() {
                            Ok(mut s) => {
                                if !s.clip.iter().any(|f| r == f) {
                                    s.clip.push(r);
                                    println!("修改");
                                    s.ctx.request_repaint();
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
    }

    fn tray_listen(&self, data: Arc<Mutex<Data>>) {
        #[cfg(not(target_os = "linux"))]
        thread::spawn(move || {
            use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

            let rev = TrayIconEvent::receiver();
            loop {
                if let Ok(event) = rev.recv() {
                    match event {
                        TrayIconEvent::Click {
                            id: _,
                            position: _,
                            rect: _,
                            button,
                            button_state,
                        } => {
                            if MouseButtonState::Up == button_state {
                                match data.lock() {
                                    Ok(mut s) => {
                                        if MouseButton::Right == button {
                                            // 切换置顶
                                            s.switch_top();
                                            s.window_visble = false;
                                            s.switch_visible();
                                        } else {
                                            s.switch_visible();
                                        }
                                    }
                                    Err(_) => {
                                        eprintln!("lock 失败2");
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
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
    }

    fn switch_top(&mut self, _ctx: &egui::Context) {
        match self.data.lock() {
            Ok(mut v) => {
                v.switch_top();
            }
            Err(_) => {}
        }
    }
}

impl eframe::App for ClipboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // 响应退出
            if ctx.input(|i| i.viewport().close_requested()) {
                let _ = self.sender.send(Clip::Quit);
            }
            let mut sw = false;
            match self.data.lock() {
                Ok(mut data) => {
                    if !data.window_visble {
                        return;
                    }
                    ui.horizontal(|ui| {
                        ui.heading("Clipboard");
                        if ui.button("top").clicked() {
                            sw = true;
                        }
                    });

                    // 滚动
                    ScrollArea::vertical()
                        .auto_shrink(false)
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            ui.with_layout(
                                egui::Layout::top_down(egui::Align::LEFT).with_cross_justify(true),
                                |ui| {
                                    let mut removed_index = None;
                                    for (index, ele) in data.clip.iter().enumerate().rev() {
                                        match ele {
                                            Clip::Text(t) => {
                                                ui.horizontal(|ui| {
                                                    if ui.button("Copy").clicked() {
                                                        println!("copy {}", t);
                                                        let _ = self.ctx.set_text(t.clone());
                                                    }
                                                    if ui.link("del").clicked() {
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
                                                            RustImageData::from_bytes(d.as_slice())
                                                                .unwrap(),
                                                        );
                                                    }
                                                    if ui.link("rm").clicked() {
                                                        removed_index = Some(index);
                                                    }
                                                    ui.image(ImageSource::Bytes {
                                                        uri: std::borrow::Cow::Borrowed(
                                                            "bytes://1.jpg",
                                                        ),
                                                        bytes: Bytes::from(d.clone()),
                                                    });
                                                });
                                            }
                                            _ => {}
                                        }
                                    }
                                    if let Some(index) = removed_index {
                                        data.clip.remove(index);
                                    }
                                },
                            );
                        });
                }
                Err(_) => {
                    println!("update fial");
                }
            }
            if sw {
                self.switch_top(ctx);
            }
        });
    }
}
