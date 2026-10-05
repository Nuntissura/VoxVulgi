//! Explicit disposable install proof; never selected implicitly by runtime mode.
use super::*;
use std::path::{Path, PathBuf};

pub(super) const FLAG: &str = "--agent-install-proof";

#[cfg(windows)]
pub(super) struct InstallProof {
    root: PathBuf,
    nonce: String,
    directory: std::fs::File,
    identity: OfflineProofDirectoryIdentity,
    _exclusive_owner: std::fs::File,
    active_install: AtomicBool,
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
    pub(super) fn phase2_admission_ready(&self, paths: &AppPaths, nonce: &str) -> Result<(),String> {
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
        self.revalidate(paths,nonce)?;
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
    if !args.iter().any(|a|a == FLAG) { return Ok(None); }
    if !headless { return Err("Install proof requires --agent-headless".into()); }
    if args.iter().any(|a|a == "--safe-mode" || a.starts_with("--offline-")) { return Err("Install proof cannot combine Safe Mode or offline one-shot workflows".into()); }
    #[cfg(not(windows))]
    { let _ = (root, protected); return Err("Install proof requires Windows".into()); }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use std::io::Write;
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
        // create_dir refuses any existing directory, including a previous proof.
        std::fs::create_dir(&expected).map_err(|e|format!("Fresh install proof root refused: {e}"))?;
        let root = std::fs::canonicalize(&expected).map_err(|e|e.to_string())?;
        assert_existing_path_chain_has_no_reparse_points(&root)?;
        let directory = std::fs::OpenOptions::new().access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&root).map_err(|e|e.to_string())?;
        let identity = directory_identity_from_handle(directory.as_raw_handle().cast(), root.clone())?;
        let mut exclusive_owner = std::fs::OpenOptions::new().read(true).write(true).create_new(true).share_mode(FILE_SHARE_READ)
            .open(root.join("install_proof_owner.json")).map_err(|e|e.to_string())?;
        let receipt = serde_json::json!({"schema":"voxvulgi.install_proof_owner.v1","nonce":nonce,"pid":std::process::id(),"root":root,"volume_serial":identity.volume_serial,"file_id":identity.file_id});
        exclusive_owner.write_all(serde_json::to_string_pretty(&receipt).map_err(|e|e.to_string())?.as_bytes()).map_err(|e|e.to_string())?;
        exclusive_owner.sync_all().map_err(|e|e.to_string())?;
        Ok(Some(Arc::new(InstallProof {root, nonce, directory, identity, _exclusive_owner:exclusive_owner,active_install:AtomicBool::new(false)})))
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
}
