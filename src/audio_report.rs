//! Developer report of the audio delivery (`KINGDOMS_AUDIO_REPORT=1` or `--audio-report`):
//! prints a table of every id of `assets/audio/manifest.ron` with its channel, the expected
//! file, the file found (by stem, any supported format), its decoded length, format and peak
//! level and hints (missing, unreadable, clipping, silence at the ends of loops, unusual
//! length), then the voice lines with the exact words to speak, and exits – no window, no
//! audio device. Exit code 0 if every file is there and decodes, else 1.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use kingdoms_engine::audio::files::{AuditRow, audit};
use kingdoms_engine::audio::{AudioChannel, AudioManifest};

/// Environment variable that asks for the report.
pub const ENV_VAR: &str = "KINGDOMS_AUDIO_REPORT";
/// Command line flag that asks for the report.
pub const FLAG: &str = "--audio-report";

/// True if the report was asked for (environment variable set to anything but `0`, or the
/// flag among `args`).
pub fn requested(env: Option<&str>, args: &[String]) -> bool {
    env.is_some_and(|v| !v.is_empty() && v != "0") || args.iter().any(|a| a == FLAG)
}

/// Prints the report for the audio folder `audio_root` and returns the exit code.
pub fn run(audio_root: &Path) -> ExitCode {
    let manifest_path = audio_root.join("manifest.ron");
    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("Audio-Manifest nicht lesbar: {} ({err})", manifest_path.display());
            return ExitCode::FAILURE;
        }
    };
    let manifest: AudioManifest = match ron::from_str(&text) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("Audio-Manifest fehlerhaft: {} ({err})", manifest_path.display());
            return ExitCode::FAILURE;
        }
    };
    let rows = audit(audio_root, &manifest);
    print!("{}", render(audio_root, &manifest, &rows));
    if rows.iter().all(AuditRow::ok) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// The whole report as text.
pub fn render(audio_root: &Path, manifest: &AudioManifest, rows: &[AuditRow]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "KINGDOMS – Audio-Lieferung in {}", audio_root.display());
    let _ = writeln!(out, "Formate: .ogg, .mp3, .wav, .flac – gesucht wird nach dem Dateinamen ohne Endung (Groß-/Kleinschreibung, Leer- und Sonderzeichen egal).");
    let _ = writeln!(out);
    let header = ["ID", "Kanal", "Erwartet", "Gefunden", "Dauer", "Format", "Spitze", "Hinweis"];
    let table: Vec<[String; 8]> = rows.iter().map(row_cells).collect();
    let mut widths = header.map(|h| h.chars().count());
    for cells in &table {
        for (w, c) in widths.iter_mut().zip(cells) {
            *w = (*w).max(c.chars().count());
        }
    }
    let line = |cells: &[String]| {
        let mut s = String::new();
        for (i, (c, w)) in cells.iter().zip(widths).enumerate() {
            if i + 1 == cells.len() {
                s.push_str(c);
            } else {
                let _ = write!(s, "{c}{}  ", " ".repeat(w - c.chars().count()));
            }
        }
        s.trim_end().to_owned()
    };
    let _ = writeln!(out, "{}", line(&header.map(str::to_owned)));
    let _ = writeln!(out, "{}", "-".repeat(widths.iter().sum::<usize>() + 2 * (widths.len() - 1)));
    for cells in &table {
        let _ = writeln!(out, "{}", line(cells));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", summary(manifest, rows));
    let voices: Vec<(String, &str)> = manifest
        .sounds
        .iter()
        .filter(|(_, s)| s.channel == AudioChannel::Voice)
        .flat_map(|(_, s)| s.files.iter().enumerate().map(move |(i, f)| (f.clone(), s.text.get(i).or_else(|| s.text.first()).map_or("", String::as_str))))
        .collect();
    if !voices.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Sprechertexte (Datei → genauer Wortlaut):");
        for (file, text) in voices {
            let _ = writeln!(out, "  {}  →  {text}", stem_of(&file));
        }
    }
    out
}

/// One table line of a file.
fn row_cells(r: &AuditRow) -> [String; 8] {
    let id = if r.variation > 0 { format!("{} #{}", r.id, r.variation) } else { r.id.clone() };
    let found = r.found.clone().unwrap_or_else(|| "–".to_owned());
    let (duration, format, peak) = match &r.decoded {
        Some(Ok(info)) => (
            duration_text(info.duration),
            format!("{} kHz {}", decimal(f64::from(info.sample_rate) / 1000.0, 1), if info.channels == 1 { "Mono" } else { "Stereo" }),
            if info.peak_dbfs().is_finite() { format!("{} dBFS", decimal(f64::from(info.peak_dbfs()), 1)) } else { "Stille".to_owned() },
        ),
        _ => ("–".to_owned(), "–".to_owned(), "–".to_owned()),
    };
    [id, channel_name(r.channel).to_owned(), r.expected.clone(), found, duration, format, peak, hints(r).join("; ")]
}

/// German channel name of the report.
fn channel_name(channel: AudioChannel) -> &'static str {
    channel.display_name()
}

/// What deserves a look about one file.
pub fn hints(r: &AuditRow) -> Vec<String> {
    let mut out = Vec::new();
    let info = match &r.decoded {
        None => return vec!["fehlt".to_owned()],
        Some(Err(err)) => return vec![format!("nicht lesbar: {err}")],
        Some(Ok(info)) => info,
    };
    if info.peak >= 0.999 {
        out.push("übersteuert (Spitze bei 0 dBFS)".to_owned());
    }
    let seconds = info.duration.as_secs_f32();
    if r.loops {
        let gap = Duration::from_millis(50);
        if info.leading_silence > gap {
            out.push(format!("Stille am Anfang {} s – Schleife mit Lücke", decimal(f64::from(info.leading_silence.as_secs_f32()), 2)));
        }
        if info.trailing_silence > gap {
            out.push(format!("Stille am Ende {} s – Schleife mit Lücke", decimal(f64::from(info.trailing_silence.as_secs_f32()), 2)));
        }
        if r.found.as_deref().is_some_and(|f| f.to_lowercase().ends_with(".mp3")) {
            out.push("MP3-Schleife: auf Lücke prüfen (OGG/WAV empfohlen)".to_owned());
        }
    }
    match r.channel {
        AudioChannel::Music if seconds < 60.0 => out.push("kurz für ein Musikstück (empfohlen 2–4 min)".to_owned()),
        AudioChannel::Ambience if seconds < 10.0 => out.push("kurze Schleife (empfohlen 30–90 s)".to_owned()),
        AudioChannel::Effects | AudioChannel::Ui if seconds > 8.0 => out.push("lang für einen Effekt".to_owned()),
        AudioChannel::Voice if seconds > 8.0 => out.push("lang für eine Ansage".to_owned()),
        _ => {}
    }
    out
}

/// Totals of the report.
pub fn summary(manifest: &AudioManifest, rows: &[AuditRow]) -> String {
    let ids = manifest.music.len() + manifest.sounds.len();
    let found = rows.iter().filter(|r| r.found.is_some()).count();
    let renamed = rows.iter().filter(|r| r.found.as_deref().is_some_and(|f| f != r.expected)).count();
    let missing = rows.iter().filter(|r| r.found.is_none()).count();
    let broken = rows.iter().filter(|r| matches!(r.decoded, Some(Err(_)))).count();
    let complete = rows.iter().filter(|r| r.ok()).count();
    let verdict = if complete == rows.len() { "Vollständig." } else { "Unvollständig." };
    let count = crate::ui::hud::format::count;
    format!(
        "{}, {} erwartet · gefunden {found} (davon {renamed} unter anderem Namen oder Format) · fehlen {missing} · nicht lesbar {broken}. {verdict}",
        count(ids as u64, "ID", "IDs"),
        count(rows.len() as u64, "Datei", "Dateien")
    )
}

/// Length as „0,42 s“ below a minute, else „2:31“.
pub fn duration_text(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s < 60.0 {
        format!("{} s", decimal(s, 2))
    } else {
        let total = s.round() as u64;
        format!("{}:{:02}", total / 60, total % 60)
    }
}

fn decimal(v: f64, places: usize) -> String {
    crate::ui::hud::format::decimal(v, places)
}

fn stem_of(file: &str) -> String {
    Path::new(file).with_extension("").to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use kingdoms_engine::audio::files::DecodedInfo;

    use super::*;

    fn row(channel: AudioChannel, found: Option<&str>, decoded: Option<Result<DecodedInfo, String>>) -> AuditRow {
        AuditRow {
            id: "amb_rain".into(),
            channel,
            variation: 0,
            expected: "ambience/rain.ogg".into(),
            found: found.map(str::to_owned),
            decoded,
            loops: channel == AudioChannel::Ambience || channel == AudioChannel::Music,
        }
    }

    fn info(seconds: f32, peak: f32, lead: f32) -> DecodedInfo {
        DecodedInfo {
            duration: Duration::from_secs_f32(seconds),
            sample_rate: 44_100,
            channels: 2,
            peak,
            leading_silence: Duration::from_secs_f32(lead),
            trailing_silence: Duration::ZERO,
        }
    }

    #[test]
    fn the_report_is_asked_for_by_variable_or_flag() {
        assert!(requested(Some("1"), &[]));
        assert!(!requested(Some("0"), &[]) && !requested(None, &[]) && !requested(Some(""), &[]));
        assert!(requested(None, &["kingdoms".into(), "--audio-report".into()]));
    }

    #[test]
    fn hints_name_missing_broken_and_doubtful_files() {
        assert_eq!(hints(&row(AudioChannel::Ambience, None, None)), ["fehlt"]);
        assert_eq!(hints(&row(AudioChannel::Ambience, Some("ambience/rain.wav"), Some(Err("kaputt".into())))), ["nicht lesbar: kaputt"]);
        let fine = row(AudioChannel::Ambience, Some("ambience/rain.ogg"), Some(Ok(info(45.0, 0.7, 0.0))));
        assert!(hints(&fine).is_empty(), "{:?}", hints(&fine));
        let gap = row(AudioChannel::Ambience, Some("ambience/Rain.mp3"), Some(Ok(info(5.0, 1.0, 0.3))));
        let h = hints(&gap);
        assert!(h.iter().any(|t| t.starts_with("übersteuert")), "{h:?}");
        assert!(h.iter().any(|t| t == "Stille am Anfang 0,3 s – Schleife mit Lücke"), "{h:?}");
        assert!(h.iter().any(|t| t.starts_with("MP3-Schleife")) && h.iter().any(|t| t.starts_with("kurze Schleife")), "{h:?}");
        let long_click = row(AudioChannel::Ui, Some("ui/click.wav"), Some(Ok(info(9.0, 0.5, 0.0))));
        assert_eq!(hints(&long_click), ["lang für einen Effekt"]);
    }

    #[test]
    fn report_lines_and_totals() {
        assert_eq!(duration_text(Duration::from_millis(420)), "0,42 s");
        assert_eq!(duration_text(Duration::from_secs(151)), "2:31");
        let manifest: AudioManifest = match ron::from_str(r#"(sounds: { "amb_rain": (files: ["ambience/rain.ogg"], channel: Ambience) })"#) {
            Ok(m) => m,
            Err(e) => panic!("{e}"),
        };
        let rows = [row(AudioChannel::Ambience, Some("ambience/rain.flac"), Some(Ok(info(45.0, 0.5, 0.0))))];
        assert_eq!(
            summary(&manifest, &rows),
            "1 ID, 1 Datei erwartet · gefunden 1 (davon 1 unter anderem Namen oder Format) · fehlen 0 · nicht lesbar 0. Vollständig."
        );
        let cells = row_cells(&rows[0]);
        assert_eq!(cells[1], "Umgebung");
        assert_eq!(cells[4], "45 s");
        assert_eq!(cells[5], "44,1 kHz Stereo");
        assert_eq!(cells[6], "−6 dBFS");
        let text = render(Path::new("assets/audio"), &manifest, &rows);
        assert!(text.contains("ambience/rain.flac") && text.contains("Vollständig."), "{text}");
    }
}
