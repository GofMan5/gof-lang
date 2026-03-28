use anyhow::{Context, Result};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use crate::{RunTarget, display_path, execute_run_target, normalize_cli_path};
use gof_compiler::normalize_source_path;

const WATCH_PREFIX: &str = "[watch]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchPlan {
    watch_roots: Vec<WatchRoot>,
    source_roots: Vec<PathBuf>,
    package_roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct WatchRoot {
    path: PathBuf,
    recursive: bool,
}

#[derive(Debug, Default)]
struct RunLoopCoordinator {
    running: bool,
    dirty: bool,
}

impl RunLoopCoordinator {
    fn start_if_idle(&mut self) -> bool {
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }

    fn record_change(&mut self) {
        if self.running {
            self.dirty = true;
        } else {
            self.running = true;
        }
    }

    fn finish_run(&mut self) -> bool {
        self.running = false;
        if self.dirty {
            self.dirty = false;
            self.running = true;
            true
        } else {
            false
        }
    }
}

enum WatchMessage {
    Fs(Result<Event, notify::Error>),
    Stop,
}

pub(crate) fn run_watch_loop(
    target: RunTarget,
    program_args: Vec<String>,
    debounce_ms: u64,
) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    let stop_requested = Arc::new(AtomicBool::new(false));
    install_ctrlc_handler(sender.clone(), stop_requested.clone())?;

    let mut watcher = RecommendedWatcher::new(
        move |result| {
            let _ = sender.send(WatchMessage::Fs(result));
        },
        Config::default(),
    )
    .context("failed to start filesystem watcher for `gof run --watch`")?;

    let mut coordinator = RunLoopCoordinator::default();
    let mut active_plan = build_watch_plan(&target)?;
    apply_watch_plan(&mut watcher, None, &active_plan)?;

    eprintln!(
        "{WATCH_PREFIX} watching for changes in {} root(s)",
        active_plan.watch_roots.len()
    );
    coordinator.start_if_idle();

    loop {
        let refreshed_plan = build_watch_plan(&target)?;
        if refreshed_plan != active_plan {
            apply_watch_plan(&mut watcher, Some(&active_plan), &refreshed_plan)?;
            active_plan = refreshed_plan;
            eprintln!("{WATCH_PREFIX} watch set refreshed");
        }

        eprintln!("{WATCH_PREFIX} running");
        let run_result = execute_run_target(&target, &program_args);
        if let Err(error) = run_result {
            eprintln!("{error:#}");
            eprintln!("{WATCH_PREFIX} run failed; waiting for changes");
        }

        let stop_now = drain_pending_messages(
            &receiver,
            &active_plan,
            &mut coordinator,
            debounce_ms,
            &stop_requested,
        )?;
        if stop_now {
            eprintln!("{WATCH_PREFIX} stopped");
            break;
        }

        if coordinator.finish_run() {
            eprintln!("{WATCH_PREFIX} change detected; rerunning");
            continue;
        }

        wait_for_relevant_change(
            &receiver,
            &active_plan,
            &mut coordinator,
            debounce_ms,
            &stop_requested,
        )?;
        if stop_requested.load(Ordering::SeqCst) {
            eprintln!("{WATCH_PREFIX} stopped");
            break;
        }

        eprintln!("{WATCH_PREFIX} change detected; rerunning");
    }

    Ok(())
}

fn build_watch_plan(target: &RunTarget) -> Result<WatchPlan> {
    let mut roots = BTreeMap::<PathBuf, bool>::new();
    let mut source_roots = BTreeSet::<PathBuf>::new();
    let mut package_roots = BTreeSet::<PathBuf>::new();

    if let Some(package_root) = &target.package_root {
        let normalized_root = normalize_source_path(package_root);
        roots.insert(normalized_root.clone(), false);
        package_roots.insert(normalized_root.clone());

        let source_root = normalized_root.join("src");
        if source_root.is_dir() {
            let source_root = normalize_source_path(&source_root);
            roots.insert(source_root.clone(), true);
            source_roots.insert(source_root);
        }

        if let Ok(Some(lockfile)) =
            gof_compiler::package::ensure_fresh_lockfile_for_source(&target.entry_path)
        {
            for package in lockfile.packages {
                let package_root = if package.source.path == "." {
                    normalized_root.clone()
                } else {
                    normalize_source_path(&normalized_root.join(package.source.path))
                };
                roots.insert(package_root.clone(), false);
                package_roots.insert(package_root.clone());

                let dependency_source_root = package_root.join("src");
                if dependency_source_root.is_dir() {
                    let dependency_source_root = normalize_source_path(&dependency_source_root);
                    roots.insert(dependency_source_root.clone(), true);
                    source_roots.insert(dependency_source_root);
                }
            }
        }
    } else if let Some(parent) = target.entry_path.parent() {
        let normalized_parent = normalize_source_path(parent);
        roots.insert(normalized_parent.clone(), false);
        source_roots.insert(normalized_parent);
    }

    Ok(WatchPlan {
        watch_roots: roots
            .into_iter()
            .map(|(path, recursive)| WatchRoot { path, recursive })
            .collect(),
        source_roots: source_roots.into_iter().collect(),
        package_roots: package_roots.into_iter().collect(),
    })
}

fn apply_watch_plan(
    watcher: &mut RecommendedWatcher,
    previous: Option<&WatchPlan>,
    next: &WatchPlan,
) -> Result<()> {
    if let Some(previous) = previous {
        for root in &previous.watch_roots {
            let _ = watcher.unwatch(&root.path);
        }
    }

    for root in &next.watch_roots {
        watcher
            .watch(
                &root.path,
                if root.recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )
            .with_context(|| format!("failed to watch {}", display_path(&root.path)))?;
    }

    Ok(())
}

fn wait_for_relevant_change(
    receiver: &Receiver<WatchMessage>,
    plan: &WatchPlan,
    coordinator: &mut RunLoopCoordinator,
    debounce_ms: u64,
    stop_requested: &AtomicBool,
) -> Result<()> {
    loop {
        match receiver.recv() {
            Ok(WatchMessage::Stop) => {
                stop_requested.store(true, Ordering::SeqCst);
                return Ok(());
            }
            Ok(WatchMessage::Fs(result)) => {
                if record_event_if_relevant(result, plan, coordinator)? {
                    if batch_additional_changes(
                        receiver,
                        plan,
                        coordinator,
                        debounce_ms,
                        stop_requested,
                    )? {
                        stop_requested.store(true, Ordering::SeqCst);
                    }
                    return Ok(());
                }
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "watch event channel closed unexpectedly: {error}"
                ));
            }
        }
    }
}

fn drain_pending_messages(
    receiver: &Receiver<WatchMessage>,
    plan: &WatchPlan,
    coordinator: &mut RunLoopCoordinator,
    debounce_ms: u64,
    stop_requested: &Arc<AtomicBool>,
) -> Result<bool> {
    let mut saw_relevant_change = false;
    loop {
        match receiver.try_recv() {
            Ok(WatchMessage::Stop) => return Ok(true),
            Ok(WatchMessage::Fs(result)) => {
                if record_event_if_relevant(result, plan, coordinator)? {
                    saw_relevant_change = true;
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                return Err(anyhow::anyhow!("watch event channel closed unexpectedly"));
            }
        }
    }

    if saw_relevant_change {
        return batch_additional_changes(receiver, plan, coordinator, debounce_ms, stop_requested);
    }

    Ok(false)
}

fn batch_additional_changes(
    receiver: &Receiver<WatchMessage>,
    plan: &WatchPlan,
    coordinator: &mut RunLoopCoordinator,
    debounce_ms: u64,
    stop_requested: &AtomicBool,
) -> Result<bool> {
    let debounce = Duration::from_millis(debounce_ms);
    loop {
        if stop_requested.load(Ordering::SeqCst) {
            return Ok(true);
        }

        match receiver.recv_timeout(debounce) {
            Ok(WatchMessage::Stop) => return Ok(true),
            Ok(WatchMessage::Fs(result)) => {
                let _ = record_event_if_relevant(result, plan, coordinator)?;
            }
            Err(RecvTimeoutError::Timeout) => return Ok(false),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(anyhow::anyhow!("watch event channel closed unexpectedly"));
            }
        }
    }
}

fn record_event_if_relevant(
    result: Result<Event, notify::Error>,
    plan: &WatchPlan,
    coordinator: &mut RunLoopCoordinator,
) -> Result<bool> {
    let event = match result {
        Ok(event) => event,
        Err(error) => {
            eprintln!("{WATCH_PREFIX} watcher warning: {error}");
            return Ok(false);
        }
    };

    if event.paths.iter().any(|path| plan.is_relevant(path)) {
        coordinator.record_change();
        return Ok(true);
    }

    Ok(false)
}

fn install_ctrlc_handler(
    sender: Sender<WatchMessage>,
    stop_requested: Arc<AtomicBool>,
) -> Result<()> {
    ctrlc::set_handler(move || {
        stop_requested.store(true, Ordering::SeqCst);
        let _ = sender.send(WatchMessage::Stop);
    })
    .context("failed to install Ctrl+C handler for `gof run --watch`")
}

impl WatchPlan {
    fn is_relevant(&self, path: &Path) -> bool {
        let normalized_path = normalize_source_path(path);
        if self
            .source_roots
            .iter()
            .any(|root| normalized_path.starts_with(root))
            && normalized_path.extension().and_then(|value| value.to_str()) == Some("gof")
        {
            return true;
        }

        let Some(parent) = normalized_path.parent() else {
            return false;
        };
        if self
            .package_roots
            .iter()
            .any(|root| normalize_cli_path(root) == normalize_cli_path(parent))
        {
            return matches!(
                normalized_path.file_name().and_then(|value| value.to_str()),
                Some("gof.mod" | "gof.lock")
            );
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::{RunLoopCoordinator, WatchPlan, WatchRoot};
    use crate::normalize_cli_path;
    use std::path::PathBuf;

    #[test]
    fn coordinator_collapses_multiple_changes_into_one_rerun() {
        let mut coordinator = RunLoopCoordinator::default();
        assert!(coordinator.start_if_idle());

        coordinator.record_change();
        coordinator.record_change();

        assert!(coordinator.finish_run());
        assert!(!coordinator.finish_run());
    }

    #[test]
    fn coordinator_does_not_restart_while_idle_without_a_change() {
        let mut coordinator = RunLoopCoordinator::default();
        assert!(coordinator.start_if_idle());
        assert!(!coordinator.start_if_idle());
        assert!(!coordinator.finish_run());
    }

    #[test]
    fn watch_plan_matches_gof_sources_and_package_metadata_only() {
        let root = PathBuf::from("C:/workspace/app");
        let plan = WatchPlan {
            watch_roots: vec![
                WatchRoot {
                    path: root.clone(),
                    recursive: false,
                },
                WatchRoot {
                    path: root.join("src"),
                    recursive: true,
                },
            ],
            source_roots: vec![root.join("src")],
            package_roots: vec![root.clone()],
        };

        assert!(plan.is_relevant(&root.join("src").join("main.gof")));
        assert!(plan.is_relevant(&root.join("gof.mod")));
        assert!(plan.is_relevant(&root.join("gof.lock")));
        assert!(!plan.is_relevant(&root.join("README.md")));
    }

    #[test]
    fn watch_plan_path_matching_is_stable_across_windows_style_case_variants() {
        if !cfg!(windows) {
            return;
        }

        let root = PathBuf::from("C:/Workspace/App");
        let plan = WatchPlan {
            watch_roots: vec![WatchRoot {
                path: root.clone(),
                recursive: false,
            }],
            source_roots: vec![root.join("src")],
            package_roots: vec![root.clone()],
        };

        let manifest_path = PathBuf::from("c:/workspace/app/gof.mod");
        assert_eq!(
            normalize_cli_path(&root),
            normalize_cli_path(
                manifest_path
                    .parent()
                    .expect("manifest path should have a parent")
            )
        );
        assert!(plan.is_relevant(&manifest_path));
    }
}
