---
file_id: WP-0267-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-16
---

<topic id="operator-request" status="active" version="v1" wp="WP-0267" updated_at="2026-07-16">

# Operator request

- Make the Options instructions unmistakable when YouTube sign-in fails.
- Make the flow work for a new user without expecting them to understand or paste cookies.
- Match the old downloader's user-facing goal of simply signing into YouTube where technically possible.
- When YouTube blocks access, stop presenting VoxVulgi as connected and make the required recovery action obvious.

</topic>

<topic id="research-basis" status="active" version="v1" wp="WP-0267" updated_at="2026-07-16">

# Research basis

- 4K Download's official sign-in guide documents a proprietary in-app Google authorization window and says Google may identify that session as an Apple iPad: https://www.4kdownload.com/faq/faq-howto-log-into-youtube-in-app/2
- yt-dlp's current YouTube extractor guidance says YouTube OAuth login no longer works with yt-dlp and that authenticated downloads require cookies: https://github.com/yt-dlp/yt-dlp/wiki/Extractors
- yt-dlp's FAQ identifies `--cookies-from-browser` as the easiest supported cookie path and manual Netscape cookies as the fallback: https://github.com/yt-dlp/yt-dlp/wiki/FAQ
- Google's OAuth policy forbids directing OAuth through an app-controlled embedded user-agent: https://developers.google.com/identity/protocols/oauth2/policies
- The installed Tauri opener provides an external-browser URL path, while VoxVulgi already persists and tests a named yt-dlp browser-cookie source.

# Selected approach

- Lead with the user goal rather than cookie terminology: open YouTube in the selected normal browser, let the user sign in there, then explicitly verify that browser session.
- Keep Google credentials out of VoxVulgi. Do not imitate 4K Download's proprietary Apple-iPad-style authorization or add a Google login WebView that Google can block.
- Persist `last_verified_at_ms` and `reconnect_required_at_ms` with the existing YouTube auth configuration. A successful exact-source preflight marks the connection ready; a rejected preflight or corroborated runtime auth block marks VoxVulgi's connection as requiring sign-in again.
- Preserve the selected browser source when access is rejected so queued recurring work remains held by the matching auth circuit. Do not delete or modify the user's real Google/browser session.
- Keep manual YouTube-only cookie import as an advanced fallback after the normal browser flow fails.

# Rejected options

- Embedded Google OAuth/login WebView: Google disallows app-controlled embedded OAuth user-agents, and an OAuth token would not authenticate yt-dlp media downloads.
- Pretending a saved browser name means connected: configuration is not proof that YouTube accepted the session.
- Clearing the browser source when rejected: this loses the auth-key linkage that holds recurring work and can turn one account problem back into repeated guest failures.
- Logging the user out of their real browser: destructive to unrelated Google sessions and unnecessary for VoxVulgi recovery.

</topic>

<topic id="scope-and-acceptance" status="active" version="v1" wp="WP-0267" updated_at="2026-07-16">

# Scope

- Replace the current implementation-led browser-cookie copy in the existing Options section with a three-step sign-in workflow.
- Add an explicit action that opens YouTube in the selected supported browser only when the user clicks it.
- Persist verified and reconnect-required state, and update it from preflight and corroborated runtime rejection.
- Show a prominent Ready / Not connected / Sign-in required status with exact recovery steps.
- Change manual cookie import to an advanced fallback that saves and tests in one action.
- Explain why the secure external-browser flow differs from the old downloader behind a disclosure.

# Acceptance criteria

- A new user can complete the normal flow without exporting or pasting cookies.
- Failure instructions say: reopen YouTube in the selected browser, sign out and back in if needed, confirm a video plays, close that browser fully, then retry verification; manual import is the next fallback.
- A successful exact-source preflight persists a verified timestamp and clears reconnect-required state.
- A rejected exact-source preflight or corroborated runtime auth block persists reconnect-required state and does not remain visually green/connected after restart.
- Rejection does not delete subscriptions, playlists, jobs, media, or browser cookies, and recurring YouTube work remains held by the existing auth circuit.
- The exact operator URL remains the runtime verification case.
- Focused frontend/engine tests, desktop build checks, a semantic version increment, and headless bridge visual verification pass.

</topic>

<topic id="red-team" status="active" version="v1" wp="WP-0267" updated_at="2026-07-16">

# Red team

- Wrong browser selected: the opener must target the selected browser; if it cannot be found, state that clearly instead of silently opening another profile.
- Browser database locked: recovery explicitly tells the user to close every window and background instance of that browser before testing.
- Browser is open and signed in but YouTube rotates/rejects cookies: status becomes Sign-in required and recurring work stays held; the UI does not claim success from a guest fallback.
- Existing users upgrade with no verification timestamps: show Not checked yet and offer one verification action rather than falsely claiming ready or deleting their configuration.
- Manual cookie import is saved but rejected: save-and-test returns the same recovery status and keeps raw technical details available without making them the headline.
- External sign-in steals focus during automated verification: do not invoke the opener in headless tests; verify the command contract and UI visually through the bridge.

</topic>
