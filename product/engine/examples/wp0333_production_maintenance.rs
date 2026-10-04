//! Actual production maintenance owner on a fresh canonical-density schema61 copy.
//! Observer-only per-thread VFS timing; no runner, policy overrides, filename
//! adapters or maintenance-only SQLite connection.
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}, sync::{Arc, Barrier, mpsc}, thread, time::{Duration, Instant}};
use voxvulgi_engine::{db, paths::AppPaths};
type ProofResult<T> = Result<T, Box<dyn std::error::Error>>;
const PREFIX: &str = "wp0333_production_fixture_";
fn hash(path: &Path) -> ProofResult<String> {
    let mut file=fs::File::open(path)?; let mut digest=Sha256::new(); let mut buffer=[0;65536];
    loop { let n=file.read(&mut buffer)?; if n==0 {break} digest.update(&buffer[..n]); }
    Ok(hex::encode(digest.finalize()))
}
fn guard(path: &Path) -> ProofResult<()> {
    for key in ["APPDATA","LOCALAPPDATA"] {
        let protected=PathBuf::from(std::env::var_os(key).ok_or("Appdata guard missing")?).join("com.voxvulgi.voxvulgi");
        let protected=if protected.exists(){protected.canonicalize()?}else{protected};
        if path.starts_with(&protected)||protected.starts_with(path){return Err("Protected root refused".into())}
    }
    Ok(())
}
fn emit(value: Value) { println!("{value}"); let _=std::io::stdout().flush(); }
fn failure(database: &db::AppDatabase,index: usize,ordinal: usize,error: &str,measurement: &Value,events: &mut Vec<Value>,overflow: &mut usize) {
    let at_ms=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    let mut event=json!({"event":"operation_error","at_ms":at_ms,"index":index,"ordinal":ordinal,
        "request_id":format!("wp0333-production-{index}-{ordinal}"),"operation":if index<3{"job_insert"}else{"job_read"},"error":error,"measurement":measurement});
    if events.len()<32 { event["immediate_runtime"]=serde_json::to_value(database.snapshot()).expect("runtime snapshot serialization");events.push(event.clone()); }
    else { *overflow+=1;event["snapshot_omitted"]=json!("bounded_32_per_worker_limit"); }
    emit(event);
}
fn insert(connection: &Connection,id: &str,payload: &str)->rusqlite::Result<usize>{
    connection.execute("INSERT INTO job(id,item_id,batch_id,type,status,progress,error,params_json,created_at_ms,started_at_ms,finished_at_ms,logs_path,lane,track,target_key,attempt_no) VALUES(?1,NULL,NULL,'download_direct_url','queued',0,NULL,?2,?3,NULL,NULL,?4,'recurring','youtube_recurring',NULL,1)",rusqlite::params![id,payload,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64,format!("{id}.jsonl")])
}
fn main()->ProofResult<()> {
    let args:Vec<_>=std::env::args_os().collect();
    if args.len()!=4 && args.len()!=6 && args.len()!=7{return Err("Usage: <absent-absolute-root> <standalone-schema61-backup> <expected-source-sha256> [count|pk seconds30..180 [connection_reuse]]".into())}
    let reuse=args.len()==7;
    let production_reuse=reuse&&args[6]=="production_connection_reuse";
    if reuse && !production_reuse && args[6]!="connection_reuse" {return Err("Require connection_reuse or production_connection_reuse mode".into())}
    #[cfg(not(feature="wp0333_connection_reuse_proof"))]
    if reuse&&!production_reuse {return Err("Diagnostic connection reuse feature is not enabled".into())}
    let workload=if args.len()>=6 {args[4].to_str().ok_or("Invalid workload")?}else{"count"};
    if !matches!(workload,"count"|"pk"){return Err("Require count or pk workload".into())}
    let seconds=if args.len()>=6 {args[5].to_str().ok_or("Invalid seconds")?.parse::<u64>()?}else{30};
    if !(30..=180).contains(&seconds){return Err("Require seconds30..180".into())}
    let requested=PathBuf::from(&args[1]);
    if !requested.is_absolute()||requested.exists(){return Err("Require fresh absent absolute root".into())}
    let root=requested.parent().ok_or("Parent missing")?.canonicalize()?.join(requested.file_name().ok_or("Filename missing")?);
    let source=PathBuf::from(&args[2]).canonicalize()?; guard(&root)?; guard(&source)?;
    if root.starts_with(source.parent().ok_or("Backup parent missing")?){return Err("Fixture cannot be inside backup parent".into())}
    for suffix in ["-wal","-shm","-journal"]{if PathBuf::from(format!("{}{suffix}",source.display())).exists(){return Err("Source must be completed standalone backup".into())}}
    let source_hash=hash(&source)?;
    if args[3].to_str()!=Some(source_hash.as_str()){return Err("Source hash mismatch".into())}
    let mut uri=url::Url::from_file_path(&source).map_err(|_|"Invalid source URI")?;
    uri.query_pairs_mut().append_pair("mode","ro").append_pair("immutable","1");
    let original=Connection::open_with_flags(uri.as_str(),OpenFlags::SQLITE_OPEN_READ_ONLY|OpenFlags::SQLITE_OPEN_URI)?;
    let schema:u32=original.pragma_query_value(None,"user_version",|r|r.get(0))?;
    let density:i64=original.query_row("SELECT COUNT(*) FROM job",[],|r|r.get(0))?;
    let collision:i64=original.query_row("SELECT COUNT(*) FROM job WHERE id GLOB 'wp0333_production_fixture_*'",[],|r|r.get(0))?;
    let payload:String=original.query_row("SELECT params_json FROM job WHERE type='download_direct_url' AND status='queued' LIMIT 1",[],|r|r.get(0))?;
    let read_job:String=original.query_row("SELECT id FROM job ORDER BY id LIMIT 1",[],|r|r.get(0))?;
    let read_meta:String=original.query_row("SELECT key FROM meta ORDER BY key LIMIT 1",[],|r|r.get(0))?;
    if schema!=61||density<1000||collision!=0{return Err("Require canonical-density schema61 without fixture prefix".into())}
    drop(original); fs::create_dir(&root)?; fs::create_dir(root.join("db"))?;
    let destination=root.join("db/app.sqlite"); fs::copy(&source,&destination)?;
    if hash(&source)?!=source_hash||hash(&destination)?!=source_hash{return Err("Copy identity mismatch".into())}
    fs::write(root.join("wp0333_production_fixture.json"),serde_json::to_vec_pretty(&json!({"root":root,"source":source,"source_sha256":source_hash,"schema":schema,"job_density":density,"pid":std::process::id(),"read_workload":workload,"seconds":seconds,"read_job":read_job,"read_meta_key":read_meta}))?)?;
    let database=db::AppDatabase::for_paths(&AppPaths::new(root.clone()))?;
    #[cfg(feature="wp0333_connection_reuse_proof")]
    let _reuse_guard=if reuse&&!production_reuse {Some(database.enable_disposable_connection_reuse(&root,&source_hash)?)} else {None};
    let mut maintenance=database.start_checkpoint_maintenance()?;
    let mut production_guard=if production_reuse {Some(database.start_connection_reuse()?)} else {None};
    let (pin_ready_tx,pin_ready_rx)=mpsc::channel();
    let pinned_database=database.clone();
    let pin=thread::spawn(move||->Result<(),String>{
        let connection=pinned_database.read_context(db::DatabaseOperationContext::new("wp0333_production","pinned_read")).map_err(|e|e.to_string())?;
        connection.execute_batch("BEGIN").map_err(|e|e.to_string())?;
        let _:i64=connection.query_row("SELECT COUNT(*) FROM job",[],|r|r.get(0)).map_err(|e|e.to_string())?;
        pin_ready_tx.send(()).map_err(|e|e.to_string())?;
        thread::sleep(Duration::from_secs(5));
        connection.execute_batch("COMMIT").map_err(|e|e.to_string())?; drop(connection); Ok(())
    });
    pin_ready_rx.recv_timeout(Duration::from_secs(3))?;
    let barrier=Arc::new(Barrier::new(7)); let deadline=Instant::now()+Duration::from_secs(seconds);
    let mut workers=Vec::new();
    for index in 0..6 {
        let database=database.clone(); let barrier=barrier.clone(); let payload=payload.clone();
        let read_job=read_job.clone();let read_meta=read_meta.clone();let workload=workload.to_owned();
        workers.push(thread::spawn(move|| {
            let mut acknowledged=Vec::new();let mut errors=Vec::new();let mut reads=0u64;let mut ordinal=0;
            let mut error_events=Vec::new();let mut error_snapshot_overflow=0;
            let mut attempt_measurements=Vec::new();let mut attempt_measurement_overflow=0;
            barrier.wait();
            while Instant::now()<deadline {
                ordinal+=1;
                let context=db::DatabaseOperationContext::new(format!("wp0333_production_{index}"),if index<3{"job_insert"}else{"job_read"})
                    .with_request_id(format!("wp0333-production-{index}-{ordinal}"));
                let id=format!("{PREFIX}{index}_{ordinal}");
                let started_at_ms=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
                let attempt_started=Instant::now();
                db::AppDatabase::begin_vfs_timing_probe();
                let result=if index<3 {
                    (||->voxvulgi_engine::Result<()> {let connection=database.write_context(context)?;insert(&connection,&id,&payload)?;drop(connection);Ok(())})()
                }else{
                    database.read(context,|connection|{
                        if workload=="count" {let _:i64=connection.query_row("SELECT COUNT(*) FROM job WHERE type='download_direct_url'",[],|r|r.get(0))?;}
                        else if index==3 {let _:String=connection.query_row("SELECT value FROM meta WHERE key=?1",[&read_meta],|r|r.get(0))?;}
                        else {let _:String=connection.query_row("SELECT status FROM job WHERE id=?1",[&read_job],|r|r.get(0))?;}
                        Ok(())
                    })
                };
                let elapsed_ns=attempt_started.elapsed().as_nanos();
                let mut callbacks=db::AppDatabase::finish_vfs_timing_probe().to_vec();
                callbacks.extend(db::AppDatabase::vfs_open_read_timing_probe());
                let finished_at_ms=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
                // Context returned or physically closed before measurement serialization.
                let measurement=json!({"request_id":format!("wp0333-production-{index}-{ordinal}"),"started_at_ms":started_at_ms,
                    "finished_at_ms":finished_at_ms,"elapsed_ns":elapsed_ns,"callbacks":callbacks,"succeeded":result.is_ok()});
                if attempt_measurements.len()<2048 {attempt_measurements.push(measurement.clone());}else{attempt_measurement_overflow+=1;}
                match result {
                    Ok(()) if index<3=>{emit(json!({"event":"ack","job_id":id,"at_ms":finished_at_ms}));acknowledged.push(id)},
                    Ok(())=>reads+=1,
                    Err(e)=>{let error=e.to_string();failure(&database,index,ordinal,&error,&measurement,&mut error_events,&mut error_snapshot_overflow);errors.push(error)}
                }
                thread::sleep(Duration::from_millis(100));
            }
            json!({"index":index,"acknowledged_ids":acknowledged,"reads":reads,"errors":errors,"error_events":error_events,"error_snapshot_overflow":error_snapshot_overflow,
                "attempt_measurements":attempt_measurements,"attempt_measurement_overflow":attempt_measurement_overflow})
        }));
    }
    barrier.wait(); let started=Instant::now();let mut observations=Vec::new();let mut manual=Vec::new();
    let mut during_pin=false;let mut after_pin=false;
    while Instant::now()<deadline {
        let elapsed=started.elapsed().as_millis() as u64;
        if elapsed>=1500&&!during_pin {manual.push(json!({"phase":"during_pin","receipt":database.checkpoint_passive()?}));during_pin=true;}
        if elapsed>=7000&&!after_pin {manual.push(json!({"phase":"after_pin","receipt":database.checkpoint_passive()?}));after_pin=true;}
        let runtime=database.snapshot();
        observations.push(json!({"elapsed_ms":elapsed,"health":database.wal_health(),"runtime":{"active_readers":runtime.active_readers,"waiting_readers":runtime.waiting_readers,"writer_active":runtime.writer_active,"waiting_writers":runtime.waiting_writers}}));
        thread::sleep(Duration::from_millis(100));
    }
    let reports:Vec<_>=workers.into_iter().map(|w|w.join().map_err(|_|"Worker panicked")).collect::<Result<_,_>>()?;
    pin.join().map_err(|_|"Pinned reader panicked")?.map_err(|e|format!("Pinned reader failed: {e}"))?;
    let shutdown_deadline=Instant::now()+db::SHUTDOWN_DRAIN_TIMEOUT;
    if let Some(guard)=production_guard.as_mut() {guard.begin_explicit_shutdown();}
    let owner_shutdown=maintenance.stop_and_join(shutdown_deadline.saturating_duration_since(Instant::now()))?;
    if !owner_shutdown.stop_requested||!owner_shutdown.joined||owner_shutdown.owner_close_error.is_some(){return Err("Maintenance did not join/close".into())}
    let drain=database.shutdown_and_drain(shutdown_deadline.saturating_duration_since(Instant::now()));
    let ack_ids:Vec<_>=reports.iter().flat_map(|r|r["acknowledged_ids"].as_array().unwrap().iter().cloned()).collect();
    let errors:usize=reports.iter().map(|r|r["errors"].as_array().unwrap().len()).sum();
    let partial=manual.iter().any(|r|r["phase"]=="during_pin"&&r["receipt"]["busy"]==0&&r["receipt"]["checkpointed_frames"].as_i64().is_some_and(|n|n>=0)&&r["receipt"]["log_frames"].as_i64().unwrap_or(-1)>r["receipt"]["checkpointed_frames"].as_i64().unwrap_or(-1));
    let recovery=manual.iter().any(|r|r["phase"]=="after_pin"&&r["receipt"]["busy"]==0&&r["receipt"]["log_frames"].as_i64().is_some_and(|n|n>=0)&&r["receipt"]["log_frames"]==r["receipt"]["checkpointed_frames"]);
    let final_complete=owner_shutdown.final_checkpoint.as_ref().is_some_and(|r|r.busy==0&&r.log_frames>=0&&r.log_frames==r.checkpointed_frames);
    let source_unchanged=hash(&source)?==source_hash;
    let reuse_proof=database.connection_reuse_proof();
    let summary=json!({"root":root,"source_backup":source,"source_sha256":source_hash,"read_workload":workload,"seconds":seconds,"reader_interval_ms":100,"writer_interval_ms":100,"writer_count":3,"reader_count":3,"pin_seconds":5,"read_job":read_job,"read_meta_key":read_meta,"workers":reports,"acknowledged_ids":ack_ids,"errors":errors,"manual":manual,"observed_partial":partial,"observed_recovery":recovery,"health_observations":observations,"owner_shutdown":owner_shutdown,"drain_error":drain.as_ref().err().map(ToString::to_string),"source_unchanged":source_unchanged,"final_runtime":database.snapshot(),"diagnostic_connection_reuse":reuse_proof});
    fs::write(root.join("production_maintenance_summary.json"),serde_json::to_vec_pretty(&summary)?)?;
    emit(json!({"event":"terminal","summary":root.join("production_maintenance_summary.json"),"errors":errors,"acknowledged":ack_ids.len()}));
    if errors!=0||!partial||!recovery||!final_complete||!source_unchanged||drain.is_err(){return Err("Actual owner proof failed; inspect retained summary".into())}
    Ok(())
}
