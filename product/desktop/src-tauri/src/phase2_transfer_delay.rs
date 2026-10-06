//! One-shot diagnostic delay of an authentic IPC response in an owned installer proof only.
use super::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
struct Identity { root: PathBuf, job: String, attempt: u32, step: String, command: String }
struct Arm { identity: Identity, generation: String, actor: String, operation: String, until: Instant, hold: Duration, state: &'static str, claimed: u32, released: u32, claimed_at_ms: Option<i64>, finished_at_ms: Option<i64> }
static ARM: Mutex<Option<Arm>> = Mutex::new(None);

fn guard(state: &AppState, nonce: &str) -> Result<(), String> {
    if !agent_bridge_state().lock().map_err(|_| "bridge state unavailable")?.agent_headless
        || state.safe_mode_enabled.load(Ordering::SeqCst) { return Err("Delay requires headless installer proof outside Safe Mode".into()); }
    let proof=state.install_proof.as_ref().ok_or("Exclusive installer proof owner required")?;
    if proof.provenance()["workflow"] == true { return Err("Workflow mode refuses installer delay".into()); }
    proof.revalidate_fast(&state.paths,nonce)
}
fn identity(paths: &AppPaths, value: &Value) -> Option<Identity> {
    let job=&value["canonical_install"]; let transfer=&value["transfer"];
    let id=job["id"].as_str()?; let attempt=u32::try_from(job["attempt_no"].as_u64()?).ok()?;
    let command=transfer["command_id"].as_str()?;
    if uuid::Uuid::parse_str(id).is_err() || uuid::Uuid::parse_str(command).is_err()
        || job["job_type"]!="install_phase2_packs_v1" || job["status"]!="running"
        || transfer["job_id"]!=id || transfer["attempt_no"]!=attempt
        || value["owner"]["job_id"]!=id || value["owner"]["attempt_no"]!=attempt
        || value["owner"]["job_type"]!="install_phase2_packs_v1" || value["owner"]["job_status"]!="running"
        || value["owner"]["step_id"]!=transfer["step_id"] || value["owner"]["step_status"]!="running" { return None; }
    Some(Identity {root:paths.base_dir.clone(),job:id.into(),attempt,step:transfer["step_id"].as_str()?.into(),command:command.into()})
}
fn expire(arm: &mut Arm, now: Instant) {
    if arm.state=="armed" && now>=arm.until {arm.state="expired";}
}
fn claim_once(arm: &mut Arm, id: &Identity, now: Instant) -> Option<(Duration,String)> {
    expire(arm,now);
    if arm.state!="armed" || arm.identity!=*id {return None;}
    arm.state="claimed";arm.claimed+=1;arm.claimed_at_ms=Some(now_epoch_ms_i64());
    Some((arm.hold,arm.generation.clone()))
}
fn view(arm: &Arm) -> Value {
    json!({"job_id":arm.identity.job,"attempt_no":arm.identity.attempt,"step_id":arm.identity.step,
        "command_id":arm.identity.command,"actor_id":arm.actor,"operation_id":arm.operation,
        "generation":arm.generation,"state":arm.state,"claimed_count":arm.claimed,"released_count":arm.released,"claimed_at_ms":arm.claimed_at_ms,"finished_at_ms":arm.finished_at_ms,
        "hold_ms":arm.hold.as_millis(),"remaining_arm_ms":arm.until.saturating_duration_since(Instant::now()).as_millis(),
        "one_shot":true,"original_response_unchanged":true})
}
pub(super) fn arm(state: &AppState, request: &Value) -> Result<Value,String> {
    let nonce=request["proof_nonce"].as_str().ok_or("proof_nonce required")?;guard(state,nonce)?;
    let hold=request["hold_ms"].as_u64().filter(|v| (1..=8000).contains(v)).ok_or("hold_ms must be1..8000")?;
    let job=request["job_id"].as_str().ok_or("job_id required")?;
    let authentic=phase2_transfer_status_readback(&state.paths,job)?;
    let id=identity(&state.paths,&authentic).ok_or("Exact original running transfer required")?;
    if request["attempt_no"]!=id.attempt || request["command_id"]!=id.command {return Err("Current attempt/command mismatch".into());}
    let mut slot=ARM.lock().map_err(|_| "delay state unavailable")?;
    if let Some(old)=slot.as_mut() {expire(old,Instant::now());if matches!(old.state,"armed"|"claimed") {return Err("One delay already pending".into());}}
    let actor=request["actor_id"].as_str().ok_or("actor_id required")?.to_owned();
    let operation=request["operation_id"].as_str().ok_or("operation_id required")?.to_owned();
    let arm=Arm {identity:id,generation:uuid::Uuid::new_v4().to_string(),actor,operation,until:Instant::now()+Duration::from_secs(20),hold:Duration::from_millis(hold),state:"armed",claimed:0,released:0,claimed_at_ms:None,finished_at_ms:None};
    let result=view(&arm);*slot=Some(arm);Ok(result)
}
pub(super) fn status(state: &AppState, request: &Value) -> Result<Value,String> {
    let nonce=request["proof_nonce"].as_str().ok_or("proof_nonce required")?;guard(state,nonce)?;
    let expected={let slot=ARM.lock().map_err(|_| "delay state unavailable")?;slot.as_ref().map(|a|a.identity.clone())};
    let authentic=expected.as_ref().map(|id|phase2_transfer_status_readback(&state.paths,&id.job)).transpose()?;
    let mut slot=ARM.lock().map_err(|_| "delay state unavailable")?;
    if let Some(arm)=slot.as_mut() {
        expire(arm,Instant::now());
        if matches!(arm.state,"armed"|"claimed") && authentic.as_ref().and_then(|v|identity(&state.paths,v)).as_ref()!=Some(&arm.identity) {arm.state="invalidated";arm.finished_at_ms=Some(now_epoch_ms_i64());}
        Ok(view(arm))
    } else {Ok(json!({"state":"idle","claimed_count":0,"released_count":0}))}
}
struct Claim(Identity,String);
impl Drop for Claim {
    fn drop(&mut self) {if let Ok(mut slot)=ARM.lock() {if let Some(arm)=slot.as_mut() {if arm.identity==self.0 && arm.generation==self.1 && arm.state=="claimed" {arm.state="canceled";arm.finished_at_ms=Some(now_epoch_ms_i64());}}}}
}
pub(super) async fn delay(state: &AppState, authentic: &Value) {
    let Some(id)=identity(&state.paths,authentic) else {return;};
    // Ordinary reads incur no owner/filesystem revalidation without an eligible arm.
    {let Ok(mut slot)=ARM.lock() else {return;};let Some(arm)=slot.as_mut() else {return;};expire(arm,Instant::now());if arm.state!="armed" || arm.identity!=id {return;}}
    let Some(proof)=state.install_proof.as_ref() else {return;};
    let nonce=proof.nonce_for_internal_admission();
    if guard(state,&nonce).is_err() {return;}
    let (hold,generation)={
        let Ok(mut slot)=ARM.lock() else {return;}; let Some(arm)=slot.as_mut() else {return;};
        let Some(claimed)=claim_once(arm,&id,Instant::now()) else {return;};claimed
    };
    let claim=Claim(id.clone(),generation.clone()); let deadline=Instant::now()+hold;
    loop {
        let remaining=deadline.saturating_duration_since(Instant::now());if remaining.is_zero() {break;}
        tokio::time::sleep(remaining.min(Duration::from_millis(250))).await;
        let paths=state.paths.clone();let job=id.job.clone();let monitor_proof=proof.clone();let monitor_nonce=nonce.clone();
        let remaining=deadline.saturating_duration_since(Instant::now());if remaining.is_zero() {break;}
        let fresh=tokio::time::timeout(remaining,tauri::async_runtime::spawn_blocking(move || {
            monitor_proof.revalidate_fast(&paths,&monitor_nonce)?;
            phase2_transfer_status_readback(&paths,&job)
        })).await;
        let still_current=!state.safe_mode_enabled.load(Ordering::SeqCst) && fresh.ok().and_then(Result::ok).and_then(Result::ok).as_ref().and_then(|v|identity(&state.paths,v)).as_ref()==Some(&id);
        {let Ok(mut slot)=ARM.lock() else {return;};let Some(arm)=slot.as_mut() else {return;};
        if arm.identity!=id || arm.generation!=generation || arm.state!="claimed" {return;}
        if !still_current {arm.state="invalidated";arm.finished_at_ms=Some(now_epoch_ms_i64());return;}}
    }
    if let Ok(mut slot)=ARM.lock() {if let Some(arm)=slot.as_mut() {if arm.identity==id && arm.generation==generation && arm.state=="claimed" {arm.state="released";arm.released+=1;arm.finished_at_ms=Some(now_epoch_ms_i64());}}}
    drop(claim);
}
pub(super) fn close(paths: &AppPaths) {if let Ok(mut slot)=ARM.lock() {if let Some(arm)=slot.as_mut() {if arm.identity.root==paths.base_dir && matches!(arm.state,"armed"|"claimed") {arm.state="invalidated";arm.finished_at_ms=Some(now_epoch_ms_i64());}}}}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn one_shot_claim_drop_cancels_only_matching_identity() {
        let id=Identity{root:PathBuf::from("isolated"),job:"job".into(),attempt:1,step:"pack".into(),command:"command".into()};
        *ARM.lock().unwrap()=Some(Arm{identity:id.clone(),generation:"generation".into(),actor:"actor".into(),operation:"operation".into(),until:Instant::now()+Duration::from_secs(20),hold:Duration::from_millis(1),state:"armed",claimed:0,released:0,claimed_at_ms:None,finished_at_ms:None});
        {let mut slot=ARM.lock().unwrap();let arm=slot.as_mut().unwrap();
        assert!(claim_once(arm,&Identity{attempt:2,..id.clone()},Instant::now()).is_none());assert_eq!(arm.claimed,0);
        assert!(claim_once(arm,&id,Instant::now()).is_some());assert!(claim_once(arm,&id,Instant::now()).is_none());assert_eq!(arm.claimed,1);}
        drop(Claim(id.clone(),"old-generation".into()));assert_eq!(ARM.lock().unwrap().as_ref().unwrap().state,"claimed");
        drop(Claim(Identity{attempt:2,..id.clone()},"generation".into()));assert_eq!(ARM.lock().unwrap().as_ref().unwrap().state,"claimed");
        drop(Claim(id,"generation".into()));assert_eq!(ARM.lock().unwrap().as_ref().unwrap().state,"canceled");*ARM.lock().unwrap()=None;
    }
    #[test] fn expired_arm_cannot_remain_claimable() {
        let mut arm=Arm{identity:Identity{root:PathBuf::new(),job:"job".into(),attempt:1,step:"pack".into(),command:"command".into()},generation:"generation".into(),actor:"actor".into(),operation:"operation".into(),until:Instant::now(),hold:Duration::from_millis(8000),state:"armed",claimed:0,released:0,claimed_at_ms:None,finished_at_ms:None};
        let id=arm.identity.clone();assert!(claim_once(&mut arm,&id,Instant::now()).is_none());assert_eq!(arm.state,"expired");assert_eq!(arm.claimed,0);
    }
    #[test] fn terminal_or_changed_attempt_has_no_running_identity() {
        let paths=AppPaths::new(PathBuf::from("isolated"));
        let mut value=json!({"canonical_install":{"id":"57ac04a0-3857-4b34-9655-08944448de67","attempt_no":1,"job_type":"install_phase2_packs_v1","status":"running"},"owner":{"job_id":"57ac04a0-3857-4b34-9655-08944448de67","attempt_no":1,"job_type":"install_phase2_packs_v1","job_status":"running","step_id":"pack","step_status":"running"},"transfer":{"job_id":"57ac04a0-3857-4b34-9655-08944448de67","attempt_no":1,"step_id":"pack","command_id":"363d9516-d2de-4008-9a1a-57d794a68d5d"}});
        assert!(identity(&paths,&value).is_some());value["transfer"]["attempt_no"]=json!(2);assert!(identity(&paths,&value).is_none());value["transfer"]["attempt_no"]=json!(1);value["canonical_install"]["status"]=json!("succeeded");assert!(identity(&paths,&value).is_none());
    }
}
