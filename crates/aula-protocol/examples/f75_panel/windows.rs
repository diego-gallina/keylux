use crate::profiles::{load, parse_hex, save, scale, Profile};
use aula_protocol::{
    f75::protocol::WIRED, transport::ProbeResult, ChannelOrder, Confidence, DeviceCandidate, Frame,
    Link, Rgb, RgbDevice, ScanOptions, F75,
};
use std::{
    cell::RefCell,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        Controls::Dialogs::*,
        Controls::*,
        Input::KeyboardAndMouse::{EnableWindow, SetFocus},
        WindowsAndMessaging::*,
    },
};

const COLOR: u16 = 10;
const BRIGHT: u16 = 20;
const SECONDS: u16 = 21;
const CONT: u16 = 22;
const APPLY: u16 = 23;
const STOP: u16 = 24;
const OFF: u16 = 25;
const CHECK: u16 = 26;
const SAVE: u16 = 27;
const LOAD: u16 = 28;
const FIRST: u16 = 30;
const PRESETS: [(&str, [u8; 3]); 4] = [
    ("Ciano", [0, 255, 255]),
    ("Quente", [255, 180, 107]),
    ("Roxo", [139, 92, 246]),
    ("Branco", [255, 255, 255]),
];
const ROWS: [&[f32]; 6] = [
    &[
        1., -0.5, 1., 1., 1., 1., -0.35, 1., 1., 1., 1., -0.35, 1., 1., 1., 1., -0.3, 1.1,
    ],
    &[1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 2., 1.],
    &[1.5, 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1.5, 1.],
    &[1.75, 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 2.25, 1.],
    &[2.25, 1., 1., 1., 1., 1., 1., 1., 1., 1., 1., 1.75, 1., 1.],
    &[1.25, 1.25, 1.25, 6.25, 1., 1., 1., 1., 1., 1.],
];
const GAP: Duration = Duration::from_millis(200);
thread_local! {static APP:RefCell<Option<App>>=const{RefCell::new(None)};}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
fn cr(c: [u8; 3]) -> u32 {
    u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16)
}
fn rgb(c: u32) -> [u8; 3] {
    [c as u8, (c >> 8) as u8, (c >> 16) as u8]
}
fn text(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), 128) }.max(0);
    String::from_utf16_lossy(&buf[..n as usize])
}

#[derive(Clone, Copy)]
enum Mode {
    Check,
    Off,
    Hold([u8; 3], Option<Duration>),
}
fn confirmed() -> anyhow::Result<F75> {
    let mut ready = F75::probe(&ScanOptions::default())?
        .into_iter()
        .filter(|p| {
            p.info.id == WIRED
                && p.info.is_vendor()
                && matches!(p.result, ProbeResult::Answered { confirmed: true })
        });
    let p = ready.next().ok_or_else(|| {
        anyhow::anyhow!("F75 não confirmado. Confira o cabo USB-C e o modo com fio.")
    })?;
    anyhow::ensure!(
        ready.next().is_none(),
        "Mais de um F75 confirmado. Deixe só um conectado."
    );
    let d = F75::open_candidate(
        &DeviceCandidate {
            id: WIRED,
            link: Link::Wired,
            model: "AULA F75",
            path: p.info.path,
            product: p.info.product,
            confidence: Confidence::Known,
            confirmed: true,
        },
        ChannelOrder::Rgb,
    )?;
    d.read_config()?;
    Ok(d)
}
fn run_mode(mode: Mode, cancel: &AtomicBool) -> anyhow::Result<&'static str> {
    let mut device = confirmed()?;
    if cancel.load(Ordering::Relaxed) {
        return Ok("Envio interrompido.");
    }
    match mode {
        Mode::Check => Ok("F75 confirmado por USB-C."),
        Mode::Off => {
            device.stream(&Frame::black(device.led_count()))?;
            Ok("Comando Apagar enviado.")
        }
        Mode::Hold([r, g, b], duration) => {
            let frame = Frame::solid(device.led_count(), Rgb::new(r, g, b));
            let start = Instant::now();
            while !cancel.load(Ordering::Relaxed) {
                if duration.is_some_and(|max| start.elapsed() >= max) {
                    return Ok("Envio concluído.");
                }
                device.refresh_wired_volatile(&frame)?;
                thread::sleep(
                    duration.map_or(GAP, |max| GAP.min(max.saturating_sub(start.elapsed()))),
                );
            }
            Ok("Envio interrompido. Use Apagar se quiser desligar a luz.")
        }
    }
}
struct Job {
    cancel: Arc<AtomicBool>,
    rx: Receiver<anyhow::Result<&'static str>>,
}
struct App {
    hwnd: HWND,
    color: HWND,
    brightness: HWND,
    brightness_label: HWND,
    seconds: HWND,
    continuous: HWND,
    profile: HWND,
    status: HWND,
    apply: HWND,
    stop: HWND,
    off: HWND,
    check: HWND,
    font: HFONT,
    title_font: HFONT,
    bg: HBRUSH,
    dark: bool,
    dpi: i32,
    rgb: [u8; 3],
    job: Option<Job>,
    path: Option<PathBuf>,
    recent: [u32; 16],
}
impl Drop for App {
    fn drop(&mut self) {
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
        unsafe {
            DeleteObject(self.font);
            DeleteObject(self.title_font);
            DeleteObject(self.bg);
        }
    }
}
impl App {
    fn px(&self, n: i32) -> i32 {
        (n * self.dpi + 48) / 96
    }
    fn status(&self, s: &str) {
        unsafe {
            SetWindowTextW(self.status, w(s).as_ptr());
        }
    }
    fn brightness(&self) -> u8 {
        unsafe { SendMessageW(self.brightness, WM_USER, 0, 0).clamp(0, 100) as u8 }
    }
    fn color(&mut self, c: [u8; 3]) {
        self.rgb = c;
        unsafe {
            SetWindowTextW(
                self.color,
                w(&format!("Cor #{:02X}{:02X}{:02X}", c[0], c[1], c[2])).as_ptr(),
            );
            InvalidateRect(self.hwnd, null(), 0);
        }
    }
    fn preview(&self) {
        unsafe {
            SetWindowTextW(
                self.brightness_label,
                w(&format!("Brilho: {}%", self.brightness())).as_ptr(),
            );
            InvalidateRect(self.hwnd, null(), 0);
        }
    }
    fn continuous(&self) -> bool {
        unsafe { SendMessageW(self.continuous, BM_GETCHECK, 0, 0) == 1 }
    }
    fn toggle(&self) {
        unsafe {
            EnableWindow(self.seconds, (!self.continuous()) as i32);
        }
    }
    fn seconds(&self) -> Option<u16> {
        let n = text(self.seconds).trim().parse().ok()?;
        (1..=3600).contains(&n).then_some(n)
    }
    fn refresh_profiles(&self, select: &str) {
        let Some(path) = &self.path else {
            return;
        };
        match load(path) {
            Ok(rows) => unsafe {
                SendMessageW(self.profile, CB_RESETCONTENT, 0, 0);
                for p in rows {
                    SendMessageW(self.profile, CB_ADDSTRING, 0, w(&p.name).as_ptr() as isize);
                }
                SetWindowTextW(self.profile, w(select).as_ptr());
            },
            Err(_) => self.status("Não foi possível ler os perfis. O arquivo foi mantido."),
        }
    }
    fn busy(&self, busy: bool) {
        unsafe {
            for b in [self.apply, self.off, self.check] {
                EnableWindow(b, (!busy) as i32);
            }
            EnableWindow(self.stop, busy as i32);
        }
    }
    fn start(&mut self, mode: Mode) {
        if self.job.is_some() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let (tx, rx) = mpsc::channel();
        if thread::Builder::new()
            .name("keylux-hid".into())
            .spawn(move || {
                let _ = tx.send(run_mode(mode, &flag));
            })
            .is_err()
        {
            self.status("Não consegui iniciar o controlador.");
            return;
        }
        self.job = Some(Job { cancel, rx });
        self.busy(true);
        unsafe {
            SetFocus(self.stop);
        }
        self.status("Aguarde…");
    }
    fn finish(&mut self) {
        let Some(j) = &self.job else {
            return;
        };
        let reply = match j.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("O controlador foi interrompido."))
            }
        };
        self.job = None;
        self.busy(false);
        unsafe {
            SetFocus(self.apply);
        }
        match reply {
            Ok(s) => self.status(s),
            Err(e) => self.status(&format!("{e}")),
        }
    }
    fn command(&mut self, id: u16, notice: u16) {
        if notice != 0 {
            return;
        }
        match id {
            FIRST..=33 => self.color(PRESETS[usize::from(id - FIRST)].1),
            COLOR => unsafe {
                let mut d = CHOOSECOLORW {
                    lStructSize: size_of::<CHOOSECOLORW>() as u32,
                    hwndOwner: self.hwnd,
                    rgbResult: cr(self.rgb),
                    lpCustColors: self.recent.as_mut_ptr(),
                    Flags: CC_RGBINIT | CC_FULLOPEN,
                    ..Default::default()
                };
                if ChooseColorW(&mut d) != 0 {
                    self.color(rgb(d.rgbResult));
                }
            },
            CONT => self.toggle(),
            CHECK => self.start(Mode::Check),
            OFF => self.start(Mode::Off),
            STOP => {
                if let Some(j) = &self.job {
                    j.cancel.store(true, Ordering::Relaxed);
                    self.status("Parando…");
                }
            }
            APPLY => {
                let duration = if self.continuous() {
                    None
                } else if let Some(n) = self.seconds() {
                    Some(Duration::from_secs(n.into()))
                } else {
                    self.status("Use de 1 a 3600 segundos.");
                    return;
                };
                self.start(Mode::Hold(scale(self.rgb, self.brightness()), duration));
            }
            SAVE => {
                let Some(path) = &self.path else {
                    self.status("Pasta de perfis indisponível.");
                    return;
                };
                let name = text(self.profile).trim().to_string();
                let Some(seconds) = self.seconds() else {
                    self.status("Use de 1 a 3600 segundos.");
                    return;
                };
                let p = Profile {
                    name: name.clone(),
                    color: format!("{:02X}{:02X}{:02X}", self.rgb[0], self.rgb[1], self.rgb[2]),
                    brightness: self.brightness(),
                    seconds,
                    continuous: self.continuous(),
                };
                let rows = match load(path) {
                    Ok(p) => p,
                    Err(_) => {
                        self.status("Arquivo de perfis inválido. Nada foi alterado.");
                        return;
                    }
                };
                if rows
                    .iter()
                    .any(|p| p.name.to_lowercase() == name.to_lowercase())
                    && unsafe {
                        MessageBoxW(
                            self.hwnd,
                            w("Substituir o perfil existente?").as_ptr(),
                            w("Salvar perfil").as_ptr(),
                            MB_YESNO | MB_DEFBUTTON2 | MB_ICONQUESTION,
                        ) != IDYES
                    }
                {
                    return;
                }
                match save(path, p) {
                    Ok(()) => {
                        self.refresh_profiles(&name);
                        self.status("Perfil salvo só neste PC.");
                    }
                    Err(e) => self.status(&format!("{e}")),
                }
            }
            LOAD => {
                let Some(path) = &self.path else {
                    return;
                };
                let name = text(self.profile).to_lowercase();
                let record = match load(path) {
                    Ok(p) => p.into_iter().find(|p| p.name.to_lowercase() == name),
                    Err(_) => {
                        self.status("Arquivo de perfis inválido. Nada foi alterado.");
                        return;
                    }
                };
                let Some(p) = record else {
                    self.status("Escolha um perfil salvo.");
                    return;
                };
                if let Some(c) = parse_hex(&p.color) {
                    self.color(c);
                }
                unsafe {
                    SendMessageW(self.brightness, TBM_SETPOS, 1, isize::from(p.brightness));
                    SetWindowTextW(self.seconds, w(&p.seconds.to_string()).as_ptr());
                    SendMessageW(self.continuous, BM_SETCHECK, p.continuous as usize, 0);
                }
                self.toggle();
                self.preview();
                self.status("Perfil carregado. Clique em Aplicar.");
            }
            _ => {}
        }
    }
}
unsafe fn br(rgb: [u8; 3]) -> HBRUSH {
    unsafe { CreateSolidBrush(cr(rgb)) }
}
unsafe fn rect(dc: HDC, x: i32, y: i32, w: i32, h: i32, b: HBRUSH) {
    unsafe {
        FillRect(
            dc,
            &RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            b,
        );
    }
}
unsafe fn paint(a: &App, hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r);
        FillRect(dc, &r, a.bg);
        SetTextColor(
            dc,
            if a.dark {
                cr([237, 241, 248])
            } else {
                GetSysColor(COLOR_WINDOWTEXT)
            },
        );
        SetBkMode(dc, TRANSPARENT as i32);
        let old = SelectObject(dc, a.title_font);
        let title = w("Luz do F75");
        TextOutW(
            dc,
            a.px(25),
            a.px(15),
            title.as_ptr(),
            (title.len() - 1) as i32,
        );
        SelectObject(dc, a.font);
        let hint = w("USB-C  ·  PRÉVIA");
        TextOutW(
            dc,
            a.px(27),
            a.px(68),
            hint.as_ptr(),
            (hint.len() - 1) as i32,
        );
        let deck = if a.dark {
            br([28, 34, 43])
        } else {
            GetSysColorBrush(COLOR_BTNFACE)
        };
        rect(dc, a.px(25), a.px(97), a.px(610), a.px(172), deck);
        let inner = br(scale(a.rgb, a.brightness()));
        let edge = if a.dark {
            br([72, 82, 96])
        } else {
            GetSysColorBrush(COLOR_WINDOWTEXT)
        };
        let unit = a.px(564) as f32 / 16.6;
        for (i, row) in ROWS.iter().enumerate() {
            let mut x = a.px(48) as f32;
            for width in *row {
                let step = width.abs() * unit;
                if *width > 0.0 {
                    let y = a.px(112 + i as i32 * 23);
                    rect(
                        dc,
                        x as i32,
                        y,
                        (step as i32 - a.px(4)).max(2),
                        a.px(19),
                        edge,
                    );
                    rect(
                        dc,
                        x as i32 + 1,
                        y + 1,
                        (step as i32 - a.px(4) - 2).max(1),
                        a.px(19) - 2,
                        inner,
                    );
                }
                x += step;
            }
        }
        if a.dark {
            DeleteObject(deck);
            DeleteObject(edge);
        }
        DeleteObject(inner);
        SelectObject(dc, old);
        EndPaint(hwnd, &ps);
    }
}
unsafe extern "system" fn event(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            APP.with(|s| {
                if let Ok(mut a) = s.try_borrow_mut() {
                    if let Some(a) = a.as_mut() {
                        a.command((wp & 65535) as u16, ((wp >> 16) & 65535) as u16);
                    }
                }
            });
            0
        }
        WM_TIMER => {
            APP.with(|s| {
                if let Ok(mut a) = s.try_borrow_mut() {
                    if let Some(a) = a.as_mut() {
                        a.finish();
                    }
                }
            });
            0
        }
        WM_HSCROLL => {
            APP.with(|s| {
                if let Ok(a) = s.try_borrow() {
                    if let Some(a) = a.as_ref() {
                        a.preview();
                    }
                }
            });
            0
        }
        WM_CTLCOLORSTATIC => {
            let color = APP.with(|s| {
                s.try_borrow()
                    .ok()
                    .and_then(|a| a.as_ref().filter(|a| a.dark).map(|a| a.bg as isize))
            });
            if let Some(c) = color {
                unsafe {
                    SetTextColor(wp as HDC, cr([186, 201, 217]));
                    SetBkMode(wp as HDC, TRANSPARENT as i32);
                }
                c
            } else {
                unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
            }
        }
        WM_PAINT => {
            let painted = APP.with(|s| {
                if let Ok(a) = s.try_borrow() {
                    if let Some(a) = a.as_ref() {
                        unsafe { paint(a, hwnd) };
                        return true;
                    }
                    false
                } else {
                    false
                }
            });
            if painted {
                0
            } else {
                unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
            }
        }
        WM_DESTROY => {
            unsafe {
                KillTimer(hwnd, 1);
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
pub fn run() {
    if let Err(e) = unsafe { start_window() } {
        unsafe {
            MessageBoxW(
                null_mut(),
                w(&e.to_string()).as_ptr(),
                w("Keylux F75").as_ptr(),
                MB_ICONERROR,
            );
        }
    }
}

unsafe fn start_window() -> anyhow::Result<()> {
    unsafe {
        SetProcessDPIAware();
        let dc = GetDC(null_mut());
        let dpi = if dc.is_null() {
            96
        } else {
            GetDeviceCaps(dc, LOGPIXELSX as i32).clamp(96, 288)
        };
        if !dc.is_null() {
            ReleaseDC(null_mut(), dc);
        }
        let px = |n: i32| (n * dpi + 48) / 96;
        let mut hc = HIGHCONTRASTW {
            cbSize: size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            (&mut hc as *mut HIGHCONTRASTW).cast(),
            0,
        );
        let dark = hc.dwFlags & HCF_HIGHCONTRASTON == 0;
        let common = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        InitCommonControlsEx(&common);
        let instance = GetModuleHandleW(null());
        let class_name = w("KeyluxF75Panel");
        let class = WNDCLASSW {
            lpfnWndProc: Some(event),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..Default::default()
        };
        anyhow::ensure!(
            RegisterClassW(&class) != 0,
            "Não consegui registrar a janela."
        );
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut r = RECT {
            left: 0,
            top: 0,
            right: px(660),
            bottom: px(545),
        };
        AdjustWindowRect(&mut r, style, 0);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class_name.as_ptr(),
            w("Keylux | AULA F75").as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            r.right - r.left,
            r.bottom - r.top,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        anyhow::ensure!(!hwnd.is_null(), "Não consegui abrir a janela.");
        let font = CreateFontW(
            -px(15),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.into(),
            0,
            0,
            CLEARTYPE_QUALITY.into(),
            0,
            w("Segoe UI").as_ptr(),
        );
        let title_font = CreateFontW(
            -px(36),
            0,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.into(),
            0,
            0,
            CLEARTYPE_QUALITY.into(),
            0,
            w("Segoe UI").as_ptr(),
        );
        let make = |class: &str,
                    label: &str,
                    id: u16,
                    extra: u32,
                    x: i32,
                    y: i32,
                    width: i32,
                    height: i32| {
            let c = CreateWindowExW(
                0,
                w(class).as_ptr(),
                w(label).as_ptr(),
                WS_VISIBLE | WS_CHILD | extra,
                px(x),
                px(y),
                px(width),
                px(height),
                hwnd,
                usize::from(id) as HMENU,
                instance,
                null_mut(),
            );
            if !c.is_null() {
                SendMessageW(c, WM_SETFONT, font as usize, 1);
            }
            c
        };
        let b = |extra| WS_TABSTOP | extra;
        let color = make("BUTTON", "Cor #00FFFF", COLOR, b(0), 25, 285, 158, 37);
        for (i, (name, _)) in PRESETS.iter().enumerate() {
            make(
                "BUTTON",
                name,
                FIRST + i as u16,
                b(0),
                193 + i as i32 * 113,
                285,
                103,
                37,
            );
        }
        let brightness_label = make("STATIC", "Brilho: 35%", 0, 0, 25, 342, 278, 24);
        let brightness = make(
            "msctls_trackbar32",
            "Brilho",
            BRIGHT,
            b(TBS_AUTOTICKS as u32),
            22,
            367,
            320,
            44,
        );
        SendMessageW(brightness, TBM_SETRANGEMAX, 1, 100);
        SendMessageW(brightness, TBM_SETPOS, 1, 35);
        SendMessageW(brightness, TBM_SETTICFREQ, 10, 0);
        make("STATIC", "Segundos", 0, 0, 367, 345, 80, 24);
        let seconds = make(
            "EDIT",
            "10",
            SECONDS,
            b(WS_BORDER | ES_NUMBER as u32),
            455,
            340,
            180,
            31,
        );
        SendMessageW(seconds, EM_SETLIMITTEXT, 4, 0);
        let continuous = make(
            "BUTTON",
            "Até eu parar",
            CONT,
            b(BS_AUTOCHECKBOX as u32),
            368,
            376,
            265,
            28,
        );
        let apply = make(
            "BUTTON",
            "&Aplicar",
            APPLY,
            b(BS_DEFPUSHBUTTON as u32),
            25,
            420,
            230,
            40,
        );
        let stop = make("BUTTON", "&Parar", STOP, b(0), 264, 420, 110, 40);
        let off = make("BUTTON", "Apa&gar", OFF, b(0), 383, 420, 105, 40);
        let check = make("BUTTON", "Verificar USB", CHECK, b(0), 497, 420, 138, 40);
        let profile = make(
            "COMBOBOX",
            "",
            0,
            b(CBS_DROPDOWN as u32 | CBS_HASSTRINGS as u32 | WS_VSCROLL),
            25,
            476,
            340,
            160,
        );
        SendMessageW(profile, CB_LIMITTEXT, 30, 0);
        let load_button = make("BUTTON", "Carregar", LOAD, b(0), 376, 475, 118, 32);
        let save_button = make("BUTTON", "Salvar perfil", SAVE, b(0), 503, 475, 132, 32);
        let status = make(
            "STATIC",
            "Pronto: ciano a 35% por 10 s.",
            0,
            0,
            25,
            515,
            611,
            25,
        );
        anyhow::ensure!(
            [
                color,
                brightness,
                seconds,
                continuous,
                apply,
                stop,
                off,
                check,
                profile,
                save_button,
                load_button,
                status
            ]
            .iter()
            .all(|h| !h.is_null()),
            "Não consegui criar os controles."
        );
        let app = App {
            hwnd,
            color,
            brightness,
            brightness_label,
            seconds,
            continuous,
            profile,
            status,
            apply,
            stop,
            off,
            check,
            font,
            title_font,
            bg: br(if dark {
                [15, 19, 25]
            } else {
                rgb(GetSysColor(COLOR_WINDOW))
            }),
            dark,
            dpi,
            rgb: [0, 255, 255],
            job: None,
            path: std::env::var_os("LOCALAPPDATA")
                .filter(|p| !p.is_empty())
                .map(|p| PathBuf::from(p).join("KeyluxF75").join("profiles.json")),
            recent: [0; 16],
        };
        app.busy(false);
        app.refresh_profiles("");
        APP.with(|s| *s.borrow_mut() = Some(app));
        SetTimer(hwnd, 1, 150, None);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetFocus(apply);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        APP.with(|s| {
            s.borrow_mut().take();
        });
        Ok(())
    }
}
