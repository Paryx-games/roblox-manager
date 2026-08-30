# Security Policy

RM handles Roblox authentication cookies (`.ROBLOSECURITY`), Discord webhook bearer URLs, CSRF tokens, launch tickets, encrypted account storage, and Windows process control. A vulnerability here doesn't mean only "the app crashes"; it can mean someone's Roblox account or Discord channel gets compromised. Report accordingly.

## Supported versions

RM is under active development with no formal release/LTS tracks yet. Security fixes are made against the latest version on the default branch. If you're running an older build, update before assuming a report is still relevant.

## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.** Public issues are for ordinary bugs, not exploits. Filing a cookie-theft vector publicly can expose users before a fix exists.

Report privately through [GitHub Security Advisories](../../security/advisories/new) for this repository. This lets us discuss and fix the issue with you before anything becomes public, and you'll get credit in the eventual advisory if you want it.

Include, where relevant:

- What component is affected (`ram_core` vs `ram_ui`, and which module if you know it)
- Steps to reproduce, or a proof of concept
- What an attacker could actually achieve (read a cookie? decrypt the account store? get RM to execute something? escalate via a launched Roblox process?)
- Whether it requires local access, a malicious server response, a malicious Place/Job ID or launch-data value, or something else

You do not need a working exploit to report something - a credible theoretical issue (e.g. "this encryption mode is weak against X") is worth reporting too.

## What's in scope

- Anything that could expose or exfiltrate `.ROBLOSECURITY` cookies, Discord webhook URLs, CSRF tokens, or launch tickets - including via logs, crash dumps, or IPC between `ram_core`/`ram_ui`
- Weaknesses in the AES-256-GCM account store encryption, Windows Credential Manager key handling, or the Argon2id password-derived key mode
- Ways to bypass or corrupt RM's atomic persistence such that account data is silently lost or tampered with
- Privilege escalation or arbitrary code execution via RM's process control (Roblox client launching, mutex patching, window management)
- Log or file scrubbing failures - i.e. if you find a spot where cookies, tokens, or user paths leak into logs despite the intended scrubbing
- Leaks across the UI/backend bridge, browser-login subprocess, or other local IPC boundary

## Discord webhook handling

Discord webhook URLs are bearer credentials: anyone who has one can post to its channel. RM treats them as secrets.

- The webhook URL is not serialized into `config.json`.
- RM stores it in Windows Credential Manager under the existing `RM-Rust` service, protected by the Windows user profile.
- The URL is validated before use, and backend requests revalidate it before contacting Discord.
- Logs and user-facing errors never include the URL or its token.
- The test action updates the webhook name to `Roblox Manager`, applies the bundled RM logo, and posts a confirmation message to Discord.

If a webhook URL is exposed, delete or rotate it immediately from the Discord channel's Integrations settings. Do not paste real webhook URLs into issues, commits, screenshots, logs, or chat.

## What's out of scope

- Reports that RM's multi-instance mutex patch "could" get you flagged by Roblox anti-cheat (Hyperion) - this is a known, documented risk in the README, not a vulnerability in RM
- Reports based on you having already pasted your own `.ROBLOSECURITY` cookie somewhere public - see the handling note below, this is a user-side mistake, not an RM vulnerability
- General Roblox platform vulnerabilities not specific to RM (report those to Roblox directly)
- Denial-of-service against your own local instance of the app

## Handling cookies responsibly when reporting

If your report involves a real cookie (for example, when demonstrating exfiltration), **do not paste it into the advisory, an issue, a screenshot, or Discord**. Revoke it if it may have been exposed, redact it, describe the mechanism, and let a maintainer request a safe reproduction if needed. Treat any `.ROBLOSECURITY` value as equivalent to a plaintext password at all times; a demonstration report is not an exception.

## Response expectations

This is a small, independently maintained project - there's no dedicated security team and no SLA. That said, cookie/encryption-related reports get priority over everything else, including in-progress features. You should expect an initial response acknowledging the report; how fast a fix ships depends on severity and complexity.

## Disclosure

Please give us a reasonable window to investigate and patch before any public disclosure. Once a fix is out, we're happy to credit reporters in the advisory - just say whether you want to be named.
