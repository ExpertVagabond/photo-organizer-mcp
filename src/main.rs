use serde::Deserialize;
use serde_json::{Value, json};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// Shell metacharacters that must never appear in arguments passed to commands.
const SHELL_METACHARACTERS: &[char] = &[';', '|', '&', '$', '`', '\\', '(', ')', '{', '}', '<', '>', '!', '\n', '\r', '\0'];

/// Allowed tool names -- reject anything not on this list.
const ALLOWED_TOOLS: &[&str] = &[
    "analyze_photos",
    "organize_photos_by_date",
    "analyze_drive",
    "organize_drive",
    "archive_old_files",
    "deduplicate_drive",
];

/// Allowed Python script names -- reject anything not on this list.
const ALLOWED_SCRIPTS: &[&str] = &["photos_organizer.py", "drive_organizer.py"];

/// Validate that a path is safe: absolute, no `..` components, and exists on disk.
fn validate_path(p: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(p);
    if !path.is_absolute() {
        return Err("Scripts path must be absolute".to_string());
    }
    // Canonicalize resolves symlinks and `..` -- if the raw string contains
    // `..` we reject it outright so callers cannot sneak traversal in.
    if p.contains("..") {
        return Err("Scripts path must not contain '..'".to_string());
    }
    let canon = path
        .canonicalize()
        .map_err(|_| format!("Scripts path does not exist or is inaccessible: {p}"))?;
    Ok(canon)
}

/// Sanitize a single shell argument -- reject if it contains dangerous chars.
fn sanitize_arg(arg: &str) -> Result<(), String> {
    if arg.chars().any(|c| SHELL_METACHARACTERS.contains(&c)) {
        return Err(format!("Argument contains forbidden characters: {}", arg.replace(|c: char| c.is_control(), "?")));
    }
    Ok(())
}

fn scripts_path() -> Result<PathBuf, String> {
    let raw = std::env::var("PHOTO_SCRIPTS_PATH").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/drive-photos-organizer")
    });
    validate_path(&raw)
}

fn run_python(script: &str, args: &[&str]) -> Result<String, String> {
    // Validate script name against allowlist
    if !ALLOWED_SCRIPTS.contains(&script) {
        return Err("Invalid script name".to_string());
    }

    // Validate all arguments
    for arg in args {
        sanitize_arg(arg)?;
    }

    let base = scripts_path()?;
    let script_path = base.join(script);

    // Ensure the resolved script path is still inside the base directory
    let canon_script = script_path
        .canonicalize()
        .map_err(|_| "Script not found".to_string())?;
    if !canon_script.starts_with(&base) {
        return Err("Script path escapes the scripts directory".to_string());
    }

    let output = Command::new("python3")
        .arg(&canon_script)
        .args(args)
        .current_dir(&base)
        .output()
        .map_err(|_| "Failed to run python3".to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if !output.status.success() {
        // Sanitize error: do not expose raw stderr to callers
        return Err("Script execution failed — check server logs for details".to_string());
    }

    let mut result = stdout;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.is_empty() {
        // Log stderr server-side but only return a generic note to the caller
        tracing::warn!("Script stderr: {stderr}");
        result.push_str("\n\n(script produced warnings — see server logs)");
    }
    Ok(result)
}

fn tool_definitions() -> Value {
    json!([
        {"name": "analyze_photos", "description": "Analyze Google Photos library - stats, duplicates, report", "inputSchema": {"type": "object", "properties": {"findDuplicates": {"type": "boolean", "default": true}}}},
        {"name": "organize_photos_by_date", "description": "Organize Google Photos into albums by date", "inputSchema": {"type": "object", "properties": {"grouping": {"type": "string", "enum": ["year", "month"], "default": "year"}, "execute": {"type": "boolean", "default": false}}, "required": ["grouping"]}},
        {"name": "analyze_drive", "description": "Analyze Google Drive - file stats, duplicates, report", "inputSchema": {"type": "object", "properties": {"findDuplicates": {"type": "boolean", "default": true}}}},
        {"name": "organize_drive", "description": "Organize Google Drive files into folders by type", "inputSchema": {"type": "object", "properties": {"execute": {"type": "boolean", "default": false}}}},
        {"name": "archive_old_files", "description": "Move old Drive files to Archive folder", "inputSchema": {"type": "object", "properties": {"days": {"type": "number", "default": 365}, "execute": {"type": "boolean", "default": false}}}},
        {"name": "deduplicate_drive", "description": "Remove exact duplicate files from Google Drive", "inputSchema": {"type": "object", "properties": {"execute": {"type": "boolean", "default": false}}}}
    ])
}

fn call_tool(name: &str, args: &Value) -> Result<Value, String> {
    // Validate tool name against allowlist before dispatching
    if !ALLOWED_TOOLS.contains(&name) {
        return Err("Unknown or disallowed tool".to_string());
    }

    let result = match name {
        "analyze_photos" => {
            let mut script_args = vec!["--report"];
            if args["findDuplicates"].as_bool().unwrap_or(true) {
                script_args.push("--export-duplicates");
            }
            run_python("photos_organizer.py", &script_args)?
        }
        "organize_photos_by_date" => {
            let grouping = args["grouping"].as_str().unwrap_or("year");
            let execute = args["execute"].as_bool().unwrap_or(false);
            let mut script_args = vec![if grouping == "year" {
                "--by-year"
            } else {
                "--by-month"
            }];
            if execute {
                script_args.push("--execute");
            }
            run_python("photos_organizer.py", &script_args)?
        }
        "analyze_drive" => run_python("drive_organizer.py", &["--report"])?,
        "organize_drive" => {
            let execute = args["execute"].as_bool().unwrap_or(false);
            let mut a = vec!["--organize"];
            if execute {
                a.push("--execute");
            }
            run_python("drive_organizer.py", &a)?
        }
        "archive_old_files" => {
            let days = args["days"].as_u64().unwrap_or(365);
            let execute = args["execute"].as_bool().unwrap_or(false);
            let days_str = days.to_string();
            let mut a = vec!["--archive", "--days", &days_str];
            if execute {
                a.push("--execute");
            }
            run_python("drive_organizer.py", &a)?
        }
        "deduplicate_drive" => {
            let execute = args["execute"].as_bool().unwrap_or(false);
            let mut a = vec!["--dedupe"];
            if execute {
                a.push("--execute");
            }
            run_python("drive_organizer.py", &a)?
        }
        // Already validated against ALLOWED_TOOLS above, but keep an exhaustive fallback
        _ => return Err("Unknown or disallowed tool".to_string()),
    };
    Ok(json!({"output": result}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_writer(std::io::stderr)
        .init();
    info!("photo-organizer-mcp starting on stdio");
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let response = match req.method.as_str() {
            "initialize" => {
                json!({"jsonrpc":"2.0","id":req.id,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"photo-organizer-mcp","version":"0.1.0"}}})
            }
            "notifications/initialized" => continue,
            "tools/list" => {
                json!({"jsonrpc":"2.0","id":req.id,"result":{"tools":tool_definitions()}})
            }
            "tools/call" => {
                let tn = req.params["name"].as_str().unwrap_or("");
                let a = &req.params["arguments"];
                match call_tool(tn, a) {
                    Ok(r) => {
                        json!({"jsonrpc":"2.0","id":req.id,"result":{"content":[{"type":"text","text":serde_json::to_string_pretty(&r).unwrap_or_default()}]}})
                    }
                    Err(e) => {
                        json!({"jsonrpc":"2.0","id":req.id,"result":{"content":[{"type":"text","text":format!("Error: {e}")}],"isError":true}})
                    }
                }
            }
            _ => {
                json!({"jsonrpc":"2.0","id":req.id,"error":{"code":-32601,"message":format!("Unknown method: {}",req.method)}})
            }
        };
        use std::io::Write;
        let out = serde_json::to_string(&response).unwrap();
        let mut lock = stdout.lock();
        let _ = writeln!(lock, "{out}");
        let _ = lock.flush();
    }
}
