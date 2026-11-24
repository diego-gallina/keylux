//! A deliberately small, local-only AULA F75 USB-C lighting entry point.
//! Dual-licensed under MIT OR Apache-2.0, as the rest of Keylux.
#![forbid(unsafe_code)]

use aula_protocol::{
    f75::protocol::WIRED, transport::ProbeResult, ChannelOrder, Confidence, DeviceCandidate, Frame,
    Link, Rgb, RgbDevice, ScanOptions, F75,
};
use std::{
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

const HOLD_INTERVAL: Duration = Duration::from_millis(200);

const HELP: &str = "F75 USB-C (258a:010c)\n\
    f75_usb check\n\
    f75_usb diagnose\n\
    f75_usb preview <RRGGBB|warm|focus|off> [brightness 0-100]\n\
    f75_usb apply <RRGGBB|warm|focus|off> [brightness 0-100]\n\
    f75_usb hold <RRGGBB|warm|focus|off> <brightness 0-100> [seconds 1-3600]\n\
    Ctrl+C to stop a hold without duration; last sent color is not restored.\n\
    preview never touches hardware; apply sends one volatile RGB frame.\n\
    No installation, network, or firmware update during use.";

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Help,
    Check,
    Diagnose,
    Preview([u8; 3]),
    Apply([u8; 3]),
    Hold([u8; 3], Option<Duration>),
}

fn parse(args: &[String]) -> Result<Action, &'static str> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] | ["help" | "--help" | "-h"] => Ok(Action::Help),
        ["check"] => Ok(Action::Check),
        ["diagnose"] => Ok(Action::Diagnose),
        [command @ ("apply" | "preview"), color] => {
            parse_color(color, "100").map(|rgb| color_action(command, rgb))
        }
        [command @ ("apply" | "preview"), color, brightness] => {
            parse_color(color, brightness).map(|rgb| color_action(command, rgb))
        }
        ["hold", color, brightness] => {
            parse_color(color, brightness).map(|rgb| Action::Hold(rgb, None))
        }
        ["hold", color, brightness, seconds] => {
            let seconds = seconds
                .parse::<u64>()
                .ok()
                .filter(|s| (1..=3600).contains(s))
                .ok_or("Hold duration must be an integer from 1 to 3600 seconds.")?;
            parse_color(color, brightness)
                .map(|rgb| Action::Hold(rgb, Some(Duration::from_secs(seconds))))
        }
        _ => Err("Expected check, diagnose, preview, apply, or hold. Try --help."),
    }
}

fn color_action(command: &str, rgb: [u8; 3]) -> Action {
    if command == "apply" {
        Action::Apply(rgb)
    } else {
        Action::Preview(rgb)
    }
}

fn parse_color(color: &str, brightness: &str) -> Result<[u8; 3], &'static str> {
    let level = brightness
        .parse::<u16>()
        .ok()
        .filter(|b| *b <= 100)
        .ok_or("Brightness must be an integer from 0 to 100.")?;
    let hex = match color {
        "warm" => "FFB46B",
        "focus" => "85B4FF",
        "off" => "000000",
        hex => hex.strip_prefix('#').unwrap_or(hex),
    };
    if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Color must be six hex digits, warm, focus, or off.");
    }
    let rgb: [u8; 3] = std::array::from_fn(|i| {
        let v = u16::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("validated ASCII hex");
        ((v * level + 50) / 100) as u8
    });
    Ok(rgb)
}

fn find_wired() -> anyhow::Result<F75> {
    // No user-supplied VID/PID, paths, deep scan, or wireless fallback.
    let mut candidates = F75::probe(&ScanOptions::default())?
        .into_iter()
        .filter(|d| {
            d.info.id == WIRED
                && d.info.is_vendor()
                && matches!(d.result, ProbeResult::Answered { confirmed: true })
        });
    let candidate = candidates.next().ok_or_else(|| anyhow::anyhow!(
        "Supported AULA F75 (258a:010c) not confirmed. Use a data-capable USB-C cable and wired mode. Do not force another USB ID."
    ))?;
    if candidates.next().is_some() {
        anyhow::bail!("More than one supported device found; disconnect the extra keyboard.");
    }
    let candidate = DeviceCandidate {
        id: WIRED,
        link: Link::Wired,
        model: "AULA F75",
        path: candidate.info.path,
        product: candidate.info.product,
        confidence: Confidence::Known,
        confirmed: true,
    };
    Ok(F75::open_candidate(&candidate, ChannelOrder::Rgb)?)
}

/// Report only per-collection metadata and read probe outcomes. The upstream
/// probe issues the fixed config READ command; it does not change settings.
fn diagnose() -> anyhow::Result<()> {
    let cols = F75::probe(&ScanOptions::default())?;
    let mut found = false;
    for col in cols.into_iter().filter(|col| col.info.id == WIRED) {
        found = true;
        let status = match col.result {
            ProbeResult::OsOwned => "not opened (OS-owned or outside the probe's vendor scope)",
            ProbeResult::Unopenable(_) => "open failed (HID handle unavailable)",
            ProbeResult::Silent => "open succeeded; initial RGB feature read failed",
            ProbeResult::Answered { confirmed: false } => {
                "initial feature read succeeded; config response not confirmed"
            }
            ProbeResult::Answered { confirmed: true } if col.info.is_vendor() => {
                "config response confirmed; candidate for wired RGB"
            }
            ProbeResult::Answered { confirmed: true } => {
                "config response confirmed; excluded because usage page is not vendor-defined"
            }
        };
        println!(
            "VID:PID 258A:010C | interface {} | usage {:04X}:{:04X} | {}",
            col.info.interface, col.info.usage_page, col.info.usage, status
        );
    }
    if !found {
        println!("No matching HID collections found for 258A:010C.");
    }
    println!("No lighting or settings changed. HID paths and serial numbers omitted.");
    Ok(())
}

fn apply_one(device: &mut impl RgbDevice, rgb: [u8; 3]) -> anyhow::Result<()> {
    // No calls to set_static() or ensure_per_key_mode(): both may write
    // non-volatile settings. A single volatile frame also avoids flooding HID.
    let [r, g, b] = rgb;
    let frame = Frame::solid(device.led_count(), Rgb::new(r, g, b));
    device.stream(&frame)?;
    Ok(())
}

// Start one frame immediately, then sleep between sends. Check the deadline
// again before each send so a scheduled wake can never add an extra frame.
fn hold_loop(
    duration: Option<Duration>,
    elapsed: impl Fn() -> Duration,
    mut sleep: impl FnMut(Duration),
    mut send: impl FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    loop {
        if duration.is_some_and(|max| elapsed() >= max) {
            return Ok(());
        }
        send()?;
        let wait = match duration {
            Some(max) => HOLD_INTERVAL.min(max.saturating_sub(elapsed())),
            None => HOLD_INTERVAL,
        };
        if wait.is_zero() {
            return Ok(());
        }
        sleep(wait);
    }
}

fn hold_color(rgb: [u8; 3], duration: Option<Duration>) -> anyhow::Result<()> {
    // One confirmed path and one opened control handle, never auto-reopened.
    let mut device = find_wired()?;
    device.read_config()?; // Re-confirm signature on this exact open handle.
    let [r, g, b] = rgb;
    let frame = Frame::solid(device.led_count(), Rgb::new(r, g, b));
    println!(
        "Sending temporary color at up to 5 frames/s. Ctrl+C stops; no stored settings change."
    );
    let start = Instant::now();
    hold_loop(
        duration,
        || start.elapsed(),
        thread::sleep,
        || {
            device.refresh_wired_volatile(&frame)?;
            Ok(())
        },
    )?;
    println!("Finished sending frames. Use apply off or Fn+backslash for another effect.");
    Ok(())
}

fn run(
    args: &[String],
    check: impl FnOnce() -> anyhow::Result<()>,
    diagnose: impl FnOnce() -> anyhow::Result<()>,
    apply: impl FnOnce([u8; 3]) -> anyhow::Result<()>,
    hold: impl FnOnce([u8; 3], Option<Duration>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    match parse(args).map_err(anyhow::Error::msg)? {
        Action::Help => println!("{HELP}"),
        Action::Check => check()?,
        Action::Diagnose => diagnose()?,
        Action::Preview([r, g, b]) => println!("#{r:02X}{g:02X}{b:02X} (preview only)"),
        Action::Apply(rgb) => apply(rgb)?,
        Action::Hold(rgb, duration) => hold(rgb, duration)?,
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let args: Result<Vec<_>, _> = args.into_iter().map(|arg| arg.into_string()).collect();
    let Ok(args) = args else {
        eprintln!("Arguments must be valid Unicode.");
        return ExitCode::FAILURE;
    };
    match run(
        &args,
        || {
            let device = find_wired()?;
            println!("AULA F75 (258a:010c) confirmed by USB-C. No lighting settings changed.");
            if !device.is_per_key_mode()? {
                println!("The current keyboard effect may override a temporary color.");
            }
            Ok(())
        },
        diagnose,
        |rgb| {
            apply_one(&mut find_wired()?, rgb)?;
            println!("One temporary frame sent. If the current effect repaints it, no settings will be forced.");
            Ok(())
        },
        hold_color,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("F75 USB-C: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aula_protocol::KeyPos;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn invalid_args_help_and_preview_do_not_touch_hardware() {
        for words in [
            vec![],
            vec!["--help"],
            vec!["preview", "warm", "25"],
            vec!["apply", "off", "101"],
            vec!["apply", "off", "100", "--force"],
            vec!["apply", "#1éé1"],
        ] {
            let result = run(
                &args(&words),
                || panic!("hardware accessed"),
                || panic!("hardware accessed"),
                |_| panic!("hardware accessed"),
                |_, _| panic!("hardware accessed"),
            );
            assert_eq!(result.is_ok(), words.first() != Some(&"apply"));
        }
    }

    // Regression: this test would fail if a convenience call switched to
    // static color, rewrote config, changed device rate, or streamed twice.
    struct Mock {
        sent: Vec<Frame>,
    }
    impl RgbDevice for Mock {
        fn name(&self) -> &str {
            "mock"
        }
        fn led_count(&self) -> usize {
            126
        }
        fn layout(&self) -> &[KeyPos] {
            &[]
        }
        fn max_fps(&self) -> u32 {
            21
        }
        fn ensure_per_key_mode(&mut self) -> aula_protocol::Result<bool> {
            panic!("nonvolatile config write")
        }
        fn set_static(&mut self, _: &Frame) -> aula_protocol::Result<()> {
            panic!("nonvolatile static write")
        }
        fn stream(&mut self, f: &Frame) -> aula_protocol::Result<()> {
            self.sent.push(f.clone());
            Ok(())
        }
        fn set_max_fps(&mut self, _: u32) {
            panic!("rate changed")
        }
    }

    #[test]
    fn apply_uses_one_volatile_frame_at_the_adjusted_color() {
        let mut fake = Mock { sent: vec![] };
        run(
            &args(&["apply", "#FF8002", "50"]),
            || panic!("wrong action"),
            || panic!("wrong action"),
            |rgb| apply_one(&mut fake, rgb),
            |_, _| panic!("wrong action"),
        )
        .unwrap();
        assert_eq!(fake.sent, vec![Frame::solid(126, Rgb::new(128, 64, 1))]);
    }

    #[test]
    fn check_never_applies_color() {
        run(
            &args(&["check"]),
            || Ok(()),
            || panic!("wrong action"),
            |_| panic!("check sent lighting"),
            |_, _| panic!("check streamed lighting"),
        )
        .unwrap();
    }

    #[test]
    fn diagnose_never_applies_color() {
        let called = std::cell::Cell::new(false);
        run(
            &args(&["diagnose"]),
            || panic!("wrong action"),
            || {
                called.set(true);
                Ok(())
            },
            |_| panic!("diagnose sent lighting"),
            |_, _| panic!("diagnose streamed lighting"),
        )
        .unwrap();
        assert!(called.get());
    }

    #[test]
    fn timed_hold_stops_at_deadline_without_an_extra_frame() {
        use std::cell::{Cell, RefCell};
        let time = Cell::new(Duration::ZERO);
        let sends = RefCell::new(Vec::new());
        run(
            &args(&["hold", "00FFFF", "35", "10"]),
            || panic!("wrong path"),
            || panic!("wrong path"),
            |_| panic!("wrong path"),
            |rgb, duration| {
                assert_eq!(rgb, [0, 89, 89]);
                hold_loop(
                    duration,
                    || time.get(),
                    |gap| time.set(time.get() + gap),
                    || {
                        sends.borrow_mut().push(time.get());
                        Ok(())
                    },
                )
            },
        )
        .unwrap();
        let sends = sends.into_inner();
        assert_eq!(sends.len(), 50);
        assert_eq!(sends[0], Duration::ZERO);
        assert_eq!(sends[49], Duration::from_millis(9800));
        assert_eq!(time.get(), Duration::from_secs(10));
        assert!(sends
            .windows(2)
            .all(|pair| pair[1] - pair[0] >= HOLD_INTERVAL));
    }

    #[test]
    fn hold_fails_closed_on_write_error_or_invalid_input() {
        for words in [
            ["hold", "00FFFF", "35", "0"],
            ["hold", "00FFFF", "35", "3601"],
            ["hold", "00FFFF", "101", "10"],
            ["hold", "wrong", "35", "10"],
        ] {
            assert!(parse(&args(&words)).is_err());
        }
        assert_eq!(
            parse(&args(&["hold", "00FFFF", "35"])).unwrap(),
            Action::Hold([0, 89, 89], None)
        );
        let mut writes = 0;
        let result = hold_loop(
            None,
            || Duration::ZERO,
            |_| panic!("would retry"),
            || {
                writes += 1;
                anyhow::bail!("USB disconnected")
            },
        );
        assert_eq!(writes, 1);
        assert_eq!(result.unwrap_err().to_string(), "USB disconnected");
    }
}
