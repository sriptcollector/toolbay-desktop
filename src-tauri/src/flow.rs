//! Toolbay Flow: hold a hotkey, talk, release, and the text is pasted into
//! the focused app. Fully local: cpal records the mic, whisper.cpp
//! (whisper-cli) transcribes, and the clipboard + Ctrl/Cmd+V pastes, with the
//! old clipboard restored afterwards. See docs/flow-plan.md for what's next.
//!
//! Prototype limits (by design, for now):
//! - Hotkey is Ctrl+Shift+Space via RegisterHotKey (global-shortcut plugin).
//!   Wispr's Ctrl+Win needs a low-level keyboard hook; that's the next step.
//! - whisper-cli is a downloaded sidecar (ggml-org/whisper.cpp release), not linked in.
//! - On macOS there is no prebuilt whisper-cli; it uses one on PATH (brew install whisper-cpp).
use crate::{installer, store};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const HOTKEY: &str = "Ctrl+Shift+Space";
const WHISPER_ZIP: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";
const MODEL_FILE: &str = "ggml-base.en.bin";

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlowStatus {
    pub state: String, // idle | recording | transcribing | error | unavailable | off
    pub model_ready: bool,
    pub model_path: Option<String>,
    pub hotkey: String,
    pub last_text: Option<String>,
    pub message: Option<String>,
}

struct Recording {
    stop: mpsc::Sender<()>,
    done: mpsc::Receiver<Result<(Vec<f32>, u32), String>>,
}

#[derive(Default)]
pub struct Flow {
    inner: Mutex<FlowInner>,
}

#[derive(Default)]
struct FlowInner {
    enabled: bool,
    busy: bool,
    recording: Option<Recording>,
    last_text: Option<String>,
    message: Option<String>,
    state: String,
}

fn flow_dir(app: &AppHandle) -> PathBuf {
    let d = store::data_dir(app).join("flow");
    std::fs::create_dir_all(&d).ok();
    d
}

fn model_path(app: &AppHandle) -> PathBuf {
    flow_dir(app).join("models").join(MODEL_FILE)
}

fn whisper_bin(app: &AppHandle) -> Option<PathBuf> {
    let name = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };
    let local = flow_dir(app).join("bin").join("Release").join(name);
    if local.exists() {
        return Some(local);
    }
    // Fallback: a whisper-cli on PATH (the macOS route).
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|p| p.join(name)).find(|p| p.exists())
    })
}

fn status(app: &AppHandle) -> FlowStatus {
    let flow = app.state::<Flow>();
    let inner = flow.inner.lock().unwrap();
    let model = model_path(app);
    let ready = model.exists() && whisper_bin(app).is_some();
    FlowStatus {
        state: if !inner.enabled {
            "off".into()
        } else if inner.state.is_empty() {
            "idle".into()
        } else {
            inner.state.clone()
        },
        model_ready: ready,
        model_path: model.exists().then(|| model.to_string_lossy().into()),
        hotkey: HOTKEY.into(),
        last_text: inner.last_text.clone(),
        message: inner.message.clone(),
    }
}

fn set_state(app: &AppHandle, state: &str, message: Option<String>) {
    {
        let flow = app.state::<Flow>();
        let mut inner = flow.inner.lock().unwrap();
        inner.state = state.into();
        inner.message = message;
    }
    let s = status(app);
    app.emit("flow-status", &s).ok();
}

#[tauri::command]
pub fn flow_status(app: AppHandle) -> FlowStatus {
    status(&app)
}

#[tauri::command]
pub fn flow_enable(app: AppHandle, enabled: bool) -> Result<FlowStatus, String> {
    let shortcut: Shortcut = HOTKEY.parse().map_err(|e| format!("{e:?}"))?;
    let gs = app.global_shortcut();
    if enabled {
        if !gs.is_registered(shortcut) {
            gs.register(shortcut).map_err(|e| format!("Could not claim {HOTKEY}: {e}"))?;
        }
    } else if gs.is_registered(shortcut) {
        gs.unregister(shortcut).map_err(|e| e.to_string())?;
    }
    app.state::<Flow>().inner.lock().unwrap().enabled = enabled;
    store::settings_set(app.clone(), "flowEnabled".into(), enabled.to_string())?;
    set_state(&app, "idle", None);
    Ok(status(&app))
}

/// Download whisper-cli (Windows) and the base.en model (~148 MB).
#[tauri::command]
pub async fn flow_download_model(app: AppHandle) -> Result<FlowStatus, String> {
    let dir = flow_dir(&app);
    if cfg!(windows) && whisper_bin(&app).is_none() {
        let zip = dir.join("whisper-bin-x64.zip");
        installer::download(&app, "toolbay-flow", WHISPER_ZIP, &zip).await?;
        let bin = dir.join("bin");
        let z = zip.clone();
        tokio::task::spawn_blocking(move || installer::unzip(&z, &bin))
            .await
            .map_err(|e| e.to_string())??;
        std::fs::remove_file(zip).ok();
    }
    let model = model_path(&app);
    if !model.exists() {
        std::fs::create_dir_all(model.parent().unwrap()).ok();
        let part = model.with_extension("part");
        installer::download(&app, "toolbay-flow", MODEL_URL, &part).await?;
        std::fs::rename(&part, &model).map_err(|e| e.to_string())?;
    }
    app.emit("install-progress", serde_json::json!({"slug":"toolbay-flow","stage":"done","received":0,"total":null})).ok();
    let s = status(&app);
    app.emit("flow-status", &s).ok();
    Ok(s)
}

/// Global-shortcut handler: press starts recording, release stops and transcribes.
pub fn on_shortcut(app: &AppHandle, state: ShortcutState) {
    match state {
        ShortcutState::Pressed => start(app),
        ShortcutState::Released => stop(app),
    }
}

fn start(app: &AppHandle) {
    let flow = app.state::<Flow>();
    let mut inner = flow.inner.lock().unwrap();
    if !inner.enabled || inner.busy || inner.recording.is_some() {
        return; // key auto-repeat sends repeated Pressed events
    }
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel();
    // cpal streams are not Send on every platform, so the stream lives on its own thread.
    std::thread::spawn(move || {
        let _ = done_tx.send(record_until(stop_rx));
    });
    inner.recording = Some(Recording { stop: stop_tx, done: done_rx });
    drop(inner);
    set_state(app, "recording", None);
}

fn record_until(stop: mpsc::Receiver<()>) -> Result<(Vec<f32>, u32), String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("No microphone found.")?;
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let buf: Arc<Mutex<Vec<f32>>> = Arc::default();
    let sink = buf.clone();
    let err_fn = |e| eprintln!("flow: audio error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| push_mono(&sink, data.iter().copied(), channels),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| push_mono(&sink, data.iter().map(|s| *s as f32 / 32768.0), channels),
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &_| {
                push_mono(&sink, data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0), channels)
            },
            err_fn,
            None,
        ),
        f => return Err(format!("Unsupported mic format {f:?}")),
    }
    .map_err(|e| format!("Could not open the microphone: {e}"))?;
    stream.play().map_err(|e| e.to_string())?;
    // Hard cap of 5 minutes per dictation.
    let _ = stop.recv_timeout(Duration::from_secs(300));
    drop(stream);
    let samples = std::mem::take(&mut *buf.lock().unwrap());
    Ok((samples, rate))
}

fn push_mono(buf: &Arc<Mutex<Vec<f32>>>, data: impl Iterator<Item = f32>, channels: usize) {
    let mut b = buf.lock().unwrap();
    let mut acc = 0.0;
    for (i, s) in data.enumerate() {
        acc += s;
        if (i + 1) % channels == 0 {
            b.push(acc / channels as f32);
            acc = 0.0;
        }
    }
}

fn stop(app: &AppHandle) {
    let rec = {
        let flow = app.state::<Flow>();
        let mut inner = flow.inner.lock().unwrap();
        let Some(rec) = inner.recording.take() else { return };
        inner.busy = true;
        rec
    };
    let _ = rec.stop.send(());
    let app = app.clone();
    std::thread::spawn(move || {
        set_state(&app, "transcribing", None);
        let result = rec
            .done
            .recv()
            .map_err(|_| "Recording stopped unexpectedly.".to_string())
            .and_then(|r| r)
            .and_then(|(samples, rate)| transcribe(&app, &samples, rate))
            .and_then(|text| {
                if !text.is_empty() {
                    paste(&text)?;
                }
                Ok(text)
            });
        let flow = app.state::<Flow>();
        let mut inner = flow.inner.lock().unwrap();
        inner.busy = false;
        match result {
            Ok(text) => {
                if !text.is_empty() {
                    inner.last_text = Some(text);
                }
                drop(inner);
                set_state(&app, "idle", None);
            }
            Err(e) => {
                drop(inner);
                set_state(&app, "error", Some(e));
            }
        }
    });
}

fn transcribe(app: &AppHandle, samples: &[f32], rate: u32) -> Result<String, String> {
    if samples.len() < (rate as usize) / 4 {
        return Ok(String::new()); // under 0.25 s: a tap, not speech
    }
    let model = model_path(app);
    let bin = whisper_bin(app).ok_or("Speech model not downloaded yet. Open Toolbay Flow and click Download model.")?;
    if !model.exists() {
        return Err("Speech model not downloaded yet.".into());
    }
    let pcm = resample(samples, rate, 16_000);
    let wav = std::env::temp_dir().join(format!("toolbay-flow-{}.wav", uuid::Uuid::new_v4()));
    write_wav(&wav, &pcm)?;
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("-m").arg(&model).arg("-f").arg(&wav).args(["-nt", "-np", "-l", "en", "-t", &threads.to_string()]);
    if let Some(dir) = bin.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("Could not run whisper: {e}"));
    std::fs::remove_file(&wav).ok();
    let out = out?;
    if !out.status.success() {
        return Err(format!("whisper failed: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")));
    }
    Ok(clean(&String::from_utf8_lossy(&out.stdout)))
}

/// Collapse whisper's line breaks and drop its non-speech markers like [BLANK_AUDIO].
fn clean(raw: &str) -> String {
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut depth = 0;
    for c in joined.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (input.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 * ratio;
            let j = pos as usize;
            let frac = (pos - j as f64) as f32;
            let a = input[j];
            let b = *input.get(j + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

fn write_wav(path: &Path, pcm: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for s in pcm {
        w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}

/// Clipboard + Ctrl/Cmd+V, then put the user's clipboard back.
fn paste(text: &str) -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut clip = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous = clip.get_text().ok();
    clip.set_text(format!("{text} ")).map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(40));
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    // The hotkey's modifiers may still be held; release them so the paste is a clean Ctrl+V.
    for k in [Key::Shift, Key::Control, Key::Alt, Key::Meta] {
        let _ = enigo.key(k, Direction::Release);
    }
    let modifier = if cfg!(target_os = "macos") { Key::Meta } else { Key::Control };
    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('v'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(300));
    if let Some(prev) = previous {
        let _ = clip.set_text(prev);
    }
    Ok(())
}

/// Turn Flow back on at launch if it was on last time.
pub fn restore(app: &AppHandle) {
    if store::setting(app, "flowEnabled").as_deref() == Some("true") {
        if let Err(e) = flow_enable(app.clone(), true) {
            set_state(app, "error", Some(e));
        }
    }
}
