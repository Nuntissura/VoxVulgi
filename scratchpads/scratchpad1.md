 Use this installer for the smoke/update pass:
                                                                                                  
  D:\Projects\LLM                                                                                 
  projects\VoxVulgi\product\desktop\build_target\Current\release\bundle\nsis\VoxVulgi_0.1.5_x64-  
  setup.exe                                                                                       
                                                                                                  
  MSI variant if needed:                                                                          
                                                                                                  
  D:\Projects\LLM                                                                                 
  projects\VoxVulgi\product\desktop\build_target\Current\release\bundle\msi\VoxVulgi_0.1.5_x64_en-
  US.msi                                                                                          
                                                                                                  
  Reply back using the same IDs.                                                                  
                                                                                                  
  Install / Startup                                                                               
                                                                                                  
  - ST-001 Installer opens and shows Update/Repair, Full reinstall, and Uninstall.   
      >> correct             
  - ST-002 Update/repair path completes without wiping app data.     
                               
  - ST-003 Installed app reports version 0.1.5.                                                   
  - ST-004 First launch shows startup progress instead of a long silent freeze.                   
  - ST-005 No unexpected tool-download prompts appear immediately on first launch.                
  - ST-006 Startup remains responsive enough while other CPU-heavy apps are running.              
                                                                                                  
  Options / Storage Roots                                                                         
                                                                                                  
  - ST-007 Options shows storage roots clearly.                                                   
  - ST-008 The old shared-root block is no longer duplicated across feature windows.              
  - ST-009 Per-feature roots can be set and persist after restart.                                
  - ST-010 Feature windows show paths consistent with what Options says.                          
  - ST-011 Selecting an existing folder creates missing app-managed subfolders when needed.       
  - ST-012 Selecting an existing archive folder indexes/reuses it instead of acting like it is    
    missing.                                                                                      
                                                                                                  
  Video Archiver                                                                                  
                                                                                                  
  - ST-013 Video Archiver no longer shows Localization ingest controls.                           
  - ST-014 Video Archiver no longer shows the shared-root ownership panel.                        
  - ST-015 Video URL download defaults to MP4-compatible output when auth is not needed.          
  - ST-016 Auth/session UI is visible and understandable for protected video downloads.           
  - ST-017 Pasting cookie JSON works for auth-required downloads.                                 
  - ST-018 Header-style cookie paste works for auth-required downloads.                           
  - ST-019 Saved/authenticated subscription flows reuse session input correctly.                  
  - ST-020 YouTube subscriptions panel exposes open folder.                                       
  - ST-021 Wide subscription panels resize correctly and do not hide actions at normal window     
    widths.                                                                                       
                                                                                                  
  Instagram Archiver                                                                              
                                                                                                  
  - ST-022 Instagram Archiver name is correct everywhere.                                         
  - ST-023 Session-cookie input is easy to find at the top of the relevant Instagram surfaces.    
  - ST-024 Instagram one-shot download works with pasted session input.                           
  - ST-025 Instagram saved subscription/heartbeat flow works with session input.                  
  - ST-026 Recent Instagram thumbnails are shown uncropped.                                       
                                                                                                  
  Localization Studio                                                                             
                                                                                                  
  - ST-027 Localization Studio contains the ingest/import block in context.                       
  - ST-028 ASR language and auto selection stay visible and understandable.                       
  - ST-029 A real localization flow produces a non-silent English dub.                            
  - ST-030 The localized output video is MP4.                                                     
  - ST-031 The new Localization Library makes source, working artifacts, and deliverables easy to 
    find.
  - ST-032 There are obvious buttons to open the source video, artifact folder, and exported
    deliverables.
  - ST-033 Subtitle export paths are obvious and the files are easy to open.
  - ST-034 Dub audio-track paths are obvious and the files are easy to open.
  - ST-035 Benchmark / backend / QC surfaces are actually discoverable in the UI.
  - ST-036 Benchmark winner promotion into template/cast-pack defaults is visible enough to use.
  - ST-037 Experimental backend adapter features are visible enough to find and understand.
  - ST-038 Variant reruns / QC reruns / artifact log matching are visible and make sense in
    practice.
  - ST-039 Multi-reference voice cleanup remains non-destructive in the UI flow.
  - ST-040 Batch dubbing flow remains stable with larger selections.

  Media Library

  - ST-041 Open file works for items stored on D: or another non-system drive.
  - ST-042 The default list-style Media Library feels better than tiles for large archives.
  - ST-043 It is clear which entries are Subscription, Playlist, Folder, and Single file.
  - ST-044 Filtering/grouping for videos vs images behaves sensibly.
  - ST-045 Large legacy-imported library remains usable and responsive.

  Diagnostics / Support

  - ST-046 Diagnostics opens without a long freeze.
  - ST-047 Diagnostics loading state is clear enough while sections are still loading.
  - ST-048 Tool/model state is understandable, including what is bundled, required, optional, or
    demo/test.
  - ST-049 App-state snapshot export is visible and useful for support.
  - ST-050 If a tool is still loading, the blocked action explains that clearly enough.

  Shell / Interaction

  - ST-051 The explicit Move window affordance is easy to use.
  - ST-052 Text selection works in logs/messages/error text.
  - ST-053 Scrollbars can be grabbed normally.
  - ST-054 Corner resize hitbox feels large enough.
  - ST-055 Panel-local scrolling works where tables/lists are wide.
  - ST-056 Switching windows no longer causes major reloads/freezes.

  Legacy / Migration

  - ST-057 Legacy 4KVDP groups still appear correctly after the migration/import work.
  - ST-058 The app stays responsive with the migrated legacy library.
  - ST-059 No path or artifact references still point to the old Build Target folder in normal
    operator flows.

  If you want, paste it back in this exact format with - correct, - broken, or notes per ID, and
  I’ll turn it directly into governed remediation packets.