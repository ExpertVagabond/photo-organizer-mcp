use serde::Deserialize;
use serde_json::{Value, json};
use std::io::BufRead;
use std::process::Command;
use tracing::info;

#[derive(Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

fn scripts_path() -> String {
    std::env::var("PHOTO_SCRIPTS_PATH").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/drive-photos-organizer")
    })
}

fn run_python(script: &str, args: &[&str]) -> Result<String, String> {
    let base = scripts_path();
    let script_path = format!("{base}/{script}");
    let output = Command::new("python3")
        .arg(&script_path)
        .args(args)
        .current_dir(&base)
        .output()
        .map_err(|e| format!("Failed to run python3: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("Script failed: {stderr}\n{stdout}"));
    }
    let mut result = stdout;
    if !stderr.is_empty() {
        result.push_str(&format!("\n\nWarnings:\n{stderr}"));
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
        _ => return Err(format!("Unknown tool: {name}")),
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
