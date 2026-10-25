//! The bundled `llama-server`: one process, one model, the context the user chose, bound to
//! loopback. Started on the first analysis that needs it, retired after a few idle minutes, and
//! swept at the next launch if the app was killed under it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;

use super::catalogue::ExecModelSpec;

/// The first interesting stderr lines kept for an error message.
const DIAGNOSTICS_KEPT: usize = 40;

/// Where the engine is in its life, from the outside.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Status {
    /// Not running, and nothing is wrong. The resting state.
    Off,
    /// Spawned, model loading, `/health` not answering yet.
    Starting { model_id: String },
    Ready { model_id: String },
    /// The last start or request failed. Holds a sentence for the user; the engine will be retried
    /// on the next request rather than latching forever.
    Failed { message: String },
}


static EMITTER: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Gives this module a way to tell the UI when the engine wakes, becomes ready, or dies.
///
/// Called once from `run()`'s `setup`. Without it everything still works and the status is simply
/// never pushed — the settings pane would learn it on its next read, and the editor's warm-up
/// indicator would never appear.
pub fn attach(app: tauri::AppHandle) {
    let _ = EMITTER.set(app);
}

/// The event name the frontend listens on.
/// Announces `payload` on `event` through the handle [`attach`] stored.
///
/// For [`super::executor`], the second engine slot: it has its own status and its own event, but
/// no reason to be handed an `AppHandle` of its own when this module already holds one.
pub(crate) fn emit<T: serde::Serialize + Clone>(event: &str, payload: T) {
    if let Some(app) = EMITTER.get() {
        use tauri::Emitter;
        let _ = app.emit(event, payload);
    }
}

/// Drains the child's stderr into `sink`, keeping the first [`DIAGNOSTICS_KEPT`] interesting lines.
///
/// Detached rather than awaited: the point is to have the lines *if* something goes wrong, and a
/// healthy server writes to stderr for as long as it lives. The task ends when the pipe closes,
/// which is when the process dies.
fn collect_diagnostics(stderr: tokio::process::ChildStderr, sink: Arc<Mutex<Vec<String>>>) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(mut kept) = sink.lock() else { return };
            if kept.len() >= DIAGNOSTICS_KEPT {
                // Keep reading and discarding. Stopping here would eventually fill the pipe buffer
                // and block the server on its own logging.
                continue;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                kept.push(trimmed.to_string());
            }
        }
    });
}

/// The bundled `llama-server`.
///
/// No fallback to a copy on `PATH`, unlike [`crate::datasource::jvm`]'s hunt for a system JDK. The
/// two cases are not alike: a JDK on the machine is a real runtime that will run our jar, whereas
/// picking up whatever `llama-server` a developer happens to have installed means completions come
/// from a build nobody chose, with flags this code did not write, and a bug report that cannot be
/// reproduced. When it is missing, the honest answer is that this install does not have it.
pub(crate) fn locate() -> Result<PathBuf, String> {
    let name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };

    // A packaged app. `resource_dir()` is `None` under `cargo test` and in some dev runs, which is
    // why the source-checkout path below exists rather than this being the only branch.
    if let Some(resources) = crate::paths::resource_dir() {
        let bundled = resources.join("llama").join(name);
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    // A source checkout, where `scripts/build-llama-runtime.mjs` writes into `src-tauri/resources`
    // and nothing has bundled it yet. Mirrors how the IRIS resources are found in dev.
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("llama").join(name);
    if checkout.is_file() {
        return Ok(checkout);
    }

    Err(format!(
        "AIMeetings ships a local model engine and this install doesn't have it (expected \
         `llama/{name}` in the app resources). Reinstalling restores it; in a source checkout, run \
         `pnpm llama:runtime`."
    ))
}

/// Whether this install actually has the engine binary.
///
/// Answered before the user is offered anything, so a partial install says "this copy of CodeFlow
/// is missing its completion engine" instead of letting them download four gigabytes of weights and
/// only then discovering there is nothing to run them with.
pub fn is_available() -> bool {
    locate().is_ok()
}

/// What `llama-server --list-devices` prints: every device this build can offload to, with the
/// memory it reports — on a Mac, the share of unified memory Metal lets it wire, which is the
/// figure its `--fit` places layers against. Measured on an M4: 51 ms, nothing loaded.
///
/// `None` when the engine is missing or has not answered within a few seconds (a GPU driver that
/// hangs on enumeration must not hang the settings pane with it). Blocking: call it off the async
/// runtime.
#[cfg_attr(not(all(target_os = "macos", target_arch = "aarch64")), allow(dead_code))]
pub fn list_devices() -> Option<String> {
    use std::io::Read;
    let binary = locate().ok()?;
    let mut child = crate::proc::std_command(&binary)
        .arg("--list-devices")
        .stdin(std::process::Stdio::null())
        // A few lines; the logs go to stderr, which is dropped so it can never fill a pipe.
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut listing = String::new();
    child.stdout.take()?.read_to_string(&mut listing).ok()?;
    Some(listing)
}

/// An unused loopback port.
///
/// Asked of the OS rather than picked from a range, and *not* delegated to the server: llama-server
/// has no ephemeral-port mode — `--port 0` was tried against b10587 and the process comes up
/// listening on nothing at all — so the port has to be chosen here and handed over.
///
/// The listener is bound and dropped, which leaves a window in which something else could take the
/// port before the server binds it. That race is real and is handled where it shows up: a server
/// that cannot bind exits immediately, `await_ready` notices within a poll, and the next completion
/// request starts a fresh one on a fresh port.
pub(crate) fn free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Couldn't find a free local port for the completion engine: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Couldn't read back the local port: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// How long the executor may sit idle before it is retired.
///
/// The same five minutes as the completion engine, measured from the end of the last request
/// rather than its start (see [`InFlight`]). Between two tasks of one run the gap is seconds; after
/// the run, the memory is given back by the run itself when "free memory when done" is on, and by
/// this timer when it is off.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

const REAP_INTERVAL: Duration = Duration::from_secs(30);

/// How long [`ensure_ready`] will wait for `/health`. The same three minutes as the completion
/// slot: a 20 GB model on a cold disk is exactly the case that budget was written for.
const START_BUDGET: Duration = Duration::from_secs(180);

const HEALTH_POLL: Duration = Duration::from_millis(250);

/// The event the frontend listens on for this slot. Same payload shape as the completion engine's.
pub const STATUS_EVENT: &str = "llm:server";

static ENGINE: OnceLock<AsyncMutex<Option<Arc<ExecEngine>>>> = OnceLock::new();
static STATUS: OnceLock<Mutex<Status>> = OnceLock::new();

fn status_cell() -> &'static Mutex<Status> {
    STATUS.get_or_init(|| Mutex::new(Status::Off))
}

pub fn status() -> Status {
    status_cell().lock().map(|s| s.clone()).unwrap_or(Status::Off)
}

fn set_status(next: Status) {
    let changed = match status_cell().lock() {
        Ok(mut slot) => {
            let moved = *slot != next;
            *slot = next.clone();
            moved
        }
        Err(_) => false,
    };
    if changed {
        emit(STATUS_EVENT, next);
    }
}

/// What a running executor was launched with. Two launches with different values are two
/// different processes — llama-server cannot change its model or context in place.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Launch {
    pub model_id: String,
    pub ctx: u32,
    /// `--reasoning-budget 0`: thinking models answer without a thinking phase. Spends nothing on
    /// models that have none.
    pub no_reasoning: bool,
    /// `--n-cpu-moe N`: the first N MoE layers keep their experts in RAM so the rest fits a discrete
    /// card. 0 leaves placement to llama.cpp's `--fit`.
    pub cpu_moe_layers: u32,
}

/// How long a request for another model waits for the running one's requests to finish.
const SWAP_WAIT: Duration = Duration::from_secs(15 * 60);

pub struct ExecEngine {
    pub launch: Launch,
    pub port: u16,
    client: reqwest::Client,
    child: Mutex<tokio::process::Child>,
    alive: Arc<AtomicBool>,
    last_used: Arc<AtomicU64>,
    in_flight: Arc<AtomicUsize>,
    diagnostics: Arc<Mutex<Vec<String>>>,
}

/// Held for the length of one request. While any is held the reaper leaves the engine alone; when
/// the last one drops, the idle clock starts from *then*.
pub struct InFlight {
    count: Arc<AtomicUsize>,
    last_used: Arc<AtomicU64>,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.last_used.store(now_millis(), Ordering::Relaxed);
        self.count.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ExecEngine {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// The server's process id — what the bench reads its resident memory from.
    pub fn pid(&self) -> Option<u32> {
        self.child.lock().ok().and_then(|child| child.id())
    }

    /// Marks a request as started. Keep the guard alive until the response has been read.
    pub fn begin(&self) -> InFlight {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        self.last_used.store(now_millis(), Ordering::Relaxed);
        InFlight { count: self.in_flight.clone(), last_used: self.last_used.clone() }
    }
}

/// The running executor for `launch`, starting (and waiting for) one when there is not a matching
/// one already.
///
/// Waits for the lock rather than declining it: the only other holder is another task's start,
/// and a task behind it wants the same answer after the same wait.
pub async fn ensure_ready(entry: &ExecModelSpec, model_path: PathBuf, launch: Launch) -> Result<Arc<ExecEngine>, String> {
    if !super::models::complete(&entry.spec, &model_path) {
        return Err(format!(
            "{} isn't downloaded, or its download is incomplete. Download it under Models › Analysis.",
            entry.spec.label
        ));
    }
    let slot = ENGINE.get_or_init(|| AsyncMutex::new(None));
    // A different launch (another model, window or reasoning setting) is another process, and the
    // slot holds one. While the running one still has requests in flight it is not stopped under
    // them: this waits for them to finish — another job's whole run, when that job holds its
    // session — and gives up after `SWAP_WAIT` rather than waiting forever on a stuck one.
    let waited_since = std::time::Instant::now();
    let mut guard = loop {
        let guard = slot.lock().await;
        match guard.as_ref() {
            Some(existing) if existing.is_alive() && existing.launch == launch => {
                existing.last_used.store(now_millis(), Ordering::Relaxed);
                return Ok(existing.clone());
            }
            Some(existing) if existing.is_alive() && existing.in_flight.load(Ordering::SeqCst) > 0 => {
                if waited_since.elapsed() > SWAP_WAIT {
                    return Err(format!(
                        "The local engine is still busy with {} for another task. Try again when it finishes.",
                        existing.launch.model_id
                    ));
                }
                drop(guard);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            _ => break guard,
        }
    };

    if guard.is_some() {
        stop_locked(&mut guard).await;
    }

    set_status(Status::Starting { model_id: launch.model_id.clone() });
    match ExecEngine::spawn(entry, model_path, launch.clone()).await {
        Ok(engine) => {
            let engine = Arc::new(engine);
            *guard = Some(engine.clone());
            set_status(Status::Ready { model_id: launch.model_id });
            spawn_reaper();
            Ok(engine)
        }
        Err(message) => {
            set_status(Status::Failed { message: message.clone() });
            Err(message)
        }
    }
}

/// Stops the executor if one is running. Idempotent.
pub async fn shutdown() {
    let slot = ENGINE.get_or_init(|| AsyncMutex::new(None));
    let mut guard = slot.lock().await;
    stop_locked(&mut guard).await;
    set_status(Status::Off);
}

/// Stops the executor unless a request is in flight — battery saving at Máximo (`crate::power`)
/// never cuts off a task the user started.
pub async fn shutdown_if_idle() {
    let slot = ENGINE.get_or_init(|| AsyncMutex::new(None));
    let mut guard = slot.lock().await;
    if guard.as_ref().is_some_and(|engine| engine.in_flight.load(Ordering::SeqCst) > 0) {
        return;
    }
    stop_locked(&mut guard).await;
    set_status(Status::Off);
}

/// The model the running executor has open and the memory it holds — weights, its window's KV
/// cache and the working buffers (`hybrid::budget::memory_need`) — for the fit estimate of a model
/// that would run *beside* it (`hybrid::runtime::resident_beside`). `None` when it is not running.
/// While another task is starting it (the slot's lock is held), the weights alone: the window being
/// started is not known yet.
pub fn resident() -> Option<(String, u64)> {
    if let Some(Ok(guard)) = ENGINE.get().map(|slot| slot.try_lock()) {
        let engine = guard.as_ref().filter(|engine| engine.is_alive())?;
        let entry = super::catalogue::find(&engine.launch.model_id)?;
        let bytes = crate::hardware::budget::memory_need(entry.spec.size_bytes, entry.kv_bytes_per_token, engine.launch.ctx);
        return Some((engine.launch.model_id.clone(), bytes));
    }
    let model_id = running_model()?;
    let bytes = super::catalogue::find(&model_id)?.spec.size_bytes;
    Some((model_id, bytes))
}

/// The model the running executor has open, if any — so deleting that model's file can stop it
/// first (a mapped file cannot be unlinked on Windows).
pub fn running_model() -> Option<String> {
    match status() {
        Status::Ready { model_id } | Status::Starting { model_id } => Some(model_id),
        _ => None,
    }
}

async fn stop_locked(guard: &mut Option<Arc<ExecEngine>>) {
    let Some(engine) = guard.take() else { return };
    engine.alive.store(false, Ordering::SeqCst);
    if let Ok(mut child) = engine.child.lock() {
                let _ = child.start_kill();
    }
    clear_pidfile();
}

impl ExecEngine {
    async fn spawn(entry: &ExecModelSpec, model_path: PathBuf, launch: Launch) -> Result<Self, String> {
        let binary = locate()?;
        let port = free_port()?;

        let mut command = crate::proc::command(&binary);
        command
            .arg("-m")
            .arg(&model_path)
            // Loopback only, for the reason the completion engine gives: no authentication, and the
            // prompts are the user's source.
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-c")
            .arg(launch.ctx.to_string())
            // One slot, so the whole context belongs to the one request in flight. With the default
            // (`-1`, automatic) llama-server may split the context between several slots, and the
            // budget every task was cut to would silently stop being true.
            .arg("-np")
            .arg("1")
            // No prompt cache in RAM, said rather than implied. b10587 keeps up to 8 GB of finished
            // prompts' state by default (`--cache-ram 8192`): with several slots every request
            // leaves its state there, which is how the embedder grew to ~9 GB (`super::embedder`).
            // One slot did not grow that way, but nothing here ever asks for a prompt it already
            // finished — a step's prefix is reused from the slot's own cache, not this one — so the
            // gigabytes would buy nothing beside a model that already takes most of the memory.
            .arg("--cache-ram")
            .arg("0")
            // No `-ngl`. llama.cpp's `--fit` (on by default) places as many layers as the GPU's free
            // memory holds and keeps the rest on the CPU — mixture experts first — but only for
            // the arguments left unset: given `-ngl 99`, b10587 gives up the moment a model does
            // not fit ("n_gpu_layers already set by user to 99, abort") and puts every layer on the
            // GPU anyway, which on a card smaller than the model fails to allocate or pages VRAM
            // through system memory. Measured on an M4 with the 7B: unset, it still offloads 29/29
            // layers when they fit (fitting took 0.11 s); with 9 GB held back, 18/29, the rest on
            // the CPU.
            .arg("--no-webui");
        if launch.no_reasoning {
            command.arg("--reasoning-budget").arg("0");
        }
        if launch.cpu_moe_layers > 0 {
            command.arg("--n-cpu-moe").arg(launch.cpu_moe_layers.to_string());
        }
        let mut child = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("AIMeetings couldn't start its local model ({}): {e}", binary.display()))?;

        let diagnostics: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        if let Some(stderr) = child.stderr.take() {
            collect_diagnostics(stderr, diagnostics.clone());
        }
        write_pidfile(child.id(), port);

        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| format!("Couldn't create the local model HTTP client: {e}"))?;

        let engine = Self {
            launch,
            port,
            client,
            child: Mutex::new(child),
            alive: Arc::new(AtomicBool::new(true)),
            last_used: Arc::new(AtomicU64::new(now_millis())),
            in_flight: Arc::new(AtomicUsize::new(0)),
            diagnostics,
        };
        engine.await_ready(entry).await?;
        Ok(engine)
    }

    async fn await_ready(&self, entry: &ExecModelSpec) -> Result<(), String> {
        let url = format!("{}/health", self.base_url());
        let deadline = std::time::Instant::now() + START_BUDGET;
        while std::time::Instant::now() < deadline {
            if let Ok(mut child) = self.child.lock() {
                if let Ok(Some(exit)) = child.try_wait() {
                    self.alive.store(false, Ordering::SeqCst);
                    return Err(format!(
                        "The local model stopped immediately ({exit}). {}",
                        self.diagnostic_tail()
                    ));
                }
            }
            if let Ok(response) = self.client.get(&url).timeout(Duration::from_secs(2)).send().await {
                if response.status().is_success() {
                    return Ok(());
                }
            }
            tokio::time::sleep(HEALTH_POLL).await;
        }
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
                        let _ = child.start_kill();
        }
        Err(format!(
            "{} didn't finish loading within {} seconds. {}",
            entry.spec.label,
            START_BUDGET.as_secs(),
            self.diagnostic_tail()
        ))
    }

    fn diagnostic_tail(&self) -> String {
        let Ok(lines) = self.diagnostics.lock() else { return String::new() };
        said(&lines)
    }
}

/// What llama-server said before it stopped: its error lines when it printed any — the start-up chatter
/// around them (verbosity, CORS warnings) buried the one that mattered in Codi's window — or all of it.
fn said(lines: &[String]) -> String {
    let errors: Vec<&str> = lines.iter().map(String::as_str).filter(|line| is_error_line(line)).collect();
    if !errors.is_empty() {
        return format!("It said: {}", errors.join(" / "));
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("It said: {}", lines.join(" / "))
}

/// `0.00.077.594 E llama_model_load: …` — llama.cpp's level letter — or an `error:` without one.
fn is_error_line(line: &str) -> bool {
    line.split_whitespace().nth(1) == Some("E") || line.to_ascii_lowercase().contains("error:")
}

/// Retires the executor after [`IDLE_TIMEOUT`] with nothing in flight.
fn spawn_reaper() {
    static ARMED: OnceLock<()> = OnceLock::new();
    if ARMED.set(()).is_err() {
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(REAP_INTERVAL).await;
            let slot = ENGINE.get_or_init(|| AsyncMutex::new(None));
            let mut guard = slot.lock().await;
            let idle = match guard.as_ref() {
                None => continue,
                // A request in flight is never idle, however long ago it started.
                Some(engine) if engine.in_flight.load(Ordering::SeqCst) > 0 => continue,
                Some(engine) => now_millis().saturating_sub(engine.last_used.load(Ordering::Relaxed)),
            };
            if idle >= crate::power::idle_limit(IDLE_TIMEOUT).as_millis() as u64 {
                stop_locked(&mut guard).await;
                set_status(Status::Off);
            }
        }
    });
}

fn pidfile() -> PathBuf {
    crate::paths::state_dir().join(".llama-server.pid")
}

fn write_pidfile(pid: Option<u32>, port: u16) {
    let Some(pid) = pid else { return };
    let _ = std::fs::write(pidfile(), format!("{pid}\n{port}\n"));
}

fn clear_pidfile() {
    let _ = std::fs::remove_file(pidfile());
}

/// Kills an executor left behind by a previous run of the app. Called once at startup, beside the
/// completion engine's own sweep.
pub fn sweep_stale() {
    sweep_pidfile(pidfile());
}


/// Every `llama-server` of ours whose parent is gone — what the pidfiles cannot cover: each slot
/// remembers only its last process, and a process that never runs its destructors (a test binary
/// that ends with the embedder in a static, a dev build killed on every rebuild) leaves one behind
/// each time. Found 2026-10-22: five embedders from five runs of a live test, alive for ten hours.
///
/// Ours means its executable is the binary this build starts ([`locate`]) or the source checkout's
/// copy, which tests and dev runs start; orphaned means its parent is launchd (pid 1) or no longer
/// exists. A server another running CodeFlow owns has that instance as its parent, so it is left
/// alone. Lists every process once, so it runs off the startup path.
pub fn sweep_orphans() {
    let ours = server_binaries();
    if ours.is_empty() {
        return;
    }
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    let mut killed = 0;
    for process in system.processes().values() {
        if !process.exe().is_some_and(|exe| ours.iter().any(|own| own == exe)) {
            continue;
        }
        let orphaned = process.parent().is_none_or(|parent| parent.as_u32() == 1 || system.process(parent).is_none());
        if orphaned && process.kill() {
            killed += 1;
        }
    }
    if killed > 0 {
        crate::applog::info(&format!("llm: stopped {killed} llama-server process(es) a previous run left behind"));
    }
}

/// The `llama-server` executables that are ours: the one this build starts ([`locate`]) and the
/// source checkout's copy, which tests and dev runs start.
pub fn server_binaries() -> Vec<PathBuf> {
    let name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let mut ours: Vec<PathBuf> = locate().into_iter().collect();
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("llama").join(name);
    if checkout.is_file() && !ours.contains(&checkout) {
        ours.push(checkout);
    }
    ours
}

/// The sweep itself, for any pidfile this module's engines write.
///
/// [`super::executor`] keeps its own pidfile — two slots, two processes — and goes through here so
/// the identity check below exists once.
pub(super) fn sweep_pidfile(path: PathBuf) {
    let Ok(contents) = std::fs::read_to_string(&path) else { return };
    let _ = std::fs::remove_file(&path);

    let Some(pid) = contents.lines().next().and_then(|line| line.trim().parse::<u32>().ok()) else {
        return;
    };

    let Ok(expected) = locate() else { return };

    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut system = System::new();
    let pid = Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always),
    );
    let Some(process) = system.process(pid) else { return };

    // The identity check that makes this safe. A pid that has been reused names some other
    // program, and its executable will not be our bundled binary.
    let same = process.exe().is_some_and(|exe| exe == expected);
    if same {
        process.kill();
    }
}

pub(super) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_start_says_its_error_lines_only() {
        let lines: Vec<String> = [
            "0.00.044.154 I cmn common_param: common_params_print_info: verbosity = 3",
            "0.00.044.466 W srv llama_server: CORS is set to allow all origins ('*')",
            "0.00.045.863 I srv load_model: loading model '/models/m.gguf'",
            "0.00.077.594 E llama_model_load: error loading model: tensor 'output.weight' data is not within the file bounds, model is corrupted or incomplete",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            said(&lines),
            "It said: 0.00.077.594 E llama_model_load: error loading model: tensor 'output.weight' data is not within the file bounds, model is corrupted or incomplete"
        );
        assert_eq!(said(&lines[..2]), format!("It said: {} / {}", lines[0], lines[1]), "no error line: all of it");
        assert_eq!(said(&[]), "");
    }

    #[test]
    fn launches_compare_by_every_field_that_needs_a_new_process() {
        let a = Launch { model_id: "m".into(), ctx: 16_384, no_reasoning: true, cpu_moe_layers: 0 };
        assert_eq!(a, a.clone());
        assert_ne!(a, Launch { ctx: 32_768, ..a.clone() });
        assert_ne!(a, Launch { model_id: "other".into(), ..a.clone() });
        assert_ne!(a, Launch { no_reasoning: false, ..a.clone() });
        assert_ne!(a, Launch { cpu_moe_layers: 4, ..a.clone() });
    }

    #[test]
    fn in_flight_guards_count_and_stamp_on_release() {
        let count = Arc::new(AtomicUsize::new(1));
        let last_used = Arc::new(AtomicU64::new(0));
        drop(InFlight { count: count.clone(), last_used: last_used.clone() });
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert!(last_used.load(Ordering::Relaxed) > 0, "the idle clock starts when the request ends");
    }

    #[tokio::test]
    async fn a_missing_model_file_is_refused_before_anything_spawns() {
        let entry = super::super::catalogue::find(super::super::catalogue::DEFAULT_EXEC_MODEL_ID).unwrap();
        let launch = Launch { model_id: entry.spec.id.into(), ctx: 8_192, no_reasoning: true, cpu_moe_layers: 0 };
        let error = ensure_ready(entry, PathBuf::from("/nonexistent/model.gguf"), launch)
            .await
            .err()
            .expect("must refuse");
        assert!(error.contains("isn't downloaded"), "{error}");
    }
}
