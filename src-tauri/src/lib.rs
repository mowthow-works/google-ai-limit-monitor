use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const AGY_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

#[cfg(target_os = "windows")]
const AGY_BINARY_NAME: &str = "agy.exe";
#[cfg(not(target_os = "windows"))]
const AGY_BINARY_NAME: &str = "agy";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub autostart_enabled: Option<bool>,
}

fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Path error: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Create dir error: {}", e))?;
    Ok(dir.join("config.json"))
}

fn load_config(app: &AppHandle) -> AppConfig {
    if let Ok(path) = get_config_path(app) {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<AppConfig>(&content) {
                    return cfg;
                }
            }
        }
    }
    AppConfig::default()
}

fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app)?;
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Serialize error: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("Write error: {}", e))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutostartReconcilePlan {
    pub desired_enabled: bool,
    pub should_save_config: bool,
    pub should_apply_os: bool,
}

pub fn plan_autostart_reconciliation(saved: Option<bool>, os_enabled: bool) -> AutostartReconcilePlan {
    match saved {
        None => AutostartReconcilePlan {
            desired_enabled: os_enabled,
            should_save_config: true,
            should_apply_os: false,
        },
        Some(desired) => AutostartReconcilePlan {
            desired_enabled: desired,
            should_save_config: false,
            should_apply_os: desired != os_enabled,
        },
    }
}

fn sync_autostart_on_startup(app: &AppHandle) -> bool {
    let mut config = load_config(app);
    let autolaunch = app.autolaunch();
    let os_enabled = autolaunch.is_enabled().unwrap_or(false);
    let plan = plan_autostart_reconciliation(config.autostart_enabled, os_enabled);

    if plan.should_save_config {
        config.autostart_enabled = Some(plan.desired_enabled);
        let _ = save_config(app, &config);
    }

    if plan.should_apply_os {
        let apply_res = if plan.desired_enabled {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if apply_res.is_ok() {
            return plan.desired_enabled;
        }
        return os_enabled;
    }

    plan.desired_enabled
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BucketInfo {
    pub bucket_id: String,
    pub display_name: String,
    pub window: String,
    pub remaining_fraction: f64,
    pub percentage: f64,
    pub reset_time: String,
    pub remaining_seconds: i64,
    pub remaining_formatted: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub model_id: String,
    pub label: String,
    pub remaining_fraction: f64,
    pub percentage: f64,
    pub reset_time: String,
    pub is_recommended: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuotaGroup {
    pub id: String,
    pub display_name: String,
    pub tab_label: String,
    pub description: String,
    pub five_hour_bucket: Option<BucketInfo>,
    pub weekly_bucket: Option<BucketInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuotaStatus {
    pub connected: bool,
    pub source: String,
    pub status_message: String,
    pub plan_name: String,
    pub prompt_credits: Option<i64>,
    pub email: String,
    pub updated_at: String,
    pub groups: Vec<QuotaGroup>,
    pub models: Vec<ModelInfo>,
}

#[derive(Deserialize, Debug)]
struct ProcessDiscovery {
    pid: Option<u32>,
    token: Option<String>,
    ports: Option<Vec<u16>>,
}

#[derive(Deserialize, Debug)]
struct ProcessDiscoveryList {
    #[serde(default)]
    items: Vec<ProcessDiscovery>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct HubDiscoveryFile {
    pid: u32,
    https_port: u16,
    csrf_token: String,
}

fn format_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "リセット完了".to_string();
    }
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}日 {}時間", days, hours)
    } else if hours > 0 {
        format!("{}時間 {}分", hours, minutes)
    } else {
        format!("{}分", minutes)
    }
}

fn get_antigravity_daemon_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".gemini").join("antigravity").join("daemon"))
}

/// 稼働中の language_server プロセス PID に一致する hub_*.json から接続情報を取得する
fn find_hub_discovery_match(
    running_pids: &HashSet<u32>,
    daemon_dir: &Path,
) -> Option<(String, Vec<u16>)> {
    if running_pids.is_empty() || !daemon_dir.is_dir() {
        return None;
    }

    let entries = fs::read_dir(daemon_dir).ok()?;
    let mut candidates: Vec<(SystemTime, String, u16)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };
        if !file_name.starts_with("hub_") || !file_name.ends_with(".json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let hub: HubDiscoveryFile = match serde_json::from_str(&content) {
            Ok(h) => h,
            Err(_) => continue,
        };

        if running_pids.contains(&hub.pid) && hub.https_port > 0 && !hub.csrf_token.is_empty() {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((modified, hub.csrf_token, hub.https_port));
        }
    }

    // 複数一致した場合は更新日時が最新のものを優先する
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates
        .into_iter()
        .next()
        .map(|(_, token, port)| (token, vec![port]))
}

fn query_language_server_processes() -> Vec<ProcessDiscovery> {
    let script = r#"
        $procs = @(Get-CimInstance Win32_Process -Filter "Name like '%language_server%'" -ErrorAction SilentlyContinue)
        $results = @()
        foreach ($proc in $procs) {
            $token = $null
            $ports = $null
            if ($proc.CommandLine -match '--csrf_token\s+([a-zA-Z0-9\-]+)') {
                $token = $Matches[1]
                $ports = @(Get-NetTCPConnection -OwningProcess $proc.ProcessId -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty LocalPort -Unique)
            }
            $results += [PSCustomObject]@{
                pid = [uint32]$proc.ProcessId
                token = $token
                ports = $ports
            }
        }
        @{ items = @($results) } | ConvertTo-Json -Compress -Depth 3
    "#;

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);

    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<ProcessDiscoveryList>(stdout.trim())
        .map(|list| list.items)
        .unwrap_or_default()
}

fn discover_language_server() -> Option<(String, Vec<u16>)> {
    let processes = query_language_server_processes();
    if processes.is_empty() {
        return None;
    }

    let running_pids: HashSet<u32> = processes.iter().filter_map(|p| p.pid).collect();

    // 1. Antigravity 2.x 以降: daemon/hub_*.json から稼働中プロセスの接続情報を取得
    if let Some(daemon_dir) = get_antigravity_daemon_dir() {
        if let Some(matched) = find_hub_discovery_match(&running_pids, &daemon_dir) {
            return Some(matched);
        }
    }

    // 2. フォールバック (Antigravity 1.x): コマンドラインの --csrf_token を使用
    for proc in processes {
        if let (Some(token), Some(ports)) = (proc.token, proc.ports) {
            if !token.is_empty() && !ports.is_empty() {
                return Some((token, ports));
            }
        }
    }

    None
}

fn fetch_rpc(
    token: &str,
    ports: &[u16],
    method: &str,
) -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(1500))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    for port in ports {
        let url = format!(
            "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{}",
            port, method
        );

        let res = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("X-Codeium-Csrf-Token", token)
            .body("{}")
            .send();

        if let Ok(resp) = res {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>() {
                    return Ok(json);
                }
            }
        }
    }

    Err(format!("Could not connect to {} on any port", method))
}

fn create_bucket_info(
    bucket_id: String,
    display_name: String,
    window: String,
    fraction: f64,
    reset_time: String,
    now_utc: DateTime<Utc>,
) -> BucketInfo {
    let percentage = (fraction * 1000.0).round() / 10.0;
    let mut diff_seconds: i64 = 0;
    if let Ok(reset_dt) = reset_time.parse::<DateTime<Utc>>() {
        diff_seconds = (reset_dt - now_utc).num_seconds();
    }

    BucketInfo {
        bucket_id,
        display_name,
        window,
        remaining_fraction: fraction,
        percentage,
        reset_time,
        remaining_seconds: diff_seconds,
        remaining_formatted: format_duration(diff_seconds),
    }
}

fn normalize_bucket_window(raw_window: Option<&str>, bucket_id: &str) -> Option<&'static str> {
    if let Some(w) = raw_window {
        match w.trim().to_ascii_lowercase().as_str() {
            "5h" => return Some("5h"),
            "weekly" => return Some("weekly"),
            _ => {}
        }
    }

    let id_lower = bucket_id.trim().to_ascii_lowercase();
    if id_lower.ends_with("-5h") || id_lower.ends_with("_5h") || id_lower == "5h" {
        Some("5h")
    } else if id_lower.ends_with("-weekly") || id_lower.ends_with("_weekly") || id_lower == "weekly" {
        Some("weekly")
    } else {
        None
    }
}

fn derive_tab_label(display_name: &str) -> String {
    let trimmed = display_name.trim();
    let lower = trimmed.to_lowercase();
    if lower.contains("argon") {
        "Argon".to_string()
    } else if lower.contains("gemini") {
        "Gemini".to_string()
    } else if lower.contains("claude") || lower.contains("gpt") {
        "Claude / GPT".to_string()
    } else {
        trimmed.to_string()
    }
}

fn derive_group_base_id(display_name: &str, fallback_bucket_id: &str) -> String {
    let lower = display_name.trim().to_lowercase();
    if lower.contains("argon") {
        return "argon".to_string();
    }
    if lower.contains("gemini") {
        return "gemini".to_string();
    }
    if lower.contains("claude") || lower.contains("gpt") {
        return "claude".to_string();
    }

    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if !slug.is_empty() && !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if !slug.is_empty() {
        slug
    } else if !fallback_bucket_id.trim().is_empty() {
        fallback_bucket_id.trim().to_ascii_lowercase()
    } else {
        "group".to_string()
    }
}

fn make_unique_group_id(base_id: String, used_ids: &mut HashSet<String>) -> String {
    if used_ids.insert(base_id.clone()) {
        return base_id;
    }
    let mut idx = 2usize;
    loop {
        let candidate = format!("{}-{}", base_id, idx);
        if used_ids.insert(candidate.clone()) {
            return candidate;
        }
        idx += 1;
    }
}

fn build_quota_group(
    raw_display_name: &str,
    description: String,
    five_hour_bucket: Option<BucketInfo>,
    weekly_bucket: Option<BucketInfo>,
    used_ids: &mut HashSet<String>,
) -> Option<QuotaGroup> {
    if five_hour_bucket.is_none() && weekly_bucket.is_none() {
        return None;
    }

    let fallback_bucket = five_hour_bucket
        .as_ref()
        .or(weekly_bucket.as_ref());
    let fallback_bucket_id = fallback_bucket
        .map(|b| b.bucket_id.as_str())
        .unwrap_or("");
    let display_name = if !raw_display_name.trim().is_empty() {
        raw_display_name.trim().to_string()
    } else if let Some(b) = fallback_bucket {
        if !b.display_name.trim().is_empty() {
            b.display_name.trim().to_string()
        } else {
            b.bucket_id.clone()
        }
    } else {
        "Quota".to_string()
    };

    let base_id = derive_group_base_id(&display_name, fallback_bucket_id);
    let id = make_unique_group_id(base_id, used_ids);
    let tab_label = derive_tab_label(&display_name);

    Some(QuotaGroup {
        id,
        display_name,
        tab_label,
        description,
        five_hour_bucket,
        weekly_bucket,
    })
}

/// RPC (RetrieveUserQuotaSummary) のレスポンスからクォータ保持グループを動的に抽出する
fn parse_rpc_quota_summary(
    summary_json: &serde_json::Value,
    now_utc: DateTime<Utc>,
) -> Vec<QuotaGroup> {
    let mut parsed_groups = Vec::new();
    let mut used_ids = HashSet::new();

    if let Some(raw_groups) = summary_json["response"]["groups"].as_array() {
        for g in raw_groups {
            let display_name = g["displayName"].as_str().unwrap_or("");
            let description = g["description"].as_str().unwrap_or("").to_string();

            let mut five_hour: Option<BucketInfo> = None;
            let mut weekly: Option<BucketInfo> = None;

            if let Some(buckets) = g["buckets"].as_array() {
                for b in buckets {
                    let Some(fraction) = b["remainingFraction"].as_f64() else {
                        continue;
                    };
                    let bucket_id = b["bucketId"].as_str().unwrap_or("").to_string();
                    let Some(window) = normalize_bucket_window(b["window"].as_str(), &bucket_id) else {
                        continue;
                    };
                    let b_display = b["displayName"].as_str().unwrap_or("").to_string();
                    let reset_time_str = b["resetTime"].as_str().unwrap_or("").to_string();

                    let info = create_bucket_info(
                        bucket_id,
                        b_display,
                        window.to_string(),
                        fraction,
                        reset_time_str,
                        now_utc,
                    );

                    if window == "5h" {
                        five_hour = Some(info);
                    } else if window == "weekly" {
                        weekly = Some(info);
                    }
                }
            }

            if let Some(group) =
                build_quota_group(display_name, description, five_hour, weekly, &mut used_ids)
            {
                parsed_groups.push(group);
            }
        }
    }

    parsed_groups
}

/// 稼働中 Language Server へのローカル RPC 経由でクォータ情報を取得する
fn fetch_quota_via_rpc(now_utc: DateTime<Utc>, updated_at: &str) -> Result<QuotaStatus, QuotaStatus> {
    let (token, ports) = match discover_language_server() {
        Some(val) => val,
        None => {
            return Err(QuotaStatus {
                connected: false,
                source: "none".to_string(),
                status_message: "Language Server 待機中...".to_string(),
                plan_name: "未接続".to_string(),
                prompt_credits: None,
                email: String::new(),
                updated_at: updated_at.to_string(),
                groups: Vec::new(),
                models: Vec::new(),
            });
        }
    };

    // 1. GetUserStatus からプラン名、クレジット、モデル別一覧を取得
    let mut email = String::new();
    let mut plan_name = "Pro".to_string();
    let mut prompt_credits: i64 = 0;
    let mut parsed_models = Vec::new();

    if let Ok(status_json) = fetch_rpc(&token, &ports, "GetUserStatus") {
        let user_status = &status_json["userStatus"];
        email = user_status["email"].as_str().unwrap_or("").to_string();
        plan_name = user_status["planStatus"]["planInfo"]["planName"]
            .as_str()
            .unwrap_or("Pro")
            .to_string();
        prompt_credits = user_status["planStatus"]["availablePromptCredits"]
            .as_i64()
            .unwrap_or(0);

        if let Some(configs) = user_status["cascadeModelConfigData"]["clientModelConfigs"].as_array() {
            for c in configs {
                let label = c["label"].as_str().unwrap_or("").to_string();
                let model_id = c["modelId"].as_str().unwrap_or("").to_string();
                let is_recommended = c["isRecommended"].as_bool().unwrap_or(false);
                let fraction = c["quotaInfo"]["remainingFraction"].as_f64().unwrap_or(1.0);
                let percentage = (fraction * 1000.0).round() / 10.0;
                let reset_time = c["quotaInfo"]["resetTime"].as_str().unwrap_or("").to_string();

                if !label.is_empty() {
                    parsed_models.push(ModelInfo {
                        model_id,
                        label,
                        remaining_fraction: fraction,
                        percentage,
                        reset_time,
                        is_recommended,
                    });
                }
            }
        }
    }

    // 2. RetrieveUserQuotaSummary から最新クォータ詳細を取得（クォータ保持グループのみ動的構成）
    let summary_json = match fetch_rpc(&token, &ports, "RetrieveUserQuotaSummary") {
        Ok(j) => j,
        Err(err) => {
            return Err(QuotaStatus {
                connected: false,
                source: "none".to_string(),
                status_message: format!("通信エラー: {}", err),
                plan_name: "エラー".to_string(),
                prompt_credits: Some(prompt_credits),
                email,
                updated_at: updated_at.to_string(),
                groups: Vec::new(),
                models: parsed_models,
            });
        }
    };

    let parsed_groups = parse_rpc_quota_summary(&summary_json, now_utc);

    if parsed_groups.is_empty() {
        return Err(QuotaStatus {
            connected: false,
            source: "none".to_string(),
            status_message: "Language Server からクォータを取得できませんでした".to_string(),
            plan_name: "未接続".to_string(),
            prompt_credits: None,
            email,
            updated_at: updated_at.to_string(),
            groups: Vec::new(),
            models: parsed_models,
        });
    }

    Ok(QuotaStatus {
        connected: true,
        source: "rpc".to_string(),
        status_message: "正常稼働中".to_string(),
        plan_name,
        prompt_credits: Some(prompt_credits),
        email,
        updated_at: updated_at.to_string(),
        groups: parsed_groups,
        models: parsed_models,
    })
}

/// %LOCALAPPDATA%\agy\bin\agy.exe を優先し、なければ PATH から agy 実行ファイルを解決する
fn resolve_agy_path_with(
    local_app_data: Option<&Path>,
    path_env: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(local_dir) = local_app_data {
        let candidate = local_dir.join("agy").join("bin").join(AGY_BINARY_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let candidate = dir.join(AGY_BINARY_NAME);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn resolve_agy_path() -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let path_env = std::env::var_os("PATH");
    resolve_agy_path_with(local_app_data.as_deref(), path_env.as_deref())
}

/// ウィンドウ非表示かつタイムアウト付きで agy -p "/usage" --output-format json を実行する
fn run_agy_usage_command(agy_path: &Path, timeout: Duration) -> Result<String, String> {
    let mut cmd = Command::new(agy_path);
    cmd.args(["-p", "/usage", "--output-format", "json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = cmd.spawn().map_err(|e| format!("agy 起動失敗: {}", e))?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });

    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    return Err("agy 実行タイムアウト".to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                return Err(format!("agy 待機エラー: {}", e));
            }
        }
    };

    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();

    if !status.success() {
        let stderr_str = String::from_utf8_lossy(&stderr_bytes);
        return Err(format!(
            "agy 異常終了 (code={:?}): {}",
            status.code(),
            stderr_str.trim()
        ));
    }

    Ok(String::from_utf8_lossy(&stdout_bytes).into_owned())
}

/// agy /usage の JSON 出力から groups と buckets の構造をもとにクォータ保持グループを動的に組み立てる
fn parse_agy_usage_json(
    raw_json: &str,
    now_utc: DateTime<Utc>,
    updated_at: String,
) -> Result<QuotaStatus, String> {
    let json: serde_json::Value = serde_json::from_str(raw_json.trim())
        .map_err(|e| format!("agy JSON パースエラー: {}", e))?;

    if json["status"].as_str() != Some("SUCCESS") {
        return Err(format!(
            "agy ステータス異常: {:?}",
            json["status"].as_str()
        ));
    }

    let raw_groups = json["command"]["data"]["groups"]
        .as_array()
        .ok_or_else(|| "agy groups が見つかりません".to_string())?;

    let mut groups = Vec::new();
    let mut used_ids = HashSet::new();

    for g in raw_groups {
        let display_name = g["name"].as_str().unwrap_or("");
        let description = g["description"].as_str().unwrap_or("").to_string();
        let Some(buckets) = g["buckets"].as_array() else {
            continue;
        };

        let mut five_hour: Option<BucketInfo> = None;
        let mut weekly: Option<BucketInfo> = None;

        for b in buckets {
            let Some(fraction) = b["remaining_fraction"].as_f64() else {
                continue;
            };
            let bucket_id = b["id"].as_str().unwrap_or("").to_string();
            let Some(window) = normalize_bucket_window(b["window"].as_str(), &bucket_id) else {
                continue;
            };
            let b_display = b["name"].as_str().unwrap_or("").to_string();
            let reset_time_str = b["reset_time"].as_str().unwrap_or("").to_string();

            let info = create_bucket_info(
                bucket_id,
                b_display,
                window.to_string(),
                fraction,
                reset_time_str,
                now_utc,
            );

            if window == "5h" {
                five_hour = Some(info);
            } else if window == "weekly" {
                weekly = Some(info);
            }
        }

        if let Some(group) =
            build_quota_group(display_name, description, five_hour, weekly, &mut used_ids)
        {
            groups.push(group);
        }
    }

    if groups.is_empty() {
        return Err("クォータバケットを含むグループが見つかりません".to_string());
    }

    Ok(QuotaStatus {
        connected: true,
        source: "agy".to_string(),
        status_message: "agy 経由で取得中".to_string(),
        plan_name: "―".to_string(),
        prompt_credits: None,
        email: String::new(),
        updated_at,
        groups,
        models: Vec::new(),
    })
}

fn fetch_quota_via_agy(now_utc: DateTime<Utc>, updated_at: &str) -> Result<QuotaStatus, String> {
    let agy_path = resolve_agy_path().ok_or_else(|| "agy が見つかりません".to_string())?;
    let raw_json = run_agy_usage_command(&agy_path, AGY_COMMAND_TIMEOUT)?;
    parse_agy_usage_json(&raw_json, now_utc, updated_at.to_string())
}

fn resolve_quota_with_fallback<FRpc, FAgy>(rpc_fetcher: FRpc, agy_fetcher: FAgy) -> QuotaStatus
where
    FRpc: FnOnce() -> Result<QuotaStatus, QuotaStatus>,
    FAgy: FnOnce() -> Result<QuotaStatus, String>,
{
    match rpc_fetcher() {
        Ok(status) => status,
        Err(wait_status) => match agy_fetcher() {
            Ok(agy_status) => agy_status,
            Err(_) => wait_status,
        },
    }
}

fn get_quota_status_blocking() -> QuotaStatus {
    let now_utc = Utc::now();
    let now_local = Local::now();
    let updated_at = now_local.format("%H:%M:%S").to_string();

    resolve_quota_with_fallback(
        || fetch_quota_via_rpc(now_utc, &updated_at),
        || fetch_quota_via_agy(now_utc, &updated_at),
    )
}

#[tauri::command]
async fn get_quota_status() -> QuotaStatus {
    tauri::async_runtime::spawn_blocking(get_quota_status_blocking)
        .await
        .unwrap_or_else(|_| {
            let now_local = Local::now();
            QuotaStatus {
                connected: false,
                source: "none".to_string(),
                status_message: "スレッドエラー".to_string(),
                plan_name: "エラー".to_string(),
                prompt_credits: None,
                email: "".to_string(),
                updated_at: now_local.format("%H:%M:%S").to_string(),
                groups: Vec::new(),
                models: Vec::new(),
            }
        })
}

#[tauri::command]
fn drag_window(window: tauri::Window) {
    let _ = window.start_dragging();
}

#[tauri::command]
fn minimize_window(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn close_window(window: tauri::Window) {
    let _ = window.hide();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .setup(|app| {
            let handle = app.handle();

            // スタートアップ自動起動の初期化・永続化同期 (Issue #14)
            let is_autostart = sync_autostart_on_startup(handle);
            let item_autostart = CheckMenuItem::with_id(
                handle,
                "toggle_autostart",
                "Windows起動時に自動起動",
                true,
                is_autostart,
                None::<&str>,
            )?;
            let item_autostart_clone = item_autostart.clone();

            let toggle_window = MenuItem::with_id(handle, "toggle_window", "表示 / 非表示", true, None::<&str>)?;
            let refresh_quota = MenuItem::with_id(handle, "refresh_quota", "今すぐ更新", true, None::<&str>)?;
            let quit = MenuItem::with_id(handle, "quit", "終了", true, None::<&str>)?;

            let menu = Menu::with_items(
                handle,
                &[
                    &toggle_window,
                    &refresh_quota,
                    &item_autostart,
                    &quit,
                ],
            )?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Google AI Limit Monitor")
                .show_menu_on_left_click(false);

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            let _tray = tray_builder
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "toggle_window" => {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                    "refresh_quota" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("trigger-refresh", ());
                        }
                    }
                    "toggle_autostart" => {
                        let autolaunch = app.autolaunch();
                        let currently_enabled = autolaunch.is_enabled().unwrap_or(false);
                        if currently_enabled {
                            if autolaunch.disable().is_ok() {
                                let _ = item_autostart_clone.set_checked(false);
                                let mut cfg = load_config(app);
                                cfg.autostart_enabled = Some(false);
                                let _ = save_config(app, &cfg);
                            }
                        } else {
                            if autolaunch.enable().is_ok() {
                                let _ = item_autostart_clone.set_checked(true);
                                let mut cfg = load_config(app);
                                cfg.autostart_enabled = Some(true);
                                let _ = save_config(app, &cfg);
                            }
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(handle)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_quota_status,
            drag_window,
            minimize_window,
            close_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_autostart_reconciliation() {
        // 初回移行時（config 未保存）は OS の現在状態を取り込んで config に保存する
        assert_eq!(
            plan_autostart_reconciliation(None, true),
            AutostartReconcilePlan {
                desired_enabled: true,
                should_save_config: true,
                should_apply_os: false,
            }
        );
        assert_eq!(
            plan_autostart_reconciliation(None, false),
            AutostartReconcilePlan {
                desired_enabled: false,
                should_save_config: true,
                should_apply_os: false,
            }
        );

        // config が true なのに OS 側が消えている（バージョンアップ時など）は自動復元する
        assert_eq!(
            plan_autostart_reconciliation(Some(true), false),
            AutostartReconcilePlan {
                desired_enabled: true,
                should_save_config: false,
                should_apply_os: true,
            }
        );

        // config と OS 側が一致していれば何もしない
        assert_eq!(
            plan_autostart_reconciliation(Some(true), true),
            AutostartReconcilePlan {
                desired_enabled: true,
                should_save_config: false,
                should_apply_os: false,
            }
        );
        assert_eq!(
            plan_autostart_reconciliation(Some(false), false),
            AutostartReconcilePlan {
                desired_enabled: false,
                should_save_config: false,
                should_apply_os: false,
            }
        );

        // config が false なのに OS 側が有効な場合は無効化して同期する
        assert_eq!(
            plan_autostart_reconciliation(Some(false), true),
            AutostartReconcilePlan {
                desired_enabled: false,
                should_save_config: false,
                should_apply_os: true,
            }
        );
    }

    #[test]
    fn test_find_hub_discovery_match() {
        let temp_dir = std::env::temp_dir().join(format!(
            "galm_hub_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // 停止済みプロセスの古い discovery ファイル
        fs::write(
            temp_dir.join("hub_40001.json"),
            r#"{"pid":11111,"httpsPort":40000,"httpPort":40001,"lsVersion":"2.34.0","csrfToken":"stale-token"}"#,
        )
        .unwrap();

        // 稼働中プロセスの discovery ファイル
        fs::write(
            temp_dir.join("hub_52707.json"),
            r#"{"pid":24688,"httpsPort":52706,"httpPort":52707,"lsVersion":"2.34.0","csrfToken":"live-token"}"#,
        )
        .unwrap();

        // 無効な JSON や別ファイルは無視される
        fs::write(temp_dir.join("hub_broken.json"), r#"{"pid":24688"#).unwrap();
        fs::write(
            temp_dir.join("other_52707.json"),
            r#"{"pid":24688,"httpsPort":55555,"csrfToken":"wrong-file"}"#,
        )
        .unwrap();

        let mut running = HashSet::new();
        running.insert(24688);

        assert_eq!(
            find_hub_discovery_match(&running, &temp_dir),
            Some(("live-token".to_string(), vec![52706]))
        );

        // 稼働中プロセスに一致しない場合は None
        let mut other_running = HashSet::new();
        other_running.insert(99999);
        assert_eq!(find_hub_discovery_match(&other_running, &temp_dir), None);

        // 稼働中プロセスが空の場合は None
        let empty_running = HashSet::new();
        assert_eq!(find_hub_discovery_match(&empty_running, &temp_dir), None);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    const SAMPLE_AGY_JSON_LEGACY_3_GROUPS: &str = r#"{"conversation_id":"","status":"SUCCESS","response":"...","duration_seconds":0,"num_turns":0,"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0},"command":{"name":"usage","data":{"description":"Within each group...","groups":[{"name":"Gemini Models","description":"Models within this group: Gemini 4 Neon, Gemini 4 Hydrogen","buckets":[{"id":"gemini-weekly","name":"Weekly Limit Remaining","description":"...","window":"weekly","remaining_fraction":0.16746044158935547,"reset_time":"2026-11-25T11:23:39Z"},{"id":"gemini-5h","name":"Five Hour Limit Remaining","description":"...","window":"5h","remaining_fraction":0.3740962743759155,"reset_time":"2026-11-21T02:29:02Z"}]},{"name":"Claude and GPT models","description":"Models within this group: Claude Opus 5.5, Claude Sonnet 5.5, GPT-OSS 120B","buckets":[{"id":"3p-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":1,"reset_time":"2026-11-28T01:04:58Z"},{"id":"3p-5h","name":"Five Hour Limit Remaining","window":"5h","remaining_fraction":1,"reset_time":"2026-11-21T06:04:58Z"}]},{"name":"Gemini 4 Argon","description":"For paid Pro plans, Argon quota is separate.","buckets":[{"id":"gemini-argon-wtu-weekly","name":"Gemini 4 Argon Weekly Limit","description":"...","window":"weekly","remaining_fraction":0.0711742639541626,"reset_time":"2026-11-25T11:15:13Z"}]}]}}}"#;

    const SAMPLE_AGY_JSON_NEW_2_GROUPS: &str = r#"{"conversation_id":"","status":"SUCCESS","response":"...","duration_seconds":0,"num_turns":0,"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0},"command":{"name":"usage","data":{"description":"Within each group...","groups":[{"name":"Gemini Models","description":"Models within this group: Gemini 4 Neon, Gemini 4 Carbon, Gemini 4 Argon, Gemini 4 Hydrogen, Orchestrator (Neon), Orchestrator (Carbon), Orchestrator (Argon), Gemini 3.8 Flash","buckets":[{"id":"gemini-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":0.9845706224441528,"reset_time":"2026-12-01T11:54:24Z"},{"id":"gemini-5h","name":"Five Hour Limit Remaining","window":"5h","remaining_fraction":0.9482823610305786,"reset_time":"2026-11-24T16:54:24Z"}]},{"name":"Claude and GPT models","description":"Models within this group: Claude Opus 5.5, Claude Sonnet 5.5, GPT-OSS 120B","buckets":[{"id":"3p-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":0.4883776009082794,"reset_time":"2026-11-28T07:37:58Z"},{"id":"3p-5h","name":"Five Hour Limit Remaining","window":"5h","remaining_fraction":1,"reset_time":"2026-11-24T16:59:13Z"}]}]}}}"#;

    #[test]
    fn test_parse_agy_usage_json_legacy_3_groups() {
        let now = "2026-11-21T01:05:00Z".parse::<DateTime<Utc>>().unwrap();
        let status = parse_agy_usage_json(
            SAMPLE_AGY_JSON_LEGACY_3_GROUPS,
            now,
            "10:05:00".to_string(),
        )
        .unwrap();

        assert!(status.connected);
        assert_eq!(status.source, "agy");
        assert_eq!(status.plan_name, "―");
        assert_eq!(status.prompt_credits, None);
        assert!(status.models.is_empty());
        assert_eq!(status.groups.len(), 3);

        // 0: Gemini Models
        let g0 = &status.groups[0];
        assert_eq!(g0.id, "gemini");
        assert_eq!(g0.tab_label, "Gemini");
        assert_eq!(g0.display_name, "Gemini Models");
        assert_eq!(g0.five_hour_bucket.as_ref().unwrap().bucket_id, "gemini-5h");
        assert_eq!(g0.five_hour_bucket.as_ref().unwrap().percentage, 37.4);
        assert_eq!(g0.weekly_bucket.as_ref().unwrap().bucket_id, "gemini-weekly");
        assert_eq!(g0.weekly_bucket.as_ref().unwrap().percentage, 16.7);

        // 1: Claude and GPT models
        let g1 = &status.groups[1];
        assert_eq!(g1.id, "claude");
        assert_eq!(g1.tab_label, "Claude / GPT");
        assert_eq!(g1.display_name, "Claude and GPT models");
        assert_eq!(g1.five_hour_bucket.as_ref().unwrap().bucket_id, "3p-5h");
        assert_eq!(g1.five_hour_bucket.as_ref().unwrap().percentage, 100.0);
        assert_eq!(g1.weekly_bucket.as_ref().unwrap().bucket_id, "3p-weekly");
        assert_eq!(g1.weekly_bucket.as_ref().unwrap().percentage, 100.0);

        // 2: Gemini 4 Argon
        let g2 = &status.groups[2];
        assert_eq!(g2.id, "argon");
        assert_eq!(g2.tab_label, "Argon");
        assert_eq!(g2.display_name, "Gemini 4 Argon");
        assert!(g2.five_hour_bucket.is_none());
        assert_eq!(
            g2.weekly_bucket.as_ref().unwrap().bucket_id,
            "gemini-argon-wtu-weekly"
        );
        assert_eq!(g2.weekly_bucket.as_ref().unwrap().percentage, 7.1);
    }

    #[test]
    fn test_parse_agy_usage_json_new_2_groups() {
        let now = "2026-11-24T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let status = parse_agy_usage_json(
            SAMPLE_AGY_JSON_NEW_2_GROUPS,
            now,
            "21:00:00".to_string(),
        )
        .unwrap();

        assert!(status.connected);
        assert_eq!(status.source, "agy");
        assert_eq!(status.groups.len(), 2);

        assert_eq!(status.groups[0].id, "gemini");
        assert_eq!(status.groups[0].tab_label, "Gemini");
        assert_eq!(status.groups[0].five_hour_bucket.as_ref().unwrap().percentage, 94.8);
        assert_eq!(status.groups[0].weekly_bucket.as_ref().unwrap().percentage, 98.5);

        assert_eq!(status.groups[1].id, "claude");
        assert_eq!(status.groups[1].tab_label, "Claude / GPT");
        assert_eq!(status.groups[1].five_hour_bucket.as_ref().unwrap().percentage, 100.0);
        assert_eq!(status.groups[1].weekly_bucket.as_ref().unwrap().percentage, 48.8);
    }

    #[test]
    fn test_parse_agy_usage_json_unknown_group_and_empty_group() {
        let now = "2026-11-24T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let raw = r#"{
            "status": "SUCCESS",
            "command": {
                "name": "usage",
                "data": {
                    "groups": [
                        {
                            "name": "Empty Group Without Quota",
                            "buckets": []
                        },
                        {
                            "name": "Experimental Ultra",
                            "description": "Future experimental models",
                            "buckets": [
                                { "id": "ultra-weekly", "name": "Ultra Weekly", "remaining_fraction": 0.72, "reset_time": "2026-12-01T11:54:24Z" }
                            ]
                        }
                    ]
                }
            }
        }"#;

        let status = parse_agy_usage_json(raw, now, "21:00:00".to_string()).unwrap();
        assert_eq!(status.groups.len(), 1);
        assert_eq!(status.groups[0].id, "experimental-ultra");
        assert_eq!(status.groups[0].tab_label, "Experimental Ultra");
        assert_eq!(status.groups[0].display_name, "Experimental Ultra");
        assert!(status.groups[0].five_hour_bucket.is_none());
        assert_eq!(status.groups[0].weekly_bucket.as_ref().unwrap().percentage, 72.0);
    }

    #[test]
    fn test_parse_rpc_quota_summary_new_2_groups_and_empty_argon() {
        let now = "2026-11-24T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let summary: serde_json::Value = serde_json::from_str(r#"{
            "response": {
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "description": "Gemini shared quota",
                        "buckets": [
                            { "bucketId": "gemini-5h", "displayName": "5h", "window": "5h", "remainingFraction": 0.98, "resetTime": "2026-11-24T16:54:24Z" },
                            { "bucketId": "gemini-weekly", "displayName": "Weekly", "window": "weekly", "remainingFraction": 0.99, "resetTime": "2026-12-01T11:54:24Z" }
                        ]
                    },
                    {
                        "displayName": "Gemini 4 Argon",
                        "description": "No dedicated quota",
                        "buckets": []
                    },
                    {
                        "displayName": "Claude and GPT models",
                        "description": "3P models",
                        "buckets": [
                            { "bucketId": "3p-5h", "displayName": "5h", "window": "5h", "remainingFraction": 1.0, "resetTime": "2026-11-24T16:59:13Z" },
                            { "bucketId": "3p-weekly", "displayName": "Weekly", "window": "weekly", "remainingFraction": 0.49, "resetTime": "2026-11-28T07:37:58Z" }
                        ]
                    }
                ]
            }
        }"#).unwrap();

        let groups = parse_rpc_quota_summary(&summary, now);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "gemini");
        assert_eq!(groups[0].tab_label, "Gemini");
        assert_eq!(groups[0].five_hour_bucket.as_ref().unwrap().percentage, 98.0);
        assert_eq!(groups[0].weekly_bucket.as_ref().unwrap().percentage, 99.0);

        assert_eq!(groups[1].id, "claude");
        assert_eq!(groups[1].tab_label, "Claude / GPT");
        assert_eq!(groups[1].five_hour_bucket.as_ref().unwrap().percentage, 100.0);
        assert_eq!(groups[1].weekly_bucket.as_ref().unwrap().percentage, 49.0);
    }

    #[test]
    fn test_parse_rpc_quota_summary_legacy_3_groups() {
        let now = "2026-11-21T01:05:00Z".parse::<DateTime<Utc>>().unwrap();
        let summary: serde_json::Value = serde_json::from_str(r#"{
            "response": {
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "buckets": [
                            { "bucketId": "gemini-5h", "window": "5h", "remainingFraction": 0.45, "resetTime": "2026-11-21T02:29:02Z" },
                            { "bucketId": "gemini-weekly", "window": "weekly", "remainingFraction": 0.65, "resetTime": "2026-11-25T11:23:39Z" }
                        ]
                    },
                    {
                        "displayName": "Gemini 4 Argon",
                        "buckets": [
                            { "bucketId": "gemini-argon-wtu-weekly", "window": "weekly", "remainingFraction": 0.18, "resetTime": "2026-11-25T11:15:13Z" }
                        ]
                    },
                    {
                        "displayName": "Claude and GPT models",
                        "buckets": [
                            { "bucketId": "3p-5h", "window": "5h", "remainingFraction": 0.90, "resetTime": "2026-11-21T06:04:58Z" },
                            { "bucketId": "3p-weekly", "window": "weekly", "remainingFraction": 0.85, "resetTime": "2026-11-28T01:04:58Z" }
                        ]
                    }
                ]
            }
        }"#).unwrap();

        let groups = parse_rpc_quota_summary(&summary, now);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].id, "gemini");
        assert_eq!(groups[1].id, "argon");
        assert_eq!(groups[1].tab_label, "Argon");
        assert!(groups[1].five_hour_bucket.is_none());
        assert_eq!(groups[1].weekly_bucket.as_ref().unwrap().percentage, 18.0);
        assert_eq!(groups[2].id, "claude");
    }

    #[test]
    fn test_app_config_ignores_legacy_tab_fields() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"autostart_enabled": true, "selected_tab": "argon", "active_group": "argon"}"#,
        )
        .unwrap();
        assert_eq!(cfg.autostart_enabled, Some(true));
    }

    #[test]
    fn test_parse_agy_usage_json_errors() {
        let now = "2026-11-21T01:05:00Z".parse::<DateTime<Utc>>().unwrap();

        assert!(parse_agy_usage_json("{invalid", now, "10:00:00".to_string()).is_err());
        assert!(parse_agy_usage_json(r#"{"status":"ERROR","command":{"data":{"groups":[]}}}"#, now, "10:00:00".to_string()).is_err());
        assert!(parse_agy_usage_json(r#"{"status":"SUCCESS","command":{"data":{}}}"#, now, "10:00:00".to_string()).is_err());
        assert!(parse_agy_usage_json(
            r#"{"status":"SUCCESS","command":{"data":{"groups":[{"name":"X","buckets":[{"id":"unknown-only","remaining_fraction":0.5}]}]}}}"#,
            now,
            "10:00:00".to_string()
        ).is_err());
    }

    #[test]
    fn test_resolve_agy_path_with() {
        let base_dir = std::env::temp_dir().join(format!("galm_agy_path_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base_dir);

        let local_app = base_dir.join("local");
        let local_bin = local_app.join("agy").join("bin");
        let path_dir = base_dir.join("path_dir");
        fs::create_dir_all(&local_bin).unwrap();
        fs::create_dir_all(&path_dir).unwrap();

        let local_exe = local_bin.join(AGY_BINARY_NAME);
        let path_exe = path_dir.join(AGY_BINARY_NAME);

        // どちらも存在しない場合は None
        let path_os = std::env::join_paths([&path_dir]).unwrap();
        assert_eq!(resolve_agy_path_with(Some(&local_app), Some(path_os.as_os_str())), None);

        // PATH にのみ存在する場合は PATH 解決
        fs::write(&path_exe, b"").unwrap();
        assert_eq!(
            resolve_agy_path_with(Some(&local_app), Some(path_os.as_os_str())),
            Some(path_exe.clone())
        );

        // %LOCALAPPDATA%\agy\bin\agy.exe が存在する場合はこちらを優先
        fs::write(&local_exe, b"").unwrap();
        assert_eq!(
            resolve_agy_path_with(Some(&local_app), Some(path_os.as_os_str())),
            Some(local_exe)
        );

        let _ = fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_resolve_quota_with_fallback_priority_and_recovery() {
        let make_status = |connected: bool, source: &str, msg: &str, credits: Option<i64>| QuotaStatus {
            connected,
            source: source.to_string(),
            status_message: msg.to_string(),
            plan_name: if source == "rpc" { "Pro" } else { "―" }.to_string(),
            prompt_credits: credits,
            email: String::new(),
            updated_at: "10:00:00".to_string(),
            groups: Vec::new(),
            models: Vec::new(),
        };

        // 1. RPC 成功時は agy を呼ばず RPC を返す
        let mut agy_called = false;
        let res1 = resolve_quota_with_fallback(
            || Ok(make_status(true, "rpc", "正常稼働中", Some(120))),
            || {
                agy_called = true;
                Ok(make_status(true, "agy", "agy 経由で取得中", None))
            },
        );
        assert!(!agy_called);
        assert_eq!(res1.source, "rpc");
        assert_eq!(res1.prompt_credits, Some(120));

        // 2. RPC 失敗時は agy へフォールバック
        let res2 = resolve_quota_with_fallback(
            || Err(make_status(false, "none", "Language Server 待機中...", None)),
            || Ok(make_status(true, "agy", "agy 経由で取得中", None)),
        );
        assert!(res2.connected);
        assert_eq!(res2.source, "agy");
        assert_eq!(res2.prompt_credits, None);

        // 3. RPC も agy も失敗時は現行の待機表示に戻る
        let res3 = resolve_quota_with_fallback(
            || Err(make_status(false, "none", "Language Server 待機中...", None)),
            || Err("agy 実行失敗".to_string()),
        );
        assert!(!res3.connected);
        assert_eq!(res3.source, "none");
        assert_eq!(res3.status_message, "Language Server 待機中...");

        // 4. Language Server が復帰すれば自動で RPC へ戻る
        let res4 = resolve_quota_with_fallback(
            || Ok(make_status(true, "rpc", "正常稼働中", Some(250))),
            || Ok(make_status(true, "agy", "agy 経由で取得中", None)),
        );
        assert!(res4.connected);
        assert_eq!(res4.source, "rpc");
        assert_eq!(res4.prompt_credits, Some(250));
    }

    #[test]
    #[ignore]
    fn test_fetch_quota_via_agy_live() {
        let now_utc = Utc::now();
        let updated_at = Local::now().format("%H:%M:%S").to_string();
        let status = fetch_quota_via_agy(now_utc, &updated_at).expect("live agy fetch should succeed");
        assert!(status.connected);
        assert_eq!(status.source, "agy");
        assert!(!status.groups.is_empty());
        for g in &status.groups {
            assert!(g.five_hour_bucket.is_some() || g.weekly_bucket.is_some());
            println!(
                "[agy] id={} tab={} group={} 5h={:?} weekly={:?}",
                g.id,
                g.tab_label,
                g.display_name,
                g.five_hour_bucket.as_ref().map(|b| (b.bucket_id.as_str(), b.percentage)),
                g.weekly_bucket.as_ref().map(|b| (b.bucket_id.as_str(), b.percentage))
            );
        }
    }

    #[test]
    #[ignore]
    fn test_fetch_quota_via_rpc_live() {
        let now_utc = Utc::now();
        let updated_at = Local::now().format("%H:%M:%S").to_string();
        let status = fetch_quota_via_rpc(now_utc, &updated_at).expect("live rpc fetch should succeed");
        assert!(status.connected);
        assert_eq!(status.source, "rpc");
        assert!(!status.groups.is_empty());
        println!(
            "[rpc] plan={} credits={:?} groups={} models={}",
            status.plan_name,
            status.prompt_credits,
            status.groups.len(),
            status.models.len()
        );
        for g in &status.groups {
            assert!(g.five_hour_bucket.is_some() || g.weekly_bucket.is_some());
            println!(
                "[rpc] id={} tab={} group={} 5h={:?} weekly={:?}",
                g.id,
                g.tab_label,
                g.display_name,
                g.five_hour_bucket.as_ref().map(|b| (b.bucket_id.as_str(), b.percentage)),
                g.weekly_bucket.as_ref().map(|b| (b.bucket_id.as_str(), b.percentage))
            );
        }
        for m in &status.models {
            println!("[rpc-model] label={} id={} pct={}", m.label, m.model_id, m.percentage);
        }
    }
}
