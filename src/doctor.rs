//! `ask doctor`: offline, side-effect-free diagnostics and optional live checks.

use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use crate::{
    config::{self, Config, Target},
    credential, emit,
    provider::{Request, RigProvider},
    runner,
    store::{DatabaseState, Health, HealthSource, StorageInspection, Store, TargetHealth},
    utc,
};

pub const LIVE_PROMPT: &str = "Reply with exactly: ok";
pub const LIVE_SYSTEM_PROMPT: &str = "Reply with exactly one word.";
const LIVE_MAX_OUTPUT_TOKENS: u64 = 128;
const COST_NOTICE: &str = "live check sends a minimal provider request that may incur cost";

pub struct Options {
    pub live: bool,
    pub all: bool,
}

struct Diagnosis {
    lines: Vec<String>,
    config_failed: bool,
    environment_failed: bool,
}

struct TargetReport {
    target: Target,
    credential: CredentialState,
    history: Option<TargetHealth>,
    live: Option<Result<(), String>>,
}

enum CredentialState {
    Present,
    Missing,
    InvalidUnicode,
}

struct Paths {
    ask_home: Option<PathBuf>,
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

#[derive(Clone, Copy)]
enum PathExpectation {
    File,
    Directory,
}

pub async fn run(
    options: Options,
    stdout: &mut impl io::Write,
    stderr: &mut impl io::Write,
) -> ExitCode {
    let diagnosis = diagnose(options, stderr).await;
    let status = diagnosis.status();
    if status != ExitCode::SUCCESS {
        let kind = if diagnosis.config_failed {
            "configuration"
        } else {
            "environment"
        };
        let _ = crate::report(stderr, &format!("doctor found {kind} problems"), status);
    }
    let emit_status = emit(stdout, stderr, &diagnosis.render());
    if emit_status != ExitCode::SUCCESS {
        return emit_status;
    }
    status
}

async fn diagnose(options: Options, stderr: &mut impl io::Write) -> Diagnosis {
    let mut diagnosis = Diagnosis::new();
    let paths = match paths(&mut diagnosis) {
        Ok(paths) => paths,
        Err(()) => return diagnosis,
    };
    check_paths(&paths, &mut diagnosis);
    let config = load_config(&paths.config, &mut diagnosis);
    let inspection = inspect_storage(&paths.data, &mut diagnosis);
    let mut target_reports =
        target_reports(config.as_ref(), options.all, &inspection, &mut diagnosis);
    if options.live {
        run_live_checks(&mut target_reports, &paths.data, &mut diagnosis, stderr).await;
    }
    diagnosis.finish(FinishInput {
        config,
        paths,
        database: inspection.state,
        targets: target_reports,
    });
    diagnosis
}

fn target_reports(
    config: Option<&Config>,
    all: bool,
    inspection: &StorageInspection,
    diagnosis: &mut Diagnosis,
) -> Vec<TargetReport> {
    let Some(config) = config else {
        return Vec::new();
    };
    let Some(targets) = targets_to_check(config, all, diagnosis) else {
        return Vec::new();
    };
    let histories = historical_health(inspection);
    let reports = targets
        .into_iter()
        .map(|target| TargetReport {
            credential: credential_state(&target.api_key_env),
            history: histories.get(&identity(&target)).cloned(),
            live: None,
            target,
        })
        .collect::<Vec<_>>();
    note_missing_credentials(&reports, diagnosis);
    reports
}

fn note_missing_credentials(reports: &[TargetReport], diagnosis: &mut Diagnosis) {
    if reports.iter().any(|entry| {
        matches!(
            entry.credential,
            CredentialState::Missing | CredentialState::InvalidUnicode
        )
    }) {
        diagnosis.environment_failed = true;
    }
}

async fn run_live_checks(
    targets: &mut [TargetReport],
    data_path: &Path,
    diagnosis: &mut Diagnosis,
    stderr: &mut impl io::Write,
) {
    if targets.is_empty() {
        return;
    }
    crate::report(
        stderr,
        &format!("warning: {COST_NOTICE}"),
        ExitCode::SUCCESS,
    );
    live_checks(targets, data_path, diagnosis, stderr).await;
    if targets
        .iter()
        .any(|entry| entry.live.as_ref().is_some_and(Result::is_err))
    {
        diagnosis.environment_failed = true;
    }
}

fn paths(diagnosis: &mut Diagnosis) -> Result<Paths, ()> {
    let ask_home = env::var_os("ASK_HOME").map(PathBuf::from);
    let config = config::config_path().map_err(|error| {
        diagnosis.note_environment(error.to_string());
    })?;
    let data = config::data_path().map_err(|error| {
        diagnosis.note_environment(error.to_string());
    })?;
    let cache = config::cache_path().map_err(|error| {
        diagnosis.note_environment(error.to_string());
    })?;
    Ok(Paths {
        ask_home,
        config,
        data,
        cache,
    })
}

fn check_paths(paths: &Paths, diagnosis: &mut Diagnosis) {
    for (path, expected) in [
        (&paths.config, PathExpectation::File),
        (&paths.data, PathExpectation::File),
        (&paths.cache, PathExpectation::Directory),
    ] {
        if let Err(message) = ancestors_accessible(path) {
            diagnosis.note_environment(message);
        }
        if !path_ready(path, expected) {
            diagnosis.note_environment(format!(
                "path '{}' is not ready: {}",
                path.display(),
                describe_path(path, expected)
            ));
        }
    }
}

fn ancestors_accessible(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let mut current = parent.to_path_buf();
    while !current.as_os_str().is_empty() {
        if let Some(error) = ancestor_access_error(&current) {
            return Err(error);
        }
        if !current.pop() {
            break;
        }
    }
    Ok(())
}

fn ancestor_access_error(path: &Path) -> Option<String> {
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => Some(format!(
            "cannot access ancestor '{}': {error}",
            path.display()
        )),
        Ok(metadata) if !metadata.is_dir() => {
            Some(format!("ancestor '{}' is not a directory", path.display()))
        }
        Ok(_) => None,
    }
}

fn path_ready(path: &Path, expected: PathExpectation) -> bool {
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(_) => false,
        Ok(metadata) => match expected {
            PathExpectation::File => metadata.is_file(),
            PathExpectation::Directory => metadata.is_dir(),
        },
    }
}

fn describe_path(path: &Path, expected: PathExpectation) -> &'static str {
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => "absent",
        Err(_) => "inaccessible",
        Ok(metadata) if metadata.is_dir() => match expected {
            PathExpectation::Directory => "directory present",
            PathExpectation::File => "wrong type (expected file, found directory)",
        },
        Ok(metadata) if metadata.is_file() => match expected {
            PathExpectation::File => "present",
            PathExpectation::Directory => "wrong type (expected directory, found file)",
        },
        Ok(_) => "wrong type",
    }
}

fn load_config(path: &Path, diagnosis: &mut Diagnosis) -> Option<Config> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            diagnosis.note_config(format!("cannot read '{}': {error}", path.display()));
            return None;
        }
    };
    match crate::validate::document(&contents) {
        Ok(config) => Some(config),
        Err(problem) => {
            diagnosis.note_config(format!(
                "invalid configuration '{}': {problem}",
                path.display()
            ));
            None
        }
    }
}

fn targets_to_check(config: &Config, all: bool, diagnosis: &mut Diagnosis) -> Option<Vec<Target>> {
    let result = if all {
        config.provider_targets()
    } else {
        config
            .resolve_named(&config.default_profile)
            .map(|target| vec![target])
    };
    match result {
        Ok(targets) => Some(targets),
        Err(error) => {
            diagnosis.note_config(error.to_string());
            None
        }
    }
}

fn inspect_storage(path: &Path, diagnosis: &mut Diagnosis) -> StorageInspection {
    let inspection = Store::inspect_storage(path);
    if matches!(
        inspection.state,
        DatabaseState::Unusable(_) | DatabaseState::Limited(_) | DatabaseState::Inaccessible(_)
    ) {
        diagnosis.environment_failed = true;
    }
    inspection
}

fn historical_health(
    inspection: &StorageInspection,
) -> BTreeMap<(String, String, String), TargetHealth> {
    let mut map = BTreeMap::new();
    if !matches!(inspection.state, DatabaseState::Current) {
        return map;
    }
    for health in &inspection.targets {
        map.insert(
            (
                health.kind.clone(),
                health.base_url.clone(),
                health.model.clone(),
            ),
            health.clone(),
        );
    }
    map
}

fn identity(target: &Target) -> (String, String, String) {
    (
        target.kind.clone(),
        target.base_url.clone(),
        target.model.clone(),
    )
}

fn credential_state(name: &str) -> CredentialState {
    match env::var(name) {
        Ok(_) => CredentialState::Present,
        Err(env::VarError::NotPresent) => CredentialState::Missing,
        Err(env::VarError::NotUnicode(_)) => CredentialState::InvalidUnicode,
    }
}

async fn live_checks(
    targets: &mut [TargetReport],
    data_path: &Path,
    diagnosis: &mut Diagnosis,
    stderr: &mut impl io::Write,
) {
    let mut store = open_store_for_live(data_path, diagnosis, stderr);
    for entry in targets {
        let credential = match credential(&entry.target.api_key_env) {
            Ok(value) => value,
            Err(message) => {
                entry.live = Some(Err(message));
                continue;
            }
        };
        let _started = Instant::now();
        let result = live_request(&entry.target, &credential).await;
        if let Some(store) = store.as_mut() {
            let saved = record_live_health(store, &entry.target, &result);
            report_live_persistence(saved, diagnosis, stderr);
        }
        entry.live = Some(result);
    }
}

fn open_store_for_live(
    data_path: &Path,
    diagnosis: &mut Diagnosis,
    stderr: &mut impl io::Write,
) -> Option<Store> {
    match Store::open(data_path) {
        Ok(store) => Some(store),
        Err(error) => {
            diagnosis.environment_failed = true;
            let _ = crate::report(
                stderr,
                &format!("live health was not recorded: {error}"),
                exit_environment(),
            );
            None
        }
    }
}

fn record_live_health(
    store: &mut Store,
    target: &Target,
    result: &Result<(), String>,
) -> Result<(), String> {
    let health = match result {
        Ok(()) => Health::Success(HealthSource::LiveCheck),
        Err(error) => Health::Failure {
            class: classify_live_error(error),
            source: HealthSource::LiveCheck,
        },
    };
    store
        .record_health(target, health)
        .map_err(|error| error.to_string())
}

fn report_live_persistence(
    result: Result<(), String>,
    diagnosis: &mut Diagnosis,
    stderr: &mut impl io::Write,
) {
    if let Err(error) = result {
        diagnosis.environment_failed = true;
        let _ = crate::report(
            stderr,
            &format!("live health was not recorded: {error}"),
            exit_environment(),
        );
    }
}

async fn live_request(target: &Target, credential: &str) -> Result<(), String> {
    let mut live_target = target.clone();
    live_target.system_prompt = LIVE_SYSTEM_PROMPT.to_string();
    live_target.max_output_tokens = Some(LIVE_MAX_OUTPUT_TOKENS);
    let provider = RigProvider::new(&live_target, credential.to_string());
    let request = Request {
        prompt: LIVE_PROMPT,
        system_prompt: &live_target.system_prompt,
        history: &[],
    };
    let mut sink = io::sink();
    let mut writer = crate::output::AnswerWriter::new(&mut sink);
    let outcome = runner::run(&provider, &live_target, request, &mut writer).await;
    match outcome.error {
        None => Ok(()),
        Some(error) => Err(error.to_string()),
    }
}

fn classify_live_error(message: &str) -> &'static str {
    if message.contains("timed out") {
        "timeout"
    } else {
        "provider"
    }
}

fn exit_config() -> ExitCode {
    ExitCode::from(1)
}

fn exit_environment() -> ExitCode {
    ExitCode::from(3)
}

struct FinishInput {
    config: Option<Config>,
    paths: Paths,
    database: DatabaseState,
    targets: Vec<TargetReport>,
}

impl Diagnosis {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            config_failed: false,
            environment_failed: false,
        }
    }

    fn note_config(&mut self, message: String) {
        self.config_failed = true;
        self.lines.push(format!("configuration: {message}"));
    }

    fn note_environment(&mut self, message: String) {
        self.environment_failed = true;
        self.lines.push(format!("environment: {message}"));
    }

    fn finish(&mut self, input: FinishInput) {
        self.push_configuration_summary();
        self.push_path_section(&input.paths);
        self.push_config_details(input.config.as_ref());
        self.push_database_section(input.database);
        self.push_target_section(input.targets);
    }

    fn push_configuration_summary(&mut self) {
        if !self.config_failed {
            self.lines.push("configuration: valid".to_string());
        }
    }

    fn push_path_section(&mut self, paths: &Paths) {
        self.lines.push(String::new());
        self.lines.push("paths:".to_string());
        self.lines.push(format!(
            "  ask_home: {}",
            display_home(paths.ask_home.as_deref())
        ));
        self.lines.push(format!(
            "  config: {} ({})",
            paths.config.display(),
            describe_path(&paths.config, PathExpectation::File)
        ));
        self.lines.push(format!(
            "  data: {} ({})",
            paths.data.display(),
            describe_path(&paths.data, PathExpectation::File)
        ));
        self.lines.push(format!(
            "  cache: {} ({})",
            paths.cache.display(),
            describe_path(&paths.cache, PathExpectation::Directory)
        ));
    }

    fn push_config_details(&mut self, config: Option<&Config>) {
        let Some(config) = config else {
            return;
        };
        self.lines.push(String::new());
        self.lines
            .push(format!("default profile: {}", config.default_profile));
        if let Some(days) = config.history_days() {
            self.lines.push(format!("history expiry: {days} days"));
        } else {
            self.lines.push("history expiry: disabled".to_string());
        }
    }

    fn push_database_section(&mut self, database: DatabaseState) {
        self.lines.push(String::new());
        self.lines.push(format!("database: {database}"));
    }

    fn push_target_section(&mut self, targets: Vec<TargetReport>) {
        if targets.is_empty() {
            return;
        }
        self.lines.push(String::new());
        self.lines
            .push("provider targets (historical health is not a current check):".to_string());
        for entry in targets {
            self.lines.push(target_line(&entry));
        }
    }

    fn render(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }

    fn status(&self) -> ExitCode {
        if self.config_failed {
            exit_config()
        } else if self.environment_failed {
            exit_environment()
        } else {
            ExitCode::SUCCESS
        }
    }
}

fn display_home(home: Option<&Path>) -> String {
    home.map_or_else(|| "unset".to_string(), |path| path.display().to_string())
}

fn target_line(entry: &TargetReport) -> String {
    let credential = match entry.credential {
        CredentialState::Present => format!("credential {}: present", entry.target.api_key_env),
        CredentialState::Missing => format!("credential {}: missing", entry.target.api_key_env),
        CredentialState::InvalidUnicode => {
            format!("credential {}: not valid Unicode", entry.target.api_key_env)
        }
    };
    let history = entry
        .history
        .as_ref()
        .map(render_health)
        .unwrap_or_else(|| "historical health: unavailable".to_string());
    let live = entry
        .live
        .as_ref()
        .map(|result| match result {
            Ok(()) => "live: ok".to_string(),
            Err(error) => format!("live: failed ({error})"),
        })
        .unwrap_or_default();
    let mut lines = vec![
        format!(
            "{} · profile {} · {} · {}",
            entry.target.kind, entry.target.profile, entry.target.base_url, entry.target.model
        ),
        format!("  {credential}"),
        format!("  {history}"),
    ];
    if !live.is_empty() {
        lines.push(format!("  {live}"));
    }
    lines.join("\n")
}

fn render_health(health: &TargetHealth) -> String {
    let healthy = health.last_success_at_ms.map_or_else(
        || "never observed healthy".to_string(),
        |at| {
            format!(
                "last observed healthy {} {}",
                utc::format(at),
                source_phrase(health.last_success_source.as_deref())
            )
        },
    );
    let failure = match (health.last_failure_at_ms, &health.last_failure_class) {
        (Some(at), Some(class)) => format!(
            "last failure {} ({class}) {}",
            utc::format(at),
            source_phrase(health.last_failure_source.as_deref())
        ),
        (Some(at), None) => format!(
            "last failure {} {}",
            utc::format(at),
            source_phrase(health.last_failure_source.as_deref())
        ),
        (None, _) => "no failures observed".to_string(),
    };
    format!("historical health: {healthy} · {failure}")
}

fn source_phrase(source: Option<&str>) -> String {
    match source {
        Some("query") => "(from query)".to_string(),
        Some("live-check") => "(from live check)".to_string(),
        Some(other) => format!("(from {other})"),
        None => String::new(),
    }
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
