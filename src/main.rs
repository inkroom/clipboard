#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    RustImageData, common::RustImage,
};
use eframe::egui::{self, IconData, ImageSource, Pos2, ScrollArea, load::Bytes};
#[cfg(feature="print")]
use log::{error, info};
use std::{
    ops::Deref,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender},
    },
    thread::{self},
};

macro_rules! s_error {
    // debug!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // debug!(target: "my_target", "a {} event", "log")
    // (target: $target:expr, $($arg:tt)+) => (log!(target: $target, $crate::Level::Debug, $($arg)+));

    // debug!("a {} event", "log")
    ($($arg:tt)+) => (
        #[cfg(feature="print")]
        log::error!($($arg)+);
    )
}

macro_rules! s_info {
    // debug!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // debug!(target: "my_target", "a {} event", "log")
    // (target: $target:expr, $($arg:tt)+) => (log!(target: $target, $crate::Level::Debug, $($arg)+));

    // debug!("a {} event", "log")
    ($($arg:tt)+) => (

        #[cfg(feature="print")]
        log::info!($($arg)+);
    )
}

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
        s_info!("{:?}", self.ctx.available_formats().unwrap());

        if let Ok(t) = self.ctx.get_text()
            && !t.is_empty()
        {
            s_info!("on_clipboard_change, txt = {}", t);
            match self.tx.send(Clip::Text(t)) {
                Ok(_) => {}
                Err(e) => {
                    s_error!("send fail {:?}", e);
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
                    s_error!("send fail {:?}", e);
                }
            };
        }
    }
}
fn load_icon() -> tray_icon::Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let b = icon_data();
        let image = image::load_from_memory(b.as_slice())
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    tray_icon::Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}

/// 非打包情况下直接包含字节
#[cfg(not(feature = "pkg"))]
fn icon_data() -> Vec<u8> {
    s_info!("include");
    let b = include_bytes!("../img/ico.png");
    b.to_vec()
}
/// bundle内执行方法
mod bundle {

    /// 检查是否在Bundle环境中运行
    pub(super) fn is_bundle_environment() -> bool {
        if let Ok(exe_path) = std::env::current_exe() {
            return exe_path.to_string_lossy().contains(".app");
        }
        false
    }
    /// 获取Bundle资源目录路径
    pub(super) fn get_bundle_resources_path() -> Option<std::path::PathBuf> {
        if let Ok(exe_path) = std::env::current_exe() {
            // 可执行文件位于: MyApp.app/Contents/MacOS/
            if let Some(contents_dir) = exe_path.parent() {
                s_info!("con = {}", contents_dir.display());
                if let Some(bundle_dir) = contents_dir.parent() {
                    s_info!("bun= {}", bundle_dir.display());
                    let resources_path = bundle_dir.join("Resources");
                    s_info!("res = {}", resources_path.display());
                    if resources_path.exists() {
                        return Some(resources_path);
                    }
                }
            }
        }
        None
    }
}
#[cfg(debug_assertions)]
mod custom_log {

    use std::{io::Write, time::Duration};
    /// 时间戳转换，从1970年开始
    pub(crate) fn time_display(value: u64) -> String {
        do_time_display(value, 1970, Duration::from_secs(8 * 60 * 60))
    }

    /// 时间戳转换，支持从不同年份开始计算
    pub(crate) fn do_time_display(value: u64, start_year: u64, timezone: Duration) -> String {
        // 先粗略定位到哪一年
        // 以 365 来计算，年通常只会相比正确值更晚，剩下的秒数也就更多，并且有可能出现需要往前一年的情况
        let value = value + timezone.as_secs();

        let per_year_sec = 365 * 24 * 60 * 60; // 平年的秒数

        let mut year = value / per_year_sec;
        // 剩下的秒数，如果这些秒数 不够填补闰年，比如粗略计算是 2024年，还有 86300秒，不足一天，那么中间有很多闰年，所以 年应该-1，只有-1，因为-2甚至更多 需要 last_sec > 365 * 86400，然而这是不可能的
        let last_sec = value - (year) * per_year_sec;
        year += start_year;

        let mut leap_year_sec = 0;
        // 计算中间有多少闰年，当前年是否是闰年不影响回退，只会影响后续具体月份计算
        for y in start_year..year {
            if is_leap(y) {
                // 出现了闰年
                leap_year_sec += 86400;
            }
        }
        if last_sec < leap_year_sec {
            // 不够填补闰年，年份应该-1
            year -= 1;
            // 上一年是闰年，所以需要补一天
            if is_leap(year) {
                leap_year_sec -= 86400;
            }
        }
        // 剩下的秒数
        let mut time = value - leap_year_sec - (year - start_year) * per_year_sec;

        // 平年的月份天数累加
        let mut day_of_year: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

        // 找到了 计算日期
        let sec = time % 60;
        time /= 60;
        let min = time % 60;
        time /= 60;
        let hour = time % 24;
        time /= 24;

        // 计算是哪天，因为每个月不一样多，所以需要修改
        if is_leap(year) {
            day_of_year[1] += 1;
        }
        let mut month = 0;
        for (index, ele) in day_of_year.iter().enumerate() {
            if &time < ele {
                month = index + 1;
                time += 1; // 日期必须加一，否则 每年的 第 1 秒就成了第0天了
                break;
            }
            time -= ele;
        }

        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, time, hour, min, sec
        )
    }
    //
    // 判断是否是闰年
    //
    fn is_leap(year: u64) -> bool {
        year % 4 == 0 && ((year % 100) != 0 || year % 400 == 0)
    }
    ///
    /// 输出当前时间格式化
    ///
    /// 例如：
    /// 2023-09-28T09:32:24Z
    ///
    pub(crate) fn time_format() -> String {
        // 获取当前时间戳
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|v| v.as_secs())
            .unwrap_or(0);

        time_display(time)
    }
    struct Writer {
        console: std::io::Stdout,
        fs: Option<std::fs::File>,
    }
    impl Writer {
        pub fn new() -> Self {
            Writer {
                console: std::io::stdout(),
                fs: None,
            }
        }
    }
    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Some(fs) = &mut self.fs {
                self.console.write(buf)?;
                fs.write(buf)
            } else {
                self.console.write(buf)
            }
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if let Some(fs) = &mut self.fs {
                self.console.flush()?;
                fs.flush()
            } else {
                self.console.flush()
            }
        }
    }
    pub(crate) fn init() -> Result<(), String> {
        // if opt.verbose {
        //     std::env::set_var("RUST_LOG", "debug");
        // } else {
        unsafe {
            std::env::set_var("RUST_LOG", "info");
        }

        // }

        let mut s = env_logger::builder();
        s.default_format()
            .parse_default_env()
            .format(|buf, record| writeln!(buf, "{}: {}", time_format(), record.args()))
            .target(env_logger::Target::Pipe(Box::new(Writer::new())));

        s.init();
        Ok(())
    }
}
/// 打包情况下手动读取文件
#[cfg(feature = "pkg")]
fn icon_data() -> Vec<u8> {
    if let Some(res) = bundle::get_bundle_resources_path() {
        let icon = res.join("img/ico.png");
        if icon.exists()
            && let Ok(v) = std::fs::read(icon)
        {
            s_info!("bytes = {:?}", &v[0..10]);
            return v;
        }
    }
    Vec::new()
}

fn main() -> eframe::Result {
    use tray_icon::{
        TrayIconBuilder,
        menu::{Menu, Submenu},
    };
    #[cfg(debug_assertions)]
    let _ = custom_log::init();

    let icon = load_icon();
    #[cfg(not(target_os = "linux"))]
    let mut _tray_icon = std::rc::Rc::new(std::cell::RefCell::new(None));
    #[cfg(not(target_os = "linux"))]
    let tray_c = _tray_icon.clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 500.0])
            .with_icon(eframe::icon_data::from_png_bytes(&icon_data()).unwrap())
            .with_taskbar(false),
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
    /// 是否是hotkey触发显示的窗口
    is_hotkey_visible: bool,
    is_top: bool,
    /// 用于操作窗口
    ctx: egui::Context,
}

impl Data {
    fn switch_visible(&mut self, hotkey: bool) {
        self.window_visble = !self.window_visble;
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(self.window_visble));
        if self.window_visble {
            // 获取焦点
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.is_hotkey_visible = hotkey;
        } else {
            self.is_hotkey_visible = false;
        }
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
            is_hotkey_visible: false,
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
        res.hotkey_listen(Arc::clone(&c));
        res
    }

    fn clip_msg_listen(&self, rx: Receiver<Clip>, data: Arc<Mutex<Data>>) {
        thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(Clip::Quit) => {
                        // 退出
                        s_info!("quit msg listen");
                        break;
                    }
                    Ok(r) => {
                        s_info!("收到消息");
                        match data.lock() {
                            Ok(mut s) => {
                                if !s.clip.iter().any(|f| r == f) {
                                    s.clip.push(r);
                                    s_info!("修改");
                                    s.ctx.request_repaint();
                                }
                            }
                            Err(_) => {
                                s_error!("lock 失败");
                            }
                        }
                    }
                    Err(e) => {
                        // 退出时一定会有一条
                        s_error!("recv : {:?}", e);
                        break;
                    }
                }
            }
        });
    }

    fn hotkey_listen(&self, data: Arc<Mutex<Data>>) {
        use device_query::{DeviceEvents, DeviceEventsHandler, DeviceQuery, DeviceState, Keycode};
        match DeviceState::checked_new() {
            Some(device_state) => {
                let event_handler = DeviceEventsHandler::new(std::time::Duration::from_millis(10))
                    .expect("无法初始化事件处理器");
                let key_up = event_handler.on_key_down(move |key: &Keycode| {
                    // s_info!("按键释放: {:?}", key);
                    let keys = device_state.get_keys();
                    if (keys.contains(&Keycode::LControl) || keys.contains(&Keycode::RControl))
                        && (keys.contains(&Keycode::LShift) || keys.contains(&Keycode::RShift))
                        && key == &Keycode::A
                    {
                        if let Ok(mut s) = data.lock() {
                            // 修改窗口位置
                            let mouse = device_state.get_mouse();

                            let rect = s.ctx.screen_rect();
                            let x = (mouse.coords.0 as f32) - rect.max.x / 2.0;
                            let y = (mouse.coords.1 as f32) - rect.max.y / 2.0;
                            s.ctx
                                .send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                                    Pos2::new(x, y),
                                ));

                            s.switch_visible(true);
                        }
                    }
                });
                // key_up 被回收事件就会被remove，所以这里直接泄漏，保证不会被drop
                // 否则就要保证生命周期，但是放到结构体里类型很难写
                Box::leak(Box::new(key_up));
            }
            None => {
                s_error!("需要打开权限");
            }
        }
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
                                            s.switch_visible(false);
                                        } else {
                                            s.switch_visible(false);
                                        }
                                    }
                                    Err(_) => {
                                        s_error!("lock 失败2");
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
                    s_info!(
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
                                    let mut copyed = false;
                                    for (index, ele) in data.clip.iter().enumerate().rev() {
                                        match ele {
                                            Clip::Text(t) => {
                                                ui.horizontal(|ui| {
                                                    if ui.button("Copy").clicked() {
                                                        s_info!("copy {}", t);
                                                        let _ = self.ctx.set_text(t.clone());
                                                        copyed = true;
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
                                                        s_info!("copy img",);
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
                                    if copyed && data.is_hotkey_visible {
                                        // 隐藏窗口
                                        data.switch_visible(false);
                                    }
                                },
                            );
                        });
                }
                Err(_) => {
                    s_info!("update fial");
                }
            }
            if sw {
                self.switch_top(ctx);
            }
        });
    }
}
