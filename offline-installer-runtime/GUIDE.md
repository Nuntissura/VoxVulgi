# VoxVulgi full-offline installer

## Repository placement

`offline-installer-runtime/` is the offline-installer worktree. It is a top-level sibling of `product/` and `governance/`.

Installer scripts, minimal recovery state, temporary packaging files, payload archives, `Install_VoxVulgi.exe`, and `simple-offline-installer.iso` stay under `offline-installer-runtime/`. Files under `product/` are read-only packaging inputs.

## Definition

A VoxVulgi full-offline installer is one ISO named `simple-offline-installer.iso` containing:

```text
Install_VoxVulgi.exe
payload/
  payload_tools.7z
  payload_models.7z
  payload_huggingface.7z
  payload_cosyvoice_venv.7z
  payload_voice_backends.7z
```

The user launches only `Install_VoxVulgi.exe`. The payload archives are internal ISO files; the user does not extract, configure, or manage them.

The installer packages the existing working VoxVulgi app/core setup and the existing working dependency directories. Packaging does not download, rebuild, reinstall, regenerate, or reconstruct the app or dependencies.

The ISO includes the app, models, Hugging Face cache, Python runtimes and environments, FFmpeg, providers, voice backends, and every dependency used from a default path.

Installation and first launch require no network access, downloads, terminal, PowerShell, pip, Python setup, package manager, developer tool, or manual path configuration.

## Installer behavior

- For a new installation, install the app and copy the bundled dependencies to their stable local runtime paths.
- For an existing installation, show the pre-maintenance explainer and these exact actions:
  - `Update`
  - `Reinstall (keep preferences and options)`
  - `Full reinstall`
  - `Uninstall (keep preferences and options)`
  - `Full uninstall`
- `Update` replaces only installer-managed app and dependency files.
- `Update`, `Reinstall (keep preferences and options)`, and `Uninstall (keep preferences and options)` preserve `%APPDATA%\com.voxvulgi.voxvulgi`, including settings, options, database, subscriptions, playlists, library metadata, and other retained state.
- Only `Full reinstall` and `Full uninstall` may remove retained app data.
- The installed app loads the bundled dependencies from their installed local paths, not from system-wide tools or environments.
- Detect and close a running VoxVulgi instance before replacing managed files.
- If installation or update fails, restore the previous managed files and leave retained user data untouched.
- Never modify, rename, overwrite, delete, or repurpose another installer.

## What this work is not

- It is not a clean-room app or dependency build.
- It is not a downloader or dependency-acquisition pipeline.
- It does not run Cargo, Tauri, npm, pip, Git, Git LFS, Hugging Face, ModelScope, model warmups, or environment creation.
- It is not a dependency upgrade, reproducibility project, governance project, reporting project, or proof-bundle project.
- Caches, archives, logs, state files, and partial outputs are not the deliverable. The working ISO is the deliverable.

## Packaging method

- Use the current 64-bit Inno Setup 7 release.
- Embed the existing working app/core setup in `Install_VoxVulgi.exe`.
- Keep the five dependency archives under the ISO's `payload/` folder.
- Use Inno Setup's native `external extractarchive ignoreversion` handling with its SHA-256 `Hash` parameter.
- Set `ArchiveExtraction=enhanced/nopassword` for normal memory use with large `.7z` contents.
- Use non-solid `.7z` archives. The official Inno documentation advises against solid archives for `extractarchive` because they can reduce extraction performance.
- Keep a stable Inno `AppId` across versions and enable `CloseApplications` so Windows Restart Manager handles running app files.
- Create the ISO with `oscdimg -u2 -udfver102 -m`.

Official implementation references:

- [Inno Setup downloads](https://jrsoftware.org/isdl.php)
- [Inno Setup external archive extraction and SHA-256 Hash](https://jrsoftware.org/ishelp/topic_filessection.htm)
- [Inno Setup archive extraction modes](https://jrsoftware.org/ishelp/topic_setup_archiveextraction.htm)
- [Inno Setup AppId](https://jrsoftware.org/ishelp/topic_setup_appid.htm)
- [Inno Setup CloseApplications](https://jrsoftware.org/ishelp/topic_setup_closeapplications.htm)
- [Microsoft Oscdimg options](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/oscdimg-command-line-options?view=windows-11)

## Minimal recovery state

Use only this packaging workspace:

```text
offline-installer-runtime/
  offline_installer_work/
    state.json
    partial/
    iso_root/
      Install_VoxVulgi.exe
      payload/
        payload_tools.7z
        payload_models.7z
        payload_huggingface.7z
        payload_cosyvoice_venv.7z
        payload_voice_backends.7z
    delivery/
      simple-offline-installer.iso
```

`state.json` contains only:

- Source path and source hash for each app/dependency input.
- Input hash and output hash for each completed archive.
- Input hash and output hash for `Install_VoxVulgi.exe`.
- Input hash and output hash for `simple-offline-installer.iso`.
- The most recent terminal error, if any.

Recovery rules:

1. Write a new archive, wrapper, or ISO to `partial/`.
2. After the command succeeds and the output hash is recorded, rename the output to its final path.
3. On retry, reuse a final output when its recorded input hash and output hash still match.
4. Rebuild only the missing or mismatched output and the outputs that depend on it.
5. On failure, record the terminal error and remove only the current partial output.
6. Never delete a valid completed archive, wrapper, or ISO merely because a later step failed.

## Build workflow

1. Locate the existing working app/core setup and five existing working dependency roots.
2. Verify the required source paths, Inno Setup, 7-Zip, Oscdimg, output path, and free disk space before compression.
3. Hash the six source inputs and update `state.json`.
4. Create or reuse the five non-solid dependency archives independently.
5. Compile or reuse `Install_VoxVulgi.exe` from the app/core setup, archive hashes, and Inno source.
6. Assemble `iso_root/` and create or reuse `simple-offline-installer.iso`.
7. Run one final test with network access disabled: existing-install `Update`, launch VoxVulgi, confirm bundled dependencies are healthy, confirm zero downloads, and confirm retained user state remains present and usable.

## Checklist

- [ ] Existing working app/core setup identified.
- [ ] Five existing working dependency roots identified.
- [ ] Packaging invokes no app build, dependency build, downloader, package manager, or model tool.
- [ ] Five archives exist and their hashes match `state.json`.
- [ ] `Install_VoxVulgi.exe` exists at the ISO root.
- [ ] `simple-offline-installer.iso` exists.
- [ ] Offline existing-install `Update` succeeds.
- [ ] VoxVulgi launches with all bundled dependencies healthy and zero downloads.
- [ ] Settings, options, database, subscriptions, playlists, and library metadata remain present and usable.
- [ ] No other installer was modified, renamed, overwritten, deleted, or repurposed.
