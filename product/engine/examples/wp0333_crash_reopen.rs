//! Owned-process crash fixture. Controller supplies a fresh guarded database copy.
//! No runner, production database, TLS filename adapter, or graceful shutdown.
use rusqlite::{config::DbConfig, Connection, TransactionBehavior};
use serde_json::json;
use std::{io::{self, Write}, path::PathBuf};
use voxvulgi_engine::{db, paths::AppPaths};

fn emit(value: serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    println!("{value}");
    io::stdout().flush()?;
    Ok(())
}
fn command(expected: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    if line.trim() != expected { return Err("Controller command mismatch".into()); }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 { return Err("Usage: <fresh-root> baseline|full_no_close_no_auto <writes1..128>".into()); }
    let root = PathBuf::from(&args[1]).canonicalize()?;
    let mode = &args[2];
    let writes: usize = args[3].parse()?;
    if !["baseline", "full_no_close_no_auto"].contains(&mode.as_str()) || !(1..=128).contains(&writes) {
        return Err("Invalid mode/write count".into());
    }
    if !root.join("wp0333_crash_fixture.json").is_file() { return Err("Missing guarded fixture marker".into()); }
    for variable in ["APPDATA", "LOCALAPPDATA"] {
        let protected = PathBuf::from(std::env::var_os(variable).ok_or("Missing appdata guard")?).join("com.voxvulgi.voxvulgi");
        let protected = if protected.exists() { protected.canonicalize()? } else { protected };
        if root.starts_with(&protected) || protected.starts_with(&root) { return Err("Protected root refused".into()); }
    }
    let paths = AppPaths::new(root.clone());
    let path = paths.db_dir().join("app.sqlite");
    if !path.is_file() { return Err("Controller must supply completed backup copy".into()); }
    let setup = Connection::open(db::AppDatabase::sqlite_open_filename(&path))?;
    setup.pragma_update(None, "journal_mode", "WAL")?;
    db::migrate(&setup)?;
    let collision: i64 = setup.query_row("SELECT count(*) FROM meta WHERE key GLOB 'wp0333_crash_*'", [], |r|r.get(0))?;
    if collision != 0 { return Err("Fixture prefix already exists".into()); }
    let sqlite_identity: (String,String) = setup.query_row("SELECT sqlite_version(),sqlite_source_id()", [], |r|Ok((r.get(0)?,r.get(1)?)))?;
    drop(setup);
    emit(json!({"event":"prepared","pid":std::process::id(),"root":root,"mode":mode,"sqlite_identity":sqlite_identity}))?;
    command("start")?;
    let database = db::AppDatabase::for_paths(&paths)?;
    let keeper = database.read_context(db::DatabaseOperationContext::new("wp0333_crash", "pinned_reader"))?;
    keeper.execute_batch("BEGIN")?;
    let _: i64 = keeper.query_row("SELECT count(*) FROM meta", [], |r|r.get(0))?;
    for ordinal in 0..writes {
        let key = format!("wp0333_crash_ack_{ordinal:04}");
        let payload = format!("canonical-commit-{ordinal:04}-{}", "x".repeat(8192));
        let mut writer = database.write_context(db::DatabaseOperationContext::new("wp0333_crash", "acknowledged_commit"))?;
        if mode == "full_no_close_no_auto" {
            writer.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
            writer.pragma_update(None,"synchronous","FULL")?;
            writer.pragma_update(None,"wal_autocheckpoint",0)?;
        }
        let sync: i64 = writer.pragma_query_value(None,"synchronous",|r|r.get(0))?;
        let auto: i64 = writer.pragma_query_value(None,"wal_autocheckpoint",|r|r.get(0))?;
        let no_close = writer.db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE)?;
        if mode == "full_no_close_no_auto" && (sync != 2 || auto != 0 || !no_close) { return Err("Policy readback mismatch".into()); }
        let tx = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO meta(key,value) VALUES(?1,?2)", rusqlite::params![key,payload])?;
        tx.commit()?;
        drop(writer);
        emit(json!({"event":"ack","ordinal":ordinal,"key":key,"payload":payload,"synchronous":sync,"wal_autocheckpoint":auto,"no_checkpoint_on_close":no_close}))?;
    }
    let mut pending = database.write_context(db::DatabaseOperationContext::new("wp0333_crash", "uncommitted_boundary"))?;
    let tx = pending.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("INSERT INTO meta(key,value) VALUES('wp0333_crash_uncommitted',?1)", ["y".repeat(2_000_000)])?;
    emit(json!({"event":"crash_ready","pid":std::process::id(),"acknowledged":writes,"pending_transaction_open":true,"runtime":database.snapshot()}))?;
    // Controller must terminate its own child here. EOF or a command is a failure;
    // never manufacture a successful crash using a graceful connection drop.
    command("controller_must_kill_owned_child")?;
    Err("Process was not crashed at the armed boundary".into())
}
