//! Windows: снимок экрана, распознавание текста (Windows.Media.Ocr), глобальные хуки клавиатуры
//! и мыши, окна поверх игры. В память игры не лезем — только то, что видно на экране.

use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context as _, Result};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use image::RgbaImage;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW,
    MonitorFromPoint, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, MapVirtualKeyW, MAPVK_VK_TO_CHAR};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowTextW,
    PeekMessageW, PostThreadMessageW, SetWindowDisplayAffinity, SetWindowLongPtrW,
    SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx, GWL_EXSTYLE, HC_ACTION, HHOOK,
    HWND_TOPMOST, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, WDA_EXCLUDEFROMCAPTURE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP, WS_EX_NOACTIVATE, WS_EX_TOPMOST,
};

use super::HotkeyEvent;

// ---------- снимок экрана ----------

/// Снимок монитора под курсором.
pub struct Shot {
    pub img: RgbaImage,
    /// Левый верхний угол монитора в координатах экрана (физические пиксели).
    pub left: i32,
    pub top: i32,
    /// Курсор в координатах снимка.
    pub cursor: (u32, u32),
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// Монитор в точке: весь (`work = false`) или без панели задач.
pub fn monitor_rect(x: i32, y: i32, work: bool) -> RECT {
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        if work { mi.rcWork } else { mi.rcMonitor }
    }
}

/// Снимок монитора, на котором курсор (GDI BitBlt — работает с игрой в режиме «оконный без рамки»).
pub fn capture_at_cursor() -> Result<Shot> {
    let (cx, cy) = cursor_pos();
    let r = monitor_rect(cx, cy, false);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w <= 0 || h <= 0 {
        bail!("не удалось узнать размер экрана");
    }
    let mut buf = vec![0u8; (w * h * 4) as usize];
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let blit = BitBlt(mem, 0, 0, w, h, Some(screen), r.left, r.top, SRCCOPY);
        SelectObject(mem, old);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // сверху вниз
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let lines = GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        blit.context("BitBlt")?;
        if lines == 0 {
            bail!("GetDIBits не вернул строк");
        }
    }
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2); // BGRA → RGBA
        px[3] = 255;
    }
    let img = RgbaImage::from_raw(w as u32, h as u32, buf).context("буфер снимка")?;
    let cursor = ((cx - r.left).clamp(0, w - 1) as u32, (cy - r.top).clamp(0, h - 1) as u32);
    Ok(Shot { img, left: r.left, top: r.top, cursor })
}

// ---------- OCR ----------

/// Движки распознавания: английский и русский, какие есть в системе.
static ENGINES: OnceLock<Vec<(String, OcrEngine)>> = OnceLock::new();

fn ensure_mta() {
    static MTA: OnceLock<()> = OnceLock::new();
    MTA.get_or_init(|| unsafe {
        // WinRT в фоновых потоках без явной инициализации COM
        let _ = windows::Win32::System::Com::CoIncrementMTAUsage();
    });
}

pub fn ocr_engines() -> &'static [(String, OcrEngine)] {
    ENGINES.get_or_init(|| {
        ensure_mta();
        let mut out = Vec::new();
        let Ok(langs) = OcrEngine::AvailableRecognizerLanguages() else { return out };
        let n = langs.Size().unwrap_or(0);
        for prefix in ["en", "ru"] {
            for i in 0..n {
                let Ok(lang) = langs.GetAt(i) else { continue };
                let tag = lang.LanguageTag().map(|t| t.to_string()).unwrap_or_default();
                if tag.to_lowercase().starts_with(prefix) {
                    if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                        out.push((tag, engine));
                    }
                    break;
                }
            }
        }
        out
    })
}

/// Текст на картинке.
/// Одновременные вызовы распознавания портят друг другу результат (пустой текст через раз) —
/// распознаём строго по одному.
static OCR_LOCK: Mutex<()> = Mutex::new(());

pub fn recognize(engine: &OcrEngine, img: &RgbaImage) -> Result<String> {
    ensure_mta();
    let _guard = OCR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut bgra = img.as_raw().clone();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let writer = DataWriter::new()?;
    writer.WriteBytes(&bgra)?;
    let buffer = writer.DetachBuffer()?;
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, img.width() as i32, img.height() as i32)?;
    let result = engine.RecognizeAsync(&bitmap)?.get()?;
    Ok(result.Text()?.to_string())
}

// ---------- горячие клавиши: низкоуровневые хуки ----------

const CTRL: u8 = 1;
const ALT: u8 = 2;
const SHIFT: u8 = 4;
const WIN: u8 = 8;

#[derive(Clone, PartialEq, Debug)]
pub struct Combo {
    mods: u8,
    key: String,
    mouse: bool,
}

/// Русская раскладка → клавиша на том же месте в английской (сочетание могли записать в любой).
fn latin_key(key: &str) -> String {
    const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";
    const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if let Some(i) = RU.chars().position(|r| r == c) {
            return EN.chars().nth(i).unwrap().to_string();
        }
    }
    key.to_string()
}

/// "ctrl-shift-f9" → модификаторы и клавиша.
pub fn parse_combo(s: &str) -> Option<Combo> {
    let s = s.to_lowercase();
    let parts: Vec<&str> = s.split('-').filter(|p| !p.is_empty()).collect();
    let (key, mods) = parts.split_last()?;
    let mut m = 0;
    for p in mods {
        m |= match *p {
            "ctrl" => CTRL,
            "alt" => ALT,
            "shift" => SHIFT,
            "win" => WIN,
            _ => 0,
        };
    }
    Some(Combo { mods: m, key: latin_key(key), mouse: crate::hotkey::is_mouse(&s) })
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct HookConfig {
    pub enabled: bool,
    pub only_in_game: bool,
    pub scan: Option<Combo>,
    pub hide: Option<Combo>,
    pub raid: Option<Combo>,
}

impl HookConfig {
    fn needs_mouse(&self) -> bool {
        self.enabled && [&self.scan, &self.hide, &self.raid].iter().any(|c| c.as_ref().is_some_and(|c| c.mouse))
    }
}

struct Hooks {
    tx: UnboundedSender<HotkeyEvent>,
    cfg: Mutex<HookConfig>,
    /// Нажатые сейчас клавиши/кнопки — чтобы автоповтор не сканировал заново.
    down: Mutex<HashSet<u32>>,
    /// Клавиша сканирования, которую держат (режим «пока держу»).
    held: Mutex<Option<u32>>,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);

fn mods_now() -> u8 {
    let on = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) } < 0;
    let mut m = 0;
    if on(0x11) {
        m |= CTRL;
    }
    if on(0x12) {
        m |= ALT;
    }
    if on(0x10) {
        m |= SHIFT;
    }
    if on(0x5B) || on(0x5C) {
        m |= WIN;
    }
    m
}

/// Окно игры впереди (у Tarkov заголовок окна — «EscapeFromTarkov»).
pub fn game_in_front() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut buf = [0u16; 64];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n.max(0) as usize]).starts_with("EscapeFromTarkov")
    }
}

/// Имена клавиши так же, как их пишет GPUI при записи сочетания.
fn key_names(vk: u32) -> Vec<String> {
    let fixed = match vk {
        0x20 => "space",
        0x08 => "backspace",
        0x0D => "enter",
        0x09 => "tab",
        0x26 => "up",
        0x28 => "down",
        0x27 => "right",
        0x25 => "left",
        0x24 => "home",
        0x23 => "end",
        0x21 => "pageup",
        0x22 => "pagedown",
        0x1B => "escape",
        0x2D => "insert",
        0x2E => "delete",
        0x5D => "menu",
        _ => "",
    };
    if !fixed.is_empty() {
        return vec![fixed.into()];
    }
    if (0x70..=0x87).contains(&vk) {
        return vec![format!("f{}", vk - 0x6F)];
    }
    let mut out = Vec::new();
    if (0x30..=0x39).contains(&vk) || (0x41..=0x5A).contains(&vk) {
        out.push((vk as u8 as char).to_ascii_lowercase().to_string());
    }
    let ch = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) } & 0xFFFF;
    if let Some(c) = char::from_u32(ch).filter(|_| ch != 0) {
        let name = if c == '-' { "minus".to_string() } else { latin_key(&c.to_lowercase().to_string()) };
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0xA0..=0xA5 | 0x5B | 0x5C | 0x14)
}

impl Hooks {
    fn input(&self, id: u32, names: &[String], down: bool) {
        let cfg = self.cfg.lock().unwrap();
        if !cfg.enabled {
            return;
        }
        if !down {
            self.down.lock().unwrap().remove(&id);
            let mut held = self.held.lock().unwrap();
            if *held == Some(id) {
                *held = None;
                let _ = self.tx.unbounded_send(HotkeyEvent::ScanUp);
            }
            return;
        }
        if !self.down.lock().unwrap().insert(id) {
            return; // автоповтор
        }
        let mods = mods_now();
        let hit = |c: &Option<Combo>| c.as_ref().is_some_and(|c| c.mods == mods && names.contains(&c.key));
        let ev = if hit(&cfg.scan) {
            HotkeyEvent::Scan
        } else if hit(&cfg.hide) {
            HotkeyEvent::Hide
        } else if hit(&cfg.raid) {
            HotkeyEvent::Raid
        } else {
            return;
        };
        if cfg.only_in_game && !game_in_front() {
            return;
        }
        if ev == HotkeyEvent::Scan {
            *self.held.lock().unwrap() = Some(id);
        }
        let _ = self.tx.unbounded_send(ev);
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        if let Some(h) = HOOKS.get() {
            let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
            let msg = wp.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if (down || up) && !is_modifier(k.vkCode) {
                h.input(k.vkCode, &key_names(k.vkCode), down);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        if let Some(h) = HOOKS.get() {
            let m = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
            let x2 = (m.mouseData >> 16) as u16 == 2;
            let button = match wp.0 as u32 {
                WM_XBUTTONDOWN => Some((if x2 { "mouse5" } else { "mouse4" }, true)),
                WM_XBUTTONUP => Some((if x2 { "mouse5" } else { "mouse4" }, false)),
                WM_MBUTTONDOWN => Some(("mmb", true)),
                WM_MBUTTONUP => Some(("mmb", false)),
                WM_LBUTTONDOWN => Some(("lmb", true)),
                WM_LBUTTONUP => Some(("lmb", false)),
                WM_RBUTTONDOWN => Some(("rmb", true)),
                WM_RBUTTONUP => Some(("rmb", false)),
                _ => None,
            };
            if let Some((name, down)) = button {
                let id = 0x1_0000 + name.bytes().fold(0u32, |a, b| a * 31 + b as u32) % 0xFFFF;
                h.input(id, &[name.to_string()], down);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// Запускает поток с хуками. Хук мыши ставится, только если в сочетаниях есть кнопка мыши:
/// он вызывается на каждое движение мыши, а в игре это лишнее.
pub fn start_hooks() -> Option<UnboundedReceiver<HotkeyEvent>> {
    let (tx, rx) = unbounded();
    let hooks = Hooks {
        tx,
        cfg: Mutex::new(HookConfig::default()),
        down: Mutex::new(HashSet::new()),
        held: Mutex::new(None),
    };
    if HOOKS.set(hooks).is_err() {
        return None;
    }
    std::thread::Builder::new()
        .name("scanner-hooks".into())
        .spawn(|| unsafe {
            let mut msg = MSG::default();
            // создаём очередь сообщений потока до того, как в неё начнут писать
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            HOOK_THREAD.store(windows::Win32::System::Threading::GetCurrentThreadId(), Ordering::SeqCst);
            let hinst = GetModuleHandleW(None).ok().map(|h| h.into());
            let _keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0);
            let mut mouse: Option<HHOOK> = None;
            let sync_mouse = |mouse: &mut Option<HHOOK>| {
                let need = HOOKS.get().is_some_and(|h| h.cfg.lock().unwrap().needs_mouse());
                if need && mouse.is_none() {
                    *mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0).ok();
                } else if !need {
                    if let Some(m) = mouse.take() {
                        let _ = UnhookWindowsHookEx(m);
                    }
                }
            };
            sync_mouse(&mut mouse);
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_APP {
                    sync_mouse(&mut mouse);
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .ok()?;
    Some(rx)
}

/// Новые сочетания и включённость сканера.
pub fn configure(cfg: HookConfig) {
    let Some(h) = HOOKS.get() else { return };
    {
        let mut cur = h.cfg.lock().unwrap();
        if *cur == cfg {
            return;
        }
        *cur = cfg;
    }
    h.down.lock().unwrap().clear();
    *h.held.lock().unwrap() = None;
    let tid = HOOK_THREAD.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_APP, WPARAM(0), LPARAM(0));
        }
    }
}

// ---------- окна ----------

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

/// HWND окна GPUI.
pub fn window_hwnd(window: &gpui::Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// Поверх всех окон, в том числе игры в режиме «оконный без рамки».
pub fn set_topmost(h: isize) {
    unsafe {
        let _ = SetWindowPos(hwnd(h), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Карточка цены: не забирает фокус у игры и не попадает в снимки экрана (иначе следующий скан
/// увидел бы её вместо предмета). Сквозные клики (WS_EX_LAYERED | WS_EX_TRANSPARENT) не ставим:
/// окно GPUI рисуется через DirectComposition, и слоистое окно может остаться пустым.
pub fn prepare_card(h: isize) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd(h), GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE | WS_EX_TOPMOST).0 as isize);
        let _ = SetWindowDisplayAffinity(hwnd(h), WDA_EXCLUDEFROMCAPTURE);
    }
}

/// Показать окно в точке экрана (физические пиксели) поверх всех, не активируя.
pub fn show_at(h: isize, x: i32, y: i32, w: i32, height: i32) {
    unsafe {
        let _ = SetWindowPos(hwnd(h), Some(HWND_TOPMOST), x, y, w, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
}

pub fn hide(h: isize) {
    unsafe {
        let _ = ShowWindow(hwnd(h), SW_HIDE);
    }
}

/// Масштаб окна: физические пиксели на логический.
pub fn scale_of(h: isize) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd(h)) };
    if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
}

/// Координаты в физических пикселях — иначе при масштабе 125–150 % снимок и курсор разъедутся.
pub fn make_dpi_aware() {
    use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
