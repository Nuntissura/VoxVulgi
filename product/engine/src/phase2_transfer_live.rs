//! Ephemeral original-owned-command transfer observation; never installer success authority.
use crate::{paths::AppPaths, phase2_transfer::{PipRawDecoder, TransferSnapshot}};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest,Sha256};
use std::{cell::RefCell,collections::HashMap,io::Read,path::Path,sync::{Arc,Mutex,OnceLock,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
#[cfg(windows)]
fn boot_tick_ms()->Option<u64> {
    #[link(name="kernel32")] extern "system" {fn GetTickCount64()->u64;}
    Some(unsafe {GetTickCount64()})
}
#[cfg(not(windows))]
fn boot_tick_ms()->Option<u64> {None}
const LIMIT:usize=256;
const FRAME_LIMIT:usize=4096;
thread_local! {static STEP:RefCell<Option<Arc<Step>>> = const {RefCell::new(None)};}
static LIVE:OnceLock<Mutex<HashMap<String,Arc<Step>>>>=OnceLock::new();
fn key(paths:&AppPaths,id:&str)->String {format!("{}:{id}",paths.base_dir.to_string_lossy())}
pub(crate) type HfArchiveSink=Arc<dyn Fn(&HfRawArchive)->std::io::Result<()>+Send+Sync>;
#[derive(Serialize)]
pub(crate) struct HfRawArchive {pub job_id:String,pub attempt_no:u32,pub step_id:String,pub command_id:String,pub child_pid:u32,pub outcome:String,pub stdout:Vec<u8>,pub stderr:Vec<u8>,pub omitted_frames:usize,pub complete:bool}
struct Step {paths:AppPaths,archive:Option<HfArchiveSink>,key:String,job_id:String,attempt_no:u32,step_id:String,active:AtomicBool,command:Mutex<Option<Arc<Command>>>}
pub struct StepScope {current:Option<Arc<Step>>,previous:Option<Arc<Step>>,thread_bound:std::marker::PhantomData<std::rc::Rc<()>>}
pub fn enter_step(paths:&AppPaths,id:&str,attempt:u32,step:&str)->StepScope {
    enter_step_with_archive(paths,id,attempt,step,None)
}
pub(crate) fn enter_step_with_archive(paths:&AppPaths,id:&str,attempt:u32,step:&str,archive:Option<HfArchiveSink>)->StepScope {
    let current=Arc::new(Step {paths:paths.clone(),archive,key:key(paths,id),job_id:id.into(),attempt_no:attempt,step_id:step.into(),active:AtomicBool::new(true),command:Mutex::new(None)});
    let registered=LIVE.get_or_init(||Mutex::new(HashMap::new())).try_lock().ok().is_some_and(|mut map| {
        map.retain(|_,v|v.active.load(Ordering::Acquire));
        if map.len()>=LIMIT || map.contains_key(&current.key) {return false;}
        map.insert(current.key.clone(),current.clone());true
    });
    let current=registered.then_some(current);
    let previous=STEP.with(|slot|slot.replace(current.clone()));
    StepScope {current,previous,thread_bound:std::marker::PhantomData}
}
impl Drop for StepScope {fn drop(&mut self) {
    if let Some(current)=&self.current {current.active.store(false,Ordering::Release);}
    STEP.with(|slot|{slot.replace(self.previous.take());});
}}
fn current_step()->Option<Arc<Step>> {STEP.with(|s|s.borrow().clone()).filter(|s|s.active.load(Ordering::Acquire))}
#[derive(Clone,Debug,Serialize)]
pub struct LiveTransfer {pub source:String,pub job_id:String,pub attempt_no:u32,pub step_id:String,pub command_id:String,pub child_pid:u32,pub measurement_age_ms:Option<u64>,#[serde(flatten)]pub measurement:TransferSnapshot}
struct Decoder {pip:PipRawDecoder,line:Vec<u8>,overflow:bool,sequence:u64,counter_source:&'static str,command_clock:Option<u64>,http_bars:HashMap<u64,(u64,Option<u64>,u64,u64)>,http_prior:Option<(u64,Option<u64>,u64,Option<u64>)>,http:Option<TransferSnapshot>}
impl Default for Decoder {fn default()->Self {Self {pip:PipRawDecoder::default(),line:Vec::new(),overflow:false,sequence:0,counter_source:"original_http_update_payload_sum",command_clock:None,http_bars:HashMap::new(),http_prior:None,http:None}}}
impl Decoder {
    fn gap(&mut self) {self.pip.note_gap();self.http=None;self.http_prior=None;self.http_bars.clear();self.line.clear();self.overflow=false;}
    fn http_frame(&mut self,line:&[u8],nonce:&str,_at:Duration) {
        let prefix=format!("@@VV_TRANSFER {nonce} ");
        if !line.starts_with(prefix.as_bytes()) {return;}
        let frame=serde_json::from_slice::<Value>(&line[prefix.len()..]);
        let valid=|v:&Value|v.as_u64().filter(|n|*n<=9_007_199_254_740_991);
        let Ok(frame)=frame else {self.gap();return;};
        let fields=(valid(&frame["sequence"]),valid(&frame["bar"]),valid(&frame["received_bytes"]),valid(&frame["initial_bytes"]));
        let (Some(seq),Some(bar),Some(received),Some(initial))=fields else {self.gap();return;};
        let (Some(emitted),Some(command_start))=(frame["observed_monotonic_ns"].as_u64(),frame["command_start_monotonic_ns"].as_u64()) else {self.gap();return;};
        if emitted<command_start || self.command_clock.is_some_and(|old|old!=command_start) {self.gap();return;}
        self.command_clock=Some(command_start);
        let total=if frame["total_bytes"].is_null(){None}else{match valid(&frame["total_bytes"]) {Some(0)=>None,Some(n)=>Some(n),None=>{self.gap();return;}}};
        if frame["schema"]!="vv.phase2.http_transfer.v1" || frame["unit"]!="B" || frame["counter_source"]!=self.counter_source || !frame["gap"].is_boolean() || bar==0 || initial>received || total.is_some_and(|n|received>n) || seq<=self.sequence {
            self.gap();return;
        }
        if seq!=self.sequence.saturating_add(1) || frame["gap"].as_bool()!=Some(false) {self.http=None;self.http_prior=None;self.http_bars.clear();}
        self.sequence=seq;
        let event=frame["event"].as_str().unwrap_or("");
        if !matches!(event,"begin"|"update") {self.http=None;self.http_prior=None;self.http_bars.remove(&bar);return;}
        if event=="update" && valid(&frame["delta_bytes"]).is_none() {self.gap();return;}
        if !self.http_bars.contains_key(&bar) && self.http_bars.len()>=128 {self.gap();return;}
        if event=="begin" {self.http_bars.remove(&bar);}
        let (baseline,rate)=match self.http_bars.get(&bar).copied() {
            Some((old,old_total,baseline,old_emitted)) if received>=old && total==old_total && emitted>old_emitted && emitted-old_emitted<=3_000_000_000 => (baseline,((received-old)>0).then(||(received-old) as f64/Duration::from_nanos(emitted-old_emitted).as_secs_f64())),
            _=>(received,None),
        };
        let rate=rate.filter(|n|n.is_finite()&&*n>0.0);
        self.http=Some(TransferSnapshot {generation:bar,received_bytes:received,baseline_bytes:baseline,newly_received_bytes:received-baseline,total_bytes:total,bytes_per_second:rate,remaining_transfer_seconds:total.zip(rate).map(|(n,r)|(n-received) as f64/r)});
        // Shared OS boot clock preserves source age; receipt arrival cannot reset it.
        self.http_prior=Some((received,total,baseline,frame["observed_boot_tick_ms"].as_u64()));
        self.http_bars.insert(bar,(received,total,baseline,emitted));
    }
    fn source_age_ms(&self,now:Option<u64>)->Option<u64> {
        let observed=self.http_prior?.3?;
        now?.checked_sub(observed)
    }
    fn http_snapshot(&self,now:Option<u64>)->Option<TransferSnapshot> {
        let mut value=self.http.clone()?;
        if self.source_age_ms(now).is_none_or(|age|age>3000) {
            value.bytes_per_second=None;value.remaining_transfer_seconds=None;
        }
        Some(value)
    }
    fn feed_http(&mut self,bytes:&[u8],nonce:&str,at:Duration) {
        for &b in bytes {
            if b==b'\n' || b==b'\r' {if !self.overflow {let line=std::mem::take(&mut self.line);self.http_frame(&line,nonce,at);}self.line.clear();self.overflow=false;}
            else if !self.overflow && self.line.len()<FRAME_LIMIT {self.line.push(b);}
            else {if self.line.starts_with(b"@@VV_TRANSFER "){self.gap();}self.line.clear();self.overflow=true;}
        }
    }
}
pub struct Command {step:Arc<Step>,id:String,pid:u32,protocol:String,active:AtomicBool,lost:AtomicBool,start:Instant,decoder:Mutex<Decoder>}
pub struct CommandScope(Option<Arc<Command>>);
impl CommandScope {
    pub fn observer(&self)->Option<Arc<Command>> {self.0.clone()}
    /// Terminal owning-thread publication, separate from optional decoder measurements and pip logs.
    pub(crate) fn archive_hf_output(&self,stdout:&[u8],stderr:&[u8],outcome:&str,pipes_complete:bool) {
        let Some(c)=&self.0 else {return;};
        if c.protocol!="huggingface_http_payload" {return;}
        let Some(sink)=&c.step.archive else {return;};
        let (stdout,stdout_omitted)=raw_hf_frames(stdout,&c.id);
        let (stderr,stderr_omitted)=raw_hf_frames(stderr,&c.id);
        let omitted=stdout_omitted.saturating_add(stderr_omitted);
        let record=HfRawArchive {job_id:c.step.job_id.clone(),attempt_no:c.step.attempt_no,step_id:c.step.step_id.clone(),command_id:c.id.clone(),child_pid:c.pid,outcome:outcome.into(),stdout,stderr,omitted_frames:omitted,complete:pipes_complete&&omitted==0};
        let failure=sink(&record).err();
        if failure.is_some()||!record.complete {
            crate::diagnostics::emit_trace_event(&c.step.paths,"phase2_hf_raw_archive_incomplete","warn",serde_json::json!({"job_id":c.step.job_id,"attempt_no":c.step.attempt_no,"step_id":c.step.step_id,"command_id":c.id,"child_pid":c.pid,"omitted_frames":omitted,"archive_write_failed":failure.is_some(),"archive_error_kind":failure.as_ref().map(|e|format!("{:?}",e.kind())),"pipes_complete":pipes_complete}));
        }
    }
}
/// Preserve source-emitted lines verbatim; inspect structure only to exclude secrets/unbounded data.
/// No decoded counters are serialized as independent raw evidence.
fn raw_hf_frames(bytes:&[u8],nonce:&str)->(Vec<u8>,usize) {
    const ARCHIVE_LIMIT:usize=4*1024*1024;
    let prefix=format!("@@VV_TRANSFER {nonce} ");let mut raw=Vec::new();let mut omitted=0usize;
    for line in bytes.split_inclusive(|b|*b==b'\n') {
        if !line.starts_with(prefix.as_bytes()) {continue;}
        let safe=line.len()<=FRAME_LIMIT&&line.ends_with(b"\n")&&serde_json::from_slice::<Value>(&line[prefix.len()..]).ok().is_some_and(|v| {
            let Some(fields)=v.as_object() else {return false;};
            fields.iter().all(|(key,value)|match key.as_str() {
                "schema"=>value=="vv.phase2.http_transfer.v1",
                "event"=>matches!(value.as_str(),Some("begin"|"update"|"invalid"|"bar_closed")),
                "unit"=>value=="B",
                "counter_source"=>value=="original_http_update_payload_sum",
                "gap"=>value.is_boolean(),
                "sequence"|"bar"|"received_bytes"|"initial_bytes"|"total_bytes"|"delta_bytes"|"observed_monotonic_ns"|"command_start_monotonic_ns"|"observed_boot_tick_ms"=>value.is_null()||value.as_u64().is_some(),
                _=>false,
            })&&fields.get("schema")==Some(&Value::from("vv.phase2.http_transfer.v1"))
        });
        if !safe||raw.len().saturating_add(line.len())>ARCHIVE_LIMIT {omitted=omitted.saturating_add(1);continue;}
        raw.extend_from_slice(line);
    }(raw,omitted)
}
impl Drop for CommandScope {fn drop(&mut self) {if let Some(c)=&self.0 {c.active.store(false,Ordering::Release);}}}
pub fn begin_command(command:&std::process::Command,pid:u32)->CommandScope {
    let env=|name:&str|command.get_envs().find(|(key,_)|*key==name).and_then(|(_,v)|v).and_then(|v|v.to_str()).map(str::to_owned);
    let Some(step)=current_step() else {return CommandScope(None);};
    let (Some(id),Some(protocol))=(env("VOXVULGI_TRANSFER_NONCE"),env("VOXVULGI_TRANSFER_PROTOCOL")) else {return CommandScope(None);};
    if !matches!(protocol.as_str(),"pip_raw"|"huggingface_http_payload") || uuid::Uuid::parse_str(&id).is_err() {return CommandScope(None);}
    let counter_source=if protocol=="pip_raw" {"original_pip_raw_payload_sum"} else {"original_http_update_payload_sum"};
    let c=Arc::new(Command {step:step.clone(),id,pid,protocol,active:AtomicBool::new(true),lost:AtomicBool::new(false),start:Instant::now(),decoder:Mutex::new(Decoder {counter_source,..Decoder::default()})});
    let scope = if let Ok(mut latest)=step.command.try_lock() {*latest=Some(c.clone());CommandScope(Some(c))} else {CommandScope(None)};
    scope
}
impl Command {
    fn feed(&self,bytes:&[u8],stderr:bool) {
        if !self.active.load(Ordering::Acquire)||!self.step.active.load(Ordering::Acquire){return;}
        let Ok(mut d)=self.decoder.try_lock() else {self.lost.store(true,Ordering::Release);return;};
        if self.lost.swap(false,Ordering::AcqRel){d.gap();}
        if stderr {d.feed_http(bytes,&self.id,self.start.elapsed());} // Unclocked raw stdout remains log-only.
    }
}
pub fn capture(mut reader:impl Read,observation:Option<Arc<Command>>,stderr:bool)->Vec<u8> {
    let mut bytes=Vec::new();let mut chunk=[0u8;8192];
    loop {match reader.read(&mut chunk) {Ok(0)=>break,Ok(n)=>{bytes.extend_from_slice(&chunk[..n]);if let Some(c)=&observation {c.feed(&chunk[..n],stderr);}},Err(e) if e.kind()==std::io::ErrorKind::Interrupted=>continue,Err(_)=>break}}
    bytes
}
/// The caller MUST independently match canonical running job/attempt and journal running step.
pub fn snapshot(paths:&AppPaths,id:&str,attempt:u32,step_id:&str)->Option<LiveTransfer> {
    let step=LIVE.get()?.try_lock().ok()?.get(&key(paths,id))?.clone();
    if step.attempt_no!=attempt||step.step_id!=step_id||!step.active.load(Ordering::Acquire){return None;}
    let c=step.command.try_lock().ok()?.clone()?;
    if !c.active.load(Ordering::Acquire)||c.lost.load(Ordering::Acquire){return None;}
    let d=c.decoder.try_lock().ok()?;
    let now=boot_tick_ms();
    let mut measurement=d.http_snapshot(now)?;
    if !step.active.load(Ordering::Acquire)||!c.active.load(Ordering::Acquire)||c.lost.load(Ordering::Acquire){return None;}
    if measurement.total_bytes.is_none(){measurement.remaining_transfer_seconds=None;}
    Some(LiveTransfer {source:c.protocol.clone(),job_id:step.job_id.clone(),attempt_no:attempt,step_id:step_id.into(),command_id:c.id.clone(),child_pid:c.pid,measurement_age_ms:d.source_age_ms(now),measurement})
}
fn pinned_site(python:&Path,entries:&Value)->Option<(std::path::PathBuf,Value)> {
    let scripts=python.parent()?;
    if !scripts.file_name()?.to_str()?.eq_ignore_ascii_case("Scripts"){return None;}
    let root=scripts.parent()?.join("Lib").join("site-packages");
    for entry in entries.as_array()? {
        let Some(files)=entry["files"].as_object() else {continue;};
        if files.is_empty(){continue;}
        let valid=files.iter().all(|(relative,expected)| {
            let p=std::path::Path::new(relative);
            if p.is_absolute()||p.components().any(|v|!matches!(v,std::path::Component::Normal(_))){return false;}
            let Some(expected)=expected.as_str() else {return false;};
            let path=root.join(p);
            if !std::fs::metadata(&path).ok().is_some_and(|m|m.is_file()&&m.len()<=2_097_152){return false;}
            std::fs::read(path).ok().is_some_and(|bytes|hex::encode(Sha256::digest(bytes))==expected)
        });
        if valid {return Some((root,entry.clone()));}
    }None
}
/// Expected hashes come ONLY from reviewed bundled provenance. Empty/unknown tables preserve args.
pub fn instrument_python(command:&mut std::process::Command,python:&Path,args:&[&str])->Option<String> {
    current_step()?;
    let table:Value=serde_json::from_str(include_str!("../resources/tooling/phase2_transfer_source_pins.json").trim_start_matches('\u{feff}')).ok()?;
    let protocol;
    let selected;
    if args.get(0)==Some(&"-m") && args.get(1)==Some(&"pip") && args.get(2)==Some(&"install") {
        let (site,entry)=pinned_site(python,&table["pip"])?;
        if entry["raw_progress_supported"]!=true{return None;}
        for required in ["pip/__init__.py","pip/_internal/cli/progress_bars.py","pip/_internal/cli/cmdoptions.py","pip/_internal/cli/spinners.py"] {
            if !entry["files"][required].as_str().is_some_and(|hash|hash.len()==64&&hash.bytes().all(|b|b.is_ascii_hexdigit())) {return None;}
        }
        let wrapper=format!("{}\n{}\n{}",include_str!("../resources/tooling/hf_owned_progress.py"),include_str!("../resources/tooling/hf_transfer_wrapper.py"),include_str!("../resources/tooling/pip_transfer_wrapper.py"));
        let code=format!("{}\n_vv_run_original_pip({}, {}, {})",wrapper,serde_json::to_string(&args[2..]).ok()?,serde_json::to_string(&entry["files"]).ok()?,serde_json::to_string(&site.to_string_lossy()).ok()?);
        replace_python_code(command,&code,false);
        command.env("VOXVULGI_ORIGINAL_SCOPED_PIP","1");protocol="pip_raw";selected=entry;
    } else if let Some((isolated,original_code))=hf_python_code(args) {
        let (site,entry)=pinned_site(python,&table["huggingface"])?;
        if !((entry["huggingface_hub"]=="0.34.4"&&entry["tqdm"]=="4.68.3")||(entry["huggingface_hub"]=="1.33.0"&&entry["tqdm"]=="4.70.1")){return None;}
        let required=["huggingface_hub.utils.tqdm","huggingface_hub.file_download","huggingface_hub.constants","tqdm.std","tqdm.utils","tqdm.auto"];
        let modules=entry["modules"].as_object()?;
        if modules.len()!=required.len() || required.iter().any(|name|!modules.get(*name).and_then(Value::as_str).is_some_and(|hash|hash.len()==64&&hash.bytes().all(|b|b.is_ascii_hexdigit()))) {return None;}
        let code=include_str!("../resources/tooling/hf_owned_progress.py");
        let wrapper=include_str!("../resources/tooling/hf_transfer_wrapper.py");
        let prelude=format!("{}\n{}\n_vv_run_original({}, {}, {})",code,wrapper,serde_json::to_string(original_code).ok()?,serde_json::to_string(&site.to_string_lossy()).ok()?,serde_json::to_string(&entry["modules"]).ok()?);
        // Replace only this command's original -c code; original acquisition code is executed unchanged.
        replace_python_code(command,&prelude,isolated);
        protocol="huggingface_http_payload";selected=entry;
    } else {return None;}
    let _=selected;
    let nonce=uuid::Uuid::new_v4().to_string();command.env("VOXVULGI_TRANSFER_NONCE",&nonce).env("VOXVULGI_TRANSFER_PROTOCOL",protocol);Some(nonce)
}
/// Only the two original acquisition forms are eligible; no extra flags or argv tail.
fn hf_python_code<'a>(args:&[&'a str])->Option<(bool,&'a str)> {
    let (isolated,code)=match args {
        ["-c",code]=>(false,*code),
        ["-I","-c",code]=>(true,*code),
        _=>return None,
    };
    (code.contains("hf_hub_download(")||code.contains("snapshot_download(")).then_some((isolated,code))
}
fn replace_python_code(command:&mut std::process::Command,code:&str,isolated:bool) {
    let program=command.get_program().to_owned();let envs:Vec<_>=command.get_envs().map(|(k,v)|(k.to_owned(),v.map(std::ffi::OsStr::to_owned))).collect();let cwd=command.get_current_dir().map(Path::to_path_buf);
    let mut replacement=crate::cmd::command(program);if isolated {replacement.arg("-I");}replacement.args(["-c",code]);
    for (k,v) in envs {if let Some(v)=v{replacement.env(k,v);}else{replacement.env_remove(k);}}if let Some(cwd)=cwd{replacement.current_dir(cwd);}*command=replacement;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test] fn hf_raw_archive_actual_owned_success_error_and_cancel_retain_original_results() {
        for mode in ["success","nonzero_exit","canceled"] {
            let temp=tempfile::tempdir().unwrap();let paths=AppPaths::new(temp.path().to_path_buf());
            let records=Arc::new(Mutex::new(Vec::new()));let received=records.clone();
            let _step=enter_step_with_archive(&paths,"real-original",1,"model",Some(Arc::new(move|r| {received.lock().unwrap().push((r.outcome.clone(),r.stderr.clone()));Ok(())})));
            let nonce=uuid::Uuid::new_v4().to_string();let raw=String::from_utf8(frame(1,1,100,100)).unwrap().replace("nonce",&nonce);
            let tail=match mode {"success"=>"exit 0","nonzero_exit"=>"exit 7",_=>"Start-Sleep -Seconds 5"};
            let script=format!("[Console]::Error.WriteLine('{}'); Write-Output 'original stdout'; {tail}",raw.trim());
            let mut command=crate::cmd::command("powershell.exe");command.args(["-NoProfile","-NonInteractive","-Command",&script]);command.env("VOXVULGI_TRANSFER_NONCE",nonce).env("VOXVULGI_TRANSFER_PROTOCOL","huggingface_http_payload");
            let started=Instant::now();let result=crate::cmd::run_owned_output(&mut command,Duration::from_secs(10),||mode=="canceled"&&started.elapsed()>Duration::from_secs(2));
            if mode=="canceled" {assert_eq!(result.unwrap_err().kind(),std::io::ErrorKind::Interrupted);} else {let output=result.unwrap();assert_eq!(output.status.success(),mode=="success");assert!(String::from_utf8_lossy(&output.stdout).contains("original stdout"));}
            let actual=records.lock().unwrap();assert_eq!(actual.len(),1);assert_eq!(actual[0].0,mode);assert!(String::from_utf8_lossy(&actual[0].1).contains(raw.trim()));
        }
    }
    #[test] fn hf_raw_archive_preserves_source_bytes_and_refuses_private_or_unbounded_frames() {
        let raw=frame(1,1,100,100);let mut input=b"private https://example.invalid?token=secret\n".to_vec();input.extend_from_slice(&raw);
        let (archived,omitted)=raw_hf_frames(&input,"nonce");assert_eq!(archived,raw);assert_eq!(omitted,0);
        let forbidden=b"@@VV_TRANSFER nonce {\"schema\":\"vv.phase2.http_transfer.v1\",\"url\":\"secret\"}\n";
        let (archived,omitted)=raw_hf_frames(forbidden,"nonce");assert!(archived.is_empty());assert_eq!(omitted,1);
        let mut many=Vec::new();while many.len()<=4*1024*1024 {many.extend_from_slice(&raw);}
        let (archived,omitted)=raw_hf_frames(&many,"nonce");assert!(archived.len()<=4*1024*1024);assert!(omitted>0);
        assert!(archived.chunks(raw.len()).all(|v|v==raw));
    }
    #[test] fn hf_raw_archive_owns_exact_command_channels_and_optional_failure_does_not_escape() {
        let temp=tempfile::tempdir().unwrap();let paths=AppPaths::new(temp.path().to_path_buf());
        let records=Arc::new(Mutex::new(Vec::new()));let captured=records.clone();
        let sink:HfArchiveSink=Arc::new(move|r| {captured.lock().unwrap().push((r.job_id.clone(),r.attempt_no,r.step_id.clone(),r.command_id.clone(),r.outcome.clone(),r.stdout.clone(),r.stderr.clone(),r.complete));Err(std::io::Error::other("owned archive refusal"))});
        let _step=enter_step_with_archive(&paths,"original",3,"cosy",Some(sink));
        let mut command=crate::cmd::command("unused");let nonce=uuid::Uuid::new_v4().to_string();command.env("VOXVULGI_TRANSFER_NONCE",&nonce).env("VOXVULGI_TRANSFER_PROTOCOL","huggingface_http_payload");
        let scope=begin_command(&command,123);let raw=String::from_utf8(frame(1,1,100,100)).unwrap().replace("nonce",&nonce).into_bytes();
        for outcome in ["success","nonzero_exit","canceled","timeout"] {scope.archive_hf_output(&[],&raw,outcome,true);}
        let actual=records.lock().unwrap();assert_eq!(actual.len(),4);
        for r in actual.iter() {assert_eq!((&r.0,r.1,&r.2,&r.3),( &"original".to_string(),3,&"cosy".to_string(),&nonce));assert!(r.5.is_empty());assert_eq!(r.6,raw);assert!(r.7);}
        drop(actual);
        command.env("VOXVULGI_TRANSFER_PROTOCOL","pip_raw");begin_command(&command,124).archive_hf_output(&raw,&raw,"success",true);
        assert_eq!(records.lock().unwrap().len(),4,"Pip remains solely under its mandatory original sink");
    }
    #[test] fn capture_preserves_partial_bytes_and_interrupted_read() {
        struct Reader(u8);impl Read for Reader {fn read(&mut self,b:&mut[u8])->std::io::Result<usize>{self.0+=1;match self.0{1=>Err(std::io::ErrorKind::Interrupted.into()),2=>{b[..3].copy_from_slice(b"abc");Ok(3)},_=>Err(std::io::ErrorKind::Other.into())}}}
        assert_eq!(capture(Reader(0),None,false),b"abc");
    }
    #[test] fn pip_unclocked_raw_output_cannot_claim_live_transfer() {
        let temp=tempfile::tempdir().unwrap();let paths=AppPaths::new(temp.path().to_path_buf());
        let _step=enter_step(&paths,"pip-pin-test",1,"model");let mut command=crate::cmd::command("unused");
        command.env("VOXVULGI_TRANSFER_NONCE",uuid::Uuid::new_v4().to_string()).env("VOXVULGI_TRANSFER_PROTOCOL","pip_raw");
        let scope=begin_command(&command,1);let observer=scope.observer().unwrap();
        observer.feed(b"Progress 1 of 2\n",false);assert!(snapshot(&paths,"pip-pin-test",1,"model").is_none());
        observer.feed(&frame(1,1,100,100),true);assert!(snapshot(&paths,"pip-pin-test",1,"model").is_none());
        let actual=String::from_utf8(frame(1,1,100,100)).unwrap().replace("nonce",&observer.id).replace("original_http_update_payload_sum","original_pip_raw_payload_sum");
        observer.feed(actual.as_bytes(),true);assert!(snapshot(&paths,"pip-pin-test",1,"model").is_some());
    }
    #[test] fn wrong_nonce_and_file_count_cannot_create_http_measurements() {
        let mut d=Decoder::default();d.feed_http(b"@@VV_TRANSFER other {}\n","mine",Duration::ZERO);assert!(d.http.is_none());
        d.feed_http(b"@@VV_TRANSFER mine {\"schema\":\"vv.phase2.http_transfer.v1\",\"unit\":\"files\"}\n","mine",Duration::ZERO);assert!(d.http.is_none());
    }
    #[test] fn unknown_sources_do_not_change_original_python_arguments() {
        let mut c=crate::cmd::command("python.exe");c.args(["-m","pip","install","example"]);
        assert!(instrument_python(&mut c,Path::new("Scripts/python.exe"),&["-m","pip","install","example"]).is_none());
        assert_eq!(c.get_args().collect::<Vec<_>>(),["-m","pip","install","example"].map(std::ffi::OsStr::new));
    }
    #[test] fn isolated_hf_code_preserves_isolation_original_code_environment_and_cwd() {
        let original="from huggingface_hub import snapshot_download\nsnapshot_download('original-repo', revision='original-revision')";
        let args=["-I","-c",original];
        let (isolated,code)=hf_python_code(&args).unwrap();
        assert!(isolated);assert_eq!(code,original);
        assert_eq!(hf_python_code(&["-c",original]),Some((false,original)));
        let temp=tempfile::tempdir().unwrap();
        let mut command=crate::cmd::command("original-python.exe");command.args(args);
        command.env("HF_HOME","original-cache").env("PIP_NO_INPUT","1").env_remove("PYTHONPATH").current_dir(temp.path());
        let before_env:Vec<_>=command.get_envs().map(|(k,v)|(k.to_owned(),v.map(std::ffi::OsStr::to_owned))).collect();
        let wrapped=format!("_vv_run_original({}, 'site', {{}})",serde_json::to_string(code).unwrap());
        replace_python_code(&mut command,&wrapped,isolated);
        assert_eq!(command.get_program(),std::ffi::OsStr::new("original-python.exe"));
        assert_eq!(command.get_args().collect::<Vec<_>>(),["-I","-c",wrapped.as_str()].map(std::ffi::OsStr::new));
        assert_eq!(command.get_current_dir(),Some(temp.path()));
        assert_eq!(command.get_envs().map(|(k,v)|(k.to_owned(),v.map(std::ffi::OsStr::to_owned))).collect::<Vec<_>>(),before_env);
        assert!(wrapped.contains(&serde_json::to_string(original).unwrap()));
    }
    #[test] fn unsupported_hf_flags_or_trailing_arguments_leave_command_unchanged() {
        let temp=tempfile::tempdir().unwrap();let paths=AppPaths::new(temp.path().to_path_buf());
        let _step=enter_step(&paths,"unsupported-hf-flags",1,"model");
        let code="hf_hub_download('original-repo', 'original-file')";
        for args in [vec!["-u","-c",code],vec!["-E","-c",code],vec!["-B","-c",code],vec!["-I","-B","-c",code],vec!["-I","-c",code,"tail"],vec!["-c",code,"tail"],vec!["-I","-c","print('not acquisition')"]] {
            assert!(hf_python_code(&args).is_none());
            let mut command=crate::cmd::command("original-python.exe");command.args(&args).env("HF_HOME","original-cache").env_remove("PYTHONPATH").current_dir(temp.path());
            let before_env:Vec<_>=command.get_envs().map(|(k,v)|(k.to_owned(),v.map(std::ffi::OsStr::to_owned))).collect();
            assert!(instrument_python(&mut command,Path::new("Scripts/python.exe"),&args).is_none());
            assert_eq!(command.get_args().collect::<Vec<_>>(),args.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>());
            assert_eq!(command.get_current_dir(),Some(temp.path()));
            assert_eq!(command.get_envs().map(|(k,v)|(k.to_owned(),v.map(std::ffi::OsStr::to_owned))).collect::<Vec<_>>(),before_env);
        }
    }
    fn frame(sequence:u64,bar:u64,bytes:u64,emitted:u64)->Vec<u8> {
        format!("@@VV_TRANSFER nonce {}\n",serde_json::json!({"schema":"vv.phase2.http_transfer.v1","unit":"B","counter_source":"original_http_update_payload_sum","sequence":sequence,"bar":bar,"received_bytes":bytes,"initial_bytes":0,"total_bytes":1000,"gap":false,"event":"update","delta_bytes":100,"observed_monotonic_ns":emitted,"command_start_monotonic_ns":100,"observed_boot_tick_ms":1000})).into_bytes()
    }
    #[test] fn emitted_child_time_not_parent_queue_delay_controls_rate() {
        let mut d=Decoder::default();
        d.feed_http(&frame(1,1,100,100),"nonce",Duration::from_secs(5));
        d.feed_http(&frame(2,1,200,1_000_000_100),"nonce",Duration::from_secs(50));
        let delayed=d.http_snapshot(Some(50000)).unwrap();
        assert_eq!(delayed.received_bytes,200);assert_eq!(delayed.bytes_per_second,None);
        assert_eq!(delayed.remaining_transfer_seconds,None);
        let aligned=d.http_snapshot(Some(2000)).unwrap();
        assert_eq!(aligned.bytes_per_second,Some(100.0));assert_eq!(aligned.remaining_transfer_seconds,Some(8.0));
        let uncertain=d.http_snapshot(None).unwrap();
        assert_eq!(uncertain.bytes_per_second,None);assert_eq!(uncertain.remaining_transfer_seconds,None);
        let future=d.http_snapshot(Some(999)).unwrap();assert_eq!(future.bytes_per_second,None);assert_eq!(future.remaining_transfer_seconds,None);
    }
    #[test] fn concurrent_bars_do_not_sum_totals_and_eligible_malformed_clears_baselines() {
        let mut d=Decoder::default();d.feed_http(&frame(1,1,100,100),"nonce",Duration::from_secs(1));
        d.feed_http(&frame(2,2,800,100),"nonce",Duration::from_secs(1));
        d.feed_http(&frame(3,1,200,1_000_000_100),"nonce",Duration::from_secs(2));
        assert_eq!(d.http.as_ref().unwrap().received_bytes,200);assert_eq!(d.http.as_ref().unwrap().total_bytes,Some(1000));
        d.feed_http(b"@@VV_TRANSFER nonce \xff\n","nonce",Duration::from_secs(2));assert!(d.http.is_none());
        d.feed_http(&frame(4,1,300,2_000_000_100),"nonce",Duration::from_secs(3));assert_eq!(d.http.as_ref().unwrap().bytes_per_second,None);
    }
    #[test] fn command_scope_and_step_end_refuse_late_reader_or_wrong_attempt() {
        let temp=tempfile::tempdir().unwrap();let paths=AppPaths::new(temp.path().to_path_buf());
        let scope=enter_step(&paths,"scope-test",1,"model");let mut command=crate::cmd::command("unused");
        command.env("VOXVULGI_TRANSFER_NONCE",uuid::Uuid::new_v4().to_string()).env("VOXVULGI_TRANSFER_PROTOCOL","pip_raw");
        let command_scope=begin_command(&command,1);let observer=command_scope.observer().unwrap();
        let actual=String::from_utf8(frame(1,1,100,100)).unwrap().replace("nonce",&observer.id).replace("original_http_update_payload_sum","original_pip_raw_payload_sum");observer.feed(actual.as_bytes(),true);assert!(snapshot(&paths,"scope-test",1,"model").is_some());
        assert!(snapshot(&paths,"scope-test",2,"model").is_none());
        {let _held=observer.decoder.lock().unwrap();observer.feed(b"Progress 2 of 2\n",false);assert!(snapshot(&paths,"scope-test",1,"model").is_none());}
        drop(command_scope);observer.feed(b"Progress 2 of 2\n",false);assert!(snapshot(&paths,"scope-test",1,"model").is_none());
        drop(scope);assert!(snapshot(&paths,"scope-test",1,"model").is_none());
    }
    #[cfg(windows)]
    #[test] fn original_owned_child_publishes_before_exit_and_retains_full_stdout() {
        let temp=tempfile::tempdir().unwrap();let paths=AppPaths::new(temp.path().to_path_buf());let worker_paths=paths.clone();
        let worker=std::thread::spawn(move|| {
            let _step=enter_step(&worker_paths,"owned-child-test",1,"model");
            let mut command=crate::cmd::command("powershell.exe");
            let nonce=uuid::Uuid::new_v4().to_string();
            let actual=String::from_utf8(frame(1,1,100,100)).unwrap().replace("nonce",&nonce).replace("original_http_update_payload_sum","original_pip_raw_payload_sum");
            let script=format!("[Console]::Error.WriteLine('{}'); Write-Output 'Progress 10 of 100'; Start-Sleep -Seconds 2; Write-Output 'Progress 100 of 100'",actual.trim());
            command.args(["-NoProfile","-NonInteractive","-Command",&script]);
            command.env("VOXVULGI_TRANSFER_NONCE",nonce).env("VOXVULGI_TRANSFER_PROTOCOL","pip_raw");
            crate::cmd::run_owned_output(&mut command,Duration::from_secs(10),||false).unwrap()
        });
        let deadline=Instant::now()+Duration::from_secs(8);let mut observed=false;
        while Instant::now()<deadline && !worker.is_finished() {
            if snapshot(&paths,"owned-child-test",1,"model").is_some_and(|v|v.measurement.received_bytes==100) {observed=true;break;}
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(observed,"Actual original owned pipe must publish while child remains live");
        let output=worker.join().unwrap();assert!(output.status.success());
        let stdout=String::from_utf8(output.stdout).unwrap();assert!(stdout.contains("Progress 10 of 100"));assert!(stdout.contains("Progress 100 of 100"));
        assert!(snapshot(&paths,"owned-child-test",1,"model").is_none());
    }
}
