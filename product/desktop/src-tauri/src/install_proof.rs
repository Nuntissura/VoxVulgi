//! Explicit disposable install proof; never selected implicitly by runtime mode.
use super::*;
use std::path::{Path, PathBuf};

pub(super) const FLAG: &str = "--agent-install-proof";
pub(super) const WORKFLOW_FLAG: &str = "--agent-install-proof-workflow";
pub(super) const REATTACH_SHA_FLAG: &str = "--agent-install-proof-owner-sha256";

#[cfg(windows)]
pub(super) struct InstallProof {
    root: PathBuf,
    nonce: String,
    directory: std::fs::File,
    identity: OfflineProofDirectoryIdentity,
    _exclusive_owner: std::fs::File,
    active_install: AtomicBool,
    original_owner_sha256: String,
    original_owner_pid: u32,
    reattached: bool,
    workflow: bool,
}
#[cfg(not(windows))]
#[derive(Debug)]
pub(super) struct InstallProof;

#[cfg(windows)]
impl std::fmt::Debug for InstallProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallProof").field("physical_identity", &self.identity)
            .field("nonce", &"<redacted>").finish_non_exhaustive()
    }
}

fn overlap(left: &Path, right: &Path) -> bool {
    let a = left.to_string_lossy().to_ascii_lowercase();
    let b = right.to_string_lossy().to_ascii_lowercase();
    let a = Path::new(&a); let b = Path::new(&b);
    a.starts_with(b) || b.starts_with(a)
}

#[cfg(windows)]
impl InstallProof {
    pub(super) fn provenance(&self) -> serde_json::Value {
        serde_json::json!({"reattached":self.reattached,"workflow":self.workflow,
            "purpose":if self.workflow {"private original product workflows; installer bridge commands refused"} else {"original guarded installer proof"},"original_owner_pid":self.original_owner_pid,
            "current_owner_pid":std::process::id(),"original_owner_sha256":self.original_owner_sha256,
            "physical_root":self.root,"nonce":self.nonce,"volume_serial":self.identity.volume_serial,"file_id":self.identity.file_id,
            "configuration_only":true,
            "original_source_and_executable":"not recorded in owner marker; reconcile original launch and Update receipts independently"})
    }
    pub(super) fn explicit_runner_policy(&self, paths: &AppPaths, safe_mode: &AtomicBool) -> Result<bool,String> {
        if !self.workflow {return Ok(false);}
        if safe_mode.load(Ordering::SeqCst) {return Err("Safe Mode blocks private workflow runner".into());}
        self.revalidate_fast(paths,&self.nonce)?;
        if !self.reattached || self.active_install.load(Ordering::SeqCst) {return Err("Private workflow requires a closed original install owner".into());}
        let conn=db::open_readonly(paths).map_err(|e|e.to_string())?;
        let pending:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM job WHERE type='install_phase2_packs_v1' AND status IN ('queued','running'))",[],|row|row.get(0)).map_err(|e|e.to_string())?;
        if pending {return Err("Original install job unfinished; private workflow refused".into());}
        Ok(true)
    }
    pub(super) fn start_explicit_runner(self:&Arc<Self>, paths:&AppPaths, safe_mode:Arc<AtomicBool>) -> voxvulgi_engine::Result<jobs::JobRunnerHandle> {
        if self.explicit_runner_policy(paths,&safe_mode).map_err(voxvulgi_engine::EngineError::InstallFailed)? {
            jobs::start_runner(paths.clone())
        } else {
            jobs::start_runner_with_install_proof_guard(paths.clone(),self.runner_guard(safe_mode))
        }
    }
    pub(super) fn phase2_admission_ready(&self, paths: &AppPaths, nonce: &str) -> Result<(),String> {
        if self.workflow {return Err("Private workflow mode refuses installer commands".into());}
        self.revalidate_fast(paths,nonce)?;
        if self.active_install.load(Ordering::SeqCst) { return Err("An actual install worker still owns this proof root".into()); }
        let conn=db::open_readonly(paths).map_err(|e|e.to_string())?;
        let pending:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM job WHERE type='install_phase2_packs_v1' AND status IN ('queued','running'))",[],|row|row.get(0)).map_err(|e|e.to_string())?;
        if pending { return Err("An original phase2 job is already queued or running".into()); }
        Ok(())
    }
    pub(super) fn acquire(self: &Arc<Self>, paths: &AppPaths, nonce: &str) -> Result<InstallProofPermit, String> {
        self.active_install.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst)
            .map_err(|_|"Another install proof worker is still active")?;
        let permit = InstallProofPermit(self.clone());
        let validation_started = std::time::Instant::now();
        self.revalidate(paths,nonce)?;
        append_diagnostics_trace_row_best_effort(paths, "install_proof_acquire_validation", serde_json::json!({
            "elapsed_ms": validation_started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            "passed": true,
            "validation": "physical_root_and_full_descendant_regular_single_link_scan",
            "physical_io_attribution": "unknown",
        }), "info");
        Ok(permit)
    }
    pub(super) fn revalidate_fast(&self, paths: &AppPaths, nonce: &str) -> Result<(), String> {
        use std::os::windows::io::AsRawHandle;
        if nonce != self.nonce || paths.runtime_mode() != voxvulgi_engine::paths::RuntimeMode::Isolated
            || std::fs::canonicalize(&paths.base_dir).map_err(|e|e.to_string())? != self.root
            || std::fs::canonicalize(paths.runtime_root()).map_err(|e|e.to_string())? != self.root {
            return Err("Install proof requires its exact owned mutable root and nonce".into());
        }
        assert_existing_path_chain_has_no_reparse_points(&self.root)?;
        let actual = directory_identity_from_handle(self.directory.as_raw_handle().cast(), self.root.clone())?;
        let reopened = open_owned_proof_directory(&self.root)?;
        if actual != self.identity || reopened.identity != self.identity { return Err("Install proof root identity changed".into()); }
        for target in [paths.tools_dir(), paths.python_venv_dir(), paths.python_models_dir(), paths.cache_dir(), paths.install_logs_dir()] {
            if !target.starts_with(&paths.base_dir) { return Err("Install proof target escaped mutable root".into()); }
            let mut existing = target.as_path();
            loop {
                match std::fs::symlink_metadata(existing) {
                    Ok(_) => break,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => { existing = existing.parent().ok_or("Missing install target ancestor")?; }
                    Err(error) => return Err(error.to_string()),
                }
            }
            assert_existing_path_chain_has_no_reparse_points(existing)?;
        }
        Ok(())
    }
    pub(super) fn revalidate(&self, paths: &AppPaths, nonce: &str) -> Result<(), String> {
        use std::os::windows::io::AsRawHandle;
        self.revalidate_fast(paths, nonce)?;
        // Seed/preparation is allowed only inside this fresh mutable root. Refuse linked
        // descendants before invoking an installer; the harness is the sole writer.
        let mut pending = vec![self.root.clone()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).map_err(|e|e.to_string())? {
                let path = entry.map_err(|e|e.to_string())?.path();
                let metadata = std::fs::symlink_metadata(&path).map_err(|e|e.to_string())?;
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 { return Err("Linked install proof descendant refused".into()); }
                if metadata.is_dir() { pending.push(path); }
                else if metadata.is_file() {
                    use std::os::windows::fs::OpenOptionsExt;
                    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
                    let file = std::fs::OpenOptions::new().access_mode(FILE_READ_ATTRIBUTES)
                        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(&path).map_err(|e|e.to_string())?;
                    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
                    if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0
                        || info.nNumberOfLinks != 1 || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return Err("Install proof requires uniquely owned regular files".into());
                    }
                } else { return Err("Nonregular install proof entry refused".into()); }
            }
        }
        Ok(())
    }
    fn revalidate_targets(&self, paths: &AppPaths, targets: &[PathBuf]) -> Result<(), String> {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
        self.revalidate_fast(paths, &self.nonce)?;
        for target in targets {
            if !target.starts_with(&paths.base_dir) || target.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
                return Err("Install proof journal target escaped owned root".into());
            }
            let mut ancestor = target.parent().ok_or("Journal target has no parent")?;
            loop {
                match std::fs::symlink_metadata(ancestor) {
                    Ok(_) => break,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => ancestor = ancestor.parent().ok_or("Journal ancestor missing")?,
                    Err(error) => return Err(error.to_string()),
                }
            }
            assert_existing_path_chain_has_no_reparse_points(ancestor)?;
            match std::fs::symlink_metadata(target) {
                Ok(metadata) => {
                    if !metadata.is_file() { return Err("Journal target must be a regular file".into()); }
                    let file = std::fs::OpenOptions::new().access_mode(FILE_READ_ATTRIBUTES)
                        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(target).map_err(|e|e.to_string())?;
                    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
                    if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0
                        || info.nNumberOfLinks != 1 || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return Err("Linked journal target refused".into());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    }
    pub(super) fn runner_guard(self: &Arc<Self>, safe_mode: Arc<AtomicBool>) -> jobs::InstallProofDispatchGuard {
        let owner = self.clone();
        let target_safe = safe_mode.clone();
        let check: Arc<dyn Fn(&AppPaths,&str)->voxvulgi_engine::Result<()> + Send + Sync> = Arc::new(move |paths, job_type| {
            if safe_mode.load(Ordering::SeqCst) { return Err(voxvulgi_engine::EngineError::InstallFailed("Safe Mode blocks install proof".into())); }
            if job_type != "install_phase2_packs_v1" {
                return Err(voxvulgi_engine::EngineError::InstallFailed("Install proof runner refuses unrelated work".into()));
            }
            owner.revalidate_fast(paths, &owner.nonce).map_err(voxvulgi_engine::EngineError::InstallFailed)
        });
        let target_owner = self.clone();
        let owner = self.clone(); let enter_check = check.clone();
        jobs::InstallProofDispatchGuard {
            check,
            targets: Arc::new(move |paths,targets| {
                if target_safe.load(Ordering::SeqCst) { return Err(voxvulgi_engine::EngineError::InstallFailed("Safe Mode blocks install proof".into())); }
                target_owner.revalidate_targets(paths,targets).map_err(voxvulgi_engine::EngineError::InstallFailed)
            }),
            enter: Arc::new(move |paths,kind| {
                enter_check(paths,kind)?;
                let permit=owner.acquire(paths,&owner.nonce).map_err(voxvulgi_engine::EngineError::InstallFailed)?;
                Ok(Box::new(permit))
            }),
        }
    }
}
#[cfg(not(windows))]
impl InstallProof {
    pub(super) fn provenance(&self) -> serde_json::Value { serde_json::Value::Null }
    pub(super) fn start_explicit_runner(self:&Arc<Self>, _: &AppPaths, _: Arc<AtomicBool>) -> voxvulgi_engine::Result<jobs::JobRunnerHandle> {Err(voxvulgi_engine::EngineError::InstallFailed("Install proof requires Windows".into()))}
    pub(super) fn phase2_admission_ready(&self, _: &AppPaths, _: &str) -> Result<(),String> { Err("Install proof requires Windows".into()) }
    pub(super) fn revalidate(&self, _: &AppPaths, _: &str) -> Result<(), String> { Err("Install proof requires Windows".into()) }
    pub(super) fn revalidate_fast(&self, _: &AppPaths, _: &str) -> Result<(), String> { Err("Install proof requires Windows".into()) }
    pub(super) fn runner_guard(self: &Arc<Self>, _: Arc<AtomicBool>) -> jobs::InstallProofDispatchGuard {
        jobs::InstallProofDispatchGuard {
            check:Arc::new(|_,_|Err(voxvulgi_engine::EngineError::InstallFailed("Install proof requires Windows".into()))),
            targets:Arc::new(|_,_|Err(voxvulgi_engine::EngineError::InstallFailed("Install proof requires Windows".into()))),
            enter:Arc::new(|_,_|Err(voxvulgi_engine::EngineError::InstallFailed("Install proof requires Windows".into()))),
        }
    }
    pub(super) fn acquire(self: &Arc<Self>, _: &AppPaths, _: &str) -> Result<InstallProofPermit,String> { Err("Install proof requires Windows".into()) }
}

pub(super) struct InstallProofPermit(Arc<InstallProof>);
impl Drop for InstallProofPermit {
    fn drop(&mut self) {
        #[cfg(windows)] self.0.active_install.store(false,Ordering::SeqCst);
    }
}

pub(super) fn prepare(args: &[String], headless: bool, root: &Path, protected: &[PathBuf]) -> Result<Option<Arc<InstallProof>>, String> {
    let reattach_requested=args.iter().any(|a|a == REATTACH_SHA_FLAG);
    let workflow=args.iter().any(|a|a == WORKFLOW_FLAG);
    if workflow && (!reattach_requested || args.iter().filter(|a|*a==WORKFLOW_FLAG).count()!=1) {
        return Err("Private workflow requires exactly one explicit workflow flag and owner reattachment SHA".into());
    }
    if !args.iter().any(|a|a == FLAG) {
        if reattach_requested || workflow {return Err("Install proof owner reattachment requires explicit install-proof mode".into());}
        return Ok(None);
    }
    if !headless { return Err("Install proof requires --agent-headless".into()); }
    if args.iter().any(|a|a == "--safe-mode" || a.starts_with("--offline-")) { return Err("Install proof cannot combine Safe Mode or offline one-shot workflows".into()); }
    #[cfg(not(windows))]
    { let _ = (root, protected); return Err("Install proof requires Windows".into()); }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use std::io::{Read, Write};
        use sha2::Digest;
        let nonce = cli_arg_value(args, "--agent-install-proof-nonce")?;
        if nonce.len() != 32 || !nonce.bytes().all(|v|v.is_ascii_hexdigit()) { return Err("Install proof nonce requires 32 hexadecimal characters".into()); }
        let temp = std::fs::canonicalize(std::env::temp_dir()).map_err(|e|e.to_string())?;
        assert_existing_path_chain_has_no_reparse_points(&temp)?;
        let expected = temp.join(format!("voxvulgi_install_proof_{}", nonce.to_ascii_lowercase()));
        let parent = root.parent().ok_or("Install proof root has no parent")?;
        if std::fs::canonicalize(parent).map_err(|e|e.to_string())? != temp
            || root.file_name() != expected.file_name() { return Err("Install proof requires its fresh nonce-named native Temp child".into()); }
        assert_existing_path_chain_has_no_reparse_points(parent)?;
        for path in protected {
            if overlap(&expected, path) { return Err("Install proof overlaps protected root".into()); }
            if path.exists() {
                let canonical = std::fs::canonicalize(path).map_err(|e|e.to_string())?;
                if overlap(&expected, &canonical) { return Err("Install proof aliases protected root".into()); }
            }
        }
        let expected_owner_sha=if reattach_requested {
            let sha=cli_arg_value(args, REATTACH_SHA_FLAG)?;
            if sha.len()!=64 || !sha.bytes().all(|v|v.is_ascii_hexdigit()) {
                return Err("Install proof owner SHA requires 64 hexadecimal characters".into());
            }
            Some(sha.to_ascii_lowercase())
        } else {None};
        // Fresh mode continues to refuse all existing roots. Reattachment never creates one.
        if expected_owner_sha.is_none() {
            std::fs::create_dir(&expected).map_err(|e|format!("Fresh install proof root refused: {e}"))?;
        }
        let root = std::fs::canonicalize(&expected).map_err(|e|e.to_string())?;
        assert_existing_path_chain_has_no_reparse_points(&root)?;
        let directory = std::fs::OpenOptions::new().access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&root).map_err(|e|e.to_string())?;
        let identity = directory_identity_from_handle(directory.as_raw_handle().cast(), root.clone())?;
        let owner_path=root.join("install_proof_owner.json");
        let (exclusive_owner, original_owner_sha256, original_owner_pid)=if let Some(expected_sha)=expected_owner_sha {
            // OPEN_EXISTING, no truncation/write: conflicts with the original or another
            // reattached READ|WRITE owner while retaining the original immutable receipt.
            let mut file=std::fs::OpenOptions::new().read(true).write(true).share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(&owner_path)
                .map_err(|e|format!("Install proof owner unavailable for exclusive reattachment: {e}"))?;
            use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
            let mut info: BY_HANDLE_FILE_INFORMATION=unsafe {std::mem::zeroed()};
            if unsafe {GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info)}==0
                || info.nNumberOfLinks!=1 || info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY)!=0 {
                return Err("Install proof owner marker must be a uniquely owned regular file".into());
            }
            let mut bytes=Vec::new();(&mut file).take(16_385).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
            if bytes.len()>16_384 || hex::encode(sha2::Sha256::digest(&bytes))!=expected_sha {
                return Err("Install proof original owner marker SHA mismatch or oversized marker".into());
            }
            let receipt:serde_json::Value=serde_json::from_slice(&bytes).map_err(|_|"Install proof original owner marker is invalid JSON")?;
            let origin_pid=receipt["pid"].as_u64().filter(|v|*v>0 && *v<=u64::from(u32::MAX)).ok_or("Original owner PID missing")? as u32;
            if receipt["schema"]!="voxvulgi.install_proof_owner.v1" || receipt["nonce"]!=nonce
                || receipt["root"].as_str().map(PathBuf::from)!=Some(root.clone())
                || receipt["volume_serial"]!=identity.volume_serial || receipt["file_id"]!=identity.file_id {
                return Err("Install proof original owner nonce or physical root identity mismatch".into());
            }
            (file,expected_sha,origin_pid)
        } else {
            let mut file=std::fs::OpenOptions::new().read(true).write(true).create_new(true).share_mode(FILE_SHARE_READ)
                .open(&owner_path).map_err(|e|e.to_string())?;
            let receipt=serde_json::json!({"schema":"voxvulgi.install_proof_owner.v1","nonce":nonce,"pid":std::process::id(),"root":root,"volume_serial":identity.volume_serial,"file_id":identity.file_id});
            let bytes=serde_json::to_string_pretty(&receipt).map_err(|e|e.to_string())?.into_bytes();
            file.write_all(&bytes).map_err(|e|e.to_string())?;file.sync_all().map_err(|e|e.to_string())?;
            (file,hex::encode(sha2::Sha256::digest(&bytes)),std::process::id())
        };
        let owner=Arc::new(InstallProof {root:root.clone(),nonce:nonce.clone(),directory,identity,
            _exclusive_owner:exclusive_owner,active_install:AtomicBool::new(false),
            original_owner_sha256,original_owner_pid,reattached:reattach_requested,workflow});
        // Reattachment must validate the entire preexisting tree before database/startup
        // mutation. Fresh mode retains its existing preparation behavior.
        if reattach_requested {owner.revalidate(&AppPaths::new(root), &nonce)?;}
        Ok(Some(owner))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn install_proof_requires_explicit_headless_fresh_root_and_nonce() {
        let nonce = "a".repeat(32);
        let root = std::env::temp_dir().join(format!("voxvulgi_install_proof_{nonce}"));
        assert!(prepare(&[], true, &root, &[]).unwrap().is_none());
        assert!(prepare(&[FLAG.into()], false, &root, &[]).is_err());
        assert!(prepare(&[FLAG.into()], true, &root, &[]).is_err());
        let args = vec![FLAG.into(),"--agent-install-proof-nonce".into(),nonce];
        assert!(prepare(&args, true, &root, &[std::env::temp_dir()]).is_err());
    }
    #[test]
    fn install_proof_owns_root_and_refuses_reuse_or_wrong_paths() {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let root = std::env::temp_dir().join(format!("voxvulgi_install_proof_{nonce}"));
        let args = vec![FLAG.into(),"--agent-install-proof-nonce".into(),nonce.clone()];
        let owner = prepare(&args, true, &root, &[]).unwrap().unwrap();
        let paths = AppPaths::new(root.clone());
        owner.revalidate(&paths, &nonce).unwrap();
        let permit = owner.acquire(&paths,&nonce).unwrap();
        assert!(owner.acquire(&paths,&nonce).is_err());
        drop(permit);
        drop(owner.acquire(&paths,&nonce).unwrap());
        assert!(owner.revalidate(&paths,"wrong").is_err());
        assert!(owner.revalidate(&AppPaths::new(root.join("other")), &nonce).is_err());
        assert!(prepare(&args,true,&root,&[]).is_err());
        std::fs::write(root.join("original.bin"), b"owned").unwrap();
        std::fs::hard_link(root.join("original.bin"), root.join("alias.bin")).unwrap();
        assert!(owner.revalidate(&paths, &nonce).is_err());
        std::fs::remove_file(root.join("alias.bin")).unwrap();
        owner.revalidate(&paths, &nonce).unwrap();
        let journal=paths.install_logs_dir().join("phase2").join("owned-job").join("state.json");
        owner.revalidate_targets(&paths,&[journal.clone()]).unwrap();
        std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
        std::fs::hard_link(root.join("original.bin"),&journal).unwrap();
        let before=std::fs::read(&journal).unwrap();
        assert!(owner.revalidate_targets(&paths,&[journal.clone()]).is_err());
        assert_eq!(std::fs::read(&journal).unwrap(),before,"refusal must not overwrite linked journal");
        std::fs::remove_file(&journal).unwrap();
        owner.revalidate_targets(&paths,&[journal]).unwrap();
        drop(owner);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn install_proof_reattach_requires_closed_owner_and_preserves_original_identity() {
        use sha2::Digest;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle,BY_HANDLE_FILE_INFORMATION};
        fn file_id(path:&Path)->(u32,u32,u32) {
            let f=std::fs::File::open(path).unwrap();let mut i:BY_HANDLE_FILE_INFORMATION=unsafe{std::mem::zeroed()};
            assert_ne!(unsafe{GetFileInformationByHandle(f.as_raw_handle().cast(),&mut i)},0);
            (i.dwVolumeSerialNumber,i.nFileIndexHigh,i.nFileIndexLow)
        }
        let nonce=uuid::Uuid::new_v4().simple().to_string();
        let root=std::env::temp_dir().join(format!("voxvulgi_install_proof_{nonce}"));
        let args=vec![FLAG.into(),"--agent-install-proof-nonce".into(),nonce.clone()];
        let original=prepare(&args,true,&root,&[]).unwrap().unwrap();
        let payload=root.join("owned.bin");std::fs::write(&payload,b"genuine owned payload").unwrap();
        let marker=root.join("install_proof_owner.json");let bytes=std::fs::read(&marker).unwrap();
        let marker_id=file_id(&marker);let payload_id=file_id(&payload);let root_id=original.identity.clone();
        let sha=hex::encode(sha2::Sha256::digest(&bytes));
        let mut reuse=args.clone();reuse.extend([REATTACH_SHA_FLAG.into(),sha.clone()]);
        assert!(prepare(&reuse,true,&root,&[]).is_err(),"Active original owner must refuse reattachment");
        drop(original);
        let mut wrong=reuse.clone();*wrong.last_mut().unwrap()="0".repeat(64);
        assert!(prepare(&wrong,true,&root,&[]).is_err());
        assert!(prepare(&reuse,true,&root.join("wrong"),&[]).is_err());
        assert!(prepare(&reuse,true,&root,&[std::env::temp_dir()]).is_err());
        assert!(prepare(&args,true,&root,&[]).is_err(),"Default fresh behavior still refuses existing root");
        assert!(prepare(&[REATTACH_SHA_FLAG.into(),sha],true,&root,&[]).is_err());
        let attached=prepare(&reuse,true,&root,&[]).unwrap().unwrap();
        assert_eq!(attached.identity,root_id);assert!(attached.reattached);
        assert_eq!(attached.provenance()["current_owner_pid"],std::process::id());
        assert_eq!(attached.provenance()["original_owner_pid"],std::process::id());
        assert!(prepare(&reuse,true,&root,&[]).is_err(),"Second attached owner must refuse");
        assert_eq!(std::fs::read(&marker).unwrap(),bytes);
        assert_eq!(std::fs::read(&payload).unwrap(),b"genuine owned payload");
        assert_eq!(file_id(&marker),marker_id);assert_eq!(file_id(&payload),payload_id);
        drop(attached);std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn install_proof_workflow_opt_in_selects_original_runner_and_refuses_installers() {
        use sha2::Digest;
        let nonce=uuid::Uuid::new_v4().simple().to_string();
        let root=std::env::temp_dir().join(format!("voxvulgi_install_proof_{nonce}"));
        let args=vec![FLAG.into(),"--agent-install-proof-nonce".into(),nonce.clone()];
        assert!(prepare(&[FLAG.into(),WORKFLOW_FLAG.into()],true,&root,&[]).is_err());
        let owner=prepare(&args,true,&root,&[]).unwrap().unwrap();
        let paths=AppPaths::new(root.clone());
        assert!(!owner.explicit_runner_policy(&paths,&AtomicBool::new(false)).unwrap());
        let sha=hex::encode(sha2::Sha256::digest(std::fs::read(root.join("install_proof_owner.json")).unwrap()));
        drop(owner);
        let mut reuse=args.clone();reuse.extend([REATTACH_SHA_FLAG.into(),sha,WORKFLOW_FLAG.into()]);
        let owner=prepare(&reuse,true,&root,&[]).unwrap().unwrap();
        db::ensure_schema(&paths).unwrap();
        assert!(owner.explicit_runner_policy(&paths,&AtomicBool::new(false)).unwrap());
        assert!(owner.phase2_admission_ready(&paths,&nonce).is_err());
        assert!(owner.explicit_runner_policy(&paths,&AtomicBool::new(true)).is_err());
        assert!(owner.explicit_runner_policy(&AppPaths::new(root.join("wrong")),&AtomicBool::new(false)).is_err());
        let conn=db::write_context(&paths).unwrap();
        conn.execute("INSERT INTO job(id,type,status,progress,params_json,created_at_ms,logs_path) VALUES('original-install','install_phase2_packs_v1','queued',0,'{}',1,'owned')",[]).unwrap();
        drop(conn);
        assert!(owner.explicit_runner_policy(&paths,&AtomicBool::new(false)).is_err());
        db::AppDatabase::for_paths(&paths).unwrap().shutdown_and_drain(std::time::Duration::from_secs(5)).unwrap();
        drop(owner);std::fs::remove_dir_all(root).unwrap();
    }

}
