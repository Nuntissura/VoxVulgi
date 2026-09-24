use std::path::PathBuf;
use voxvulgi_engine::offline_payload_validation::{
    self, ExternalSourceLockRequest, ValidationRequest,
};

fn main() -> std::result::Result<(), String> {
    run().map_err(|error| error.to_string())
}

fn run() -> voxvulgi_engine::Result<()> {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|value| value == "-h" || value == "--help")
    {
        print_help();
        return Ok(());
    }
    let mut stage_base_dir = None;
    let mut export_dir = None;
    let mut receipt_path = None;
    let mut app_version = None;
    let mut source_lock_path = None;
    let mut source_lock_owner_pid = None;
    let mut source_lock_token_sha256 = None;
    let mut source_lock_record_sha256 = None;
    let mut transaction_id = None;
    let mut index = 1;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        index += 1;
        let value = arguments.get(index).ok_or_else(|| {
            voxvulgi_engine::EngineError::InstallFailed(format!("{flag} requires a value"))
        })?;
        match flag {
            "--stage-base-dir" => stage_base_dir = Some(PathBuf::from(value)),
            "--export-dir" => export_dir = Some(PathBuf::from(value)),
            "--receipt" => receipt_path = Some(PathBuf::from(value)),
            "--app-version" => app_version = Some(value.clone()),
            "--source-lock-path" => source_lock_path = Some(PathBuf::from(value)),
            "--source-lock-owner-pid" => {
                source_lock_owner_pid = Some(value.parse::<u32>().map_err(|_| {
                    voxvulgi_engine::EngineError::InstallFailed(
                        "--source-lock-owner-pid must be an unsigned integer".to_string(),
                    )
                })?)
            }
            "--source-lock-token-sha256" => source_lock_token_sha256 = Some(value.clone()),
            "--source-lock-record-sha256" => source_lock_record_sha256 = Some(value.clone()),
            "--transaction-id" => transaction_id = Some(value.clone()),
            other => {
                return Err(voxvulgi_engine::EngineError::InstallFailed(format!(
                    "unknown argument: {other}"
                )))
            }
        }
        index += 1;
    }
    let required = |value: Option<PathBuf>, name: &str| {
        value.ok_or_else(|| {
            voxvulgi_engine::EngineError::InstallFailed(format!("missing required {name}"))
        })
    };
    let external_source_lock = match (
        source_lock_path,
        source_lock_owner_pid,
        source_lock_token_sha256,
        source_lock_record_sha256,
        transaction_id,
    ) {
        (None, None, None, None, None) => None,
        (
            Some(path),
            Some(owner_pid),
            Some(token_sha256),
            Some(record_sha256),
            Some(transaction_id),
        ) => Some(ExternalSourceLockRequest {
            path,
            owner_pid,
            transaction_id,
            token_sha256,
            record_sha256,
        }),
        _ => {
            return Err(voxvulgi_engine::EngineError::InstallFailed(
                "external source-lock arguments are all-or-none".to_string(),
            ))
        }
    };
    let receipt = offline_payload_validation::validate_and_write(ValidationRequest {
        stage_base_dir: required(stage_base_dir, "--stage-base-dir")?,
        export_dir: required(export_dir, "--export-dir")?,
        receipt_path: required(receipt_path, "--receipt")?,
        app_version: app_version.ok_or_else(|| {
            voxvulgi_engine::EngineError::InstallFailed(
                "missing required --app-version".to_string(),
            )
        })?,
        external_source_lock,
    })?;
    println!(
        "offline payload validated: schema={} bundle={} receipt={}",
        receipt.schema, receipt.manifest.bundle_id, receipt.inputs.payload_dir
    );
    Ok(())
}

fn print_help() {
    let lines = [
        "VoxVulgi fresh full-offline payload validator",
        "",
        "Usage:",
        "  voxvulgi_offline_payload_validate",
        "    --stage-base-dir <fresh-prepared-appdata-root>",
        "    --export-dir <fresh-exported-payload-root>",
        "    --receipt <new-receipt.json>",
        "    --app-version <desktop-version>",
        "    [--source-lock-path <canonical-lock> --source-lock-owner-pid <pid>",
        "     --source-lock-token-sha256 <sha256> --source-lock-record-sha256 <sha256>",
        "     --transaction-id <id>]",
        "",
        "The validator holds or attests the exclusive source lock, runs with offline guards,",
        "refuses to overwrite a receipt, and emits only after the final rehash passes.",
    ];
    println!("{}", lines.join("\n"));
}
