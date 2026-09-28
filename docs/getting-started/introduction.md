---
description: Learn how Roblox Manager organizes accounts and runs Roblox sessions.
icon: circle-info
---

# Introduction

Roblox Manager keeps Roblox accounts and sessions in one Windows app. Organize accounts, choose a game, and launch the sessions you need.

These guides describe the Tauri v2 interface on the current branch. Older published releases may use egui and have different controls. The migration retains the existing Rust logic and encrypted account-store format.

### What you can do

Roblox Manager helps you:

* Organize accounts with aliases, groups, and pins.
* Launch games for one account or several accounts at once.
* Save launch presets, private servers, and server targets.
* Run multiple clients and arrange open windows.
* Track account presence and developer asset uploads.
* Inspect running clients and compare inventories across accounts.

### Who it is for

Roblox Manager supports anyone who regularly uses multiple Roblox accounts. It reduces account switching and keeps repeat sessions organized.

### Platform support

Roblox Manager runs on Windows.

* **Windows 10 and Windows 11, 64-bit**, are the target platforms.
* The Tauri interface requires **Microsoft WebView2 Runtime**.
* Compatibility with a target platform does not mean every installer/runtime combination has been manually tested. Check the release notes for testing limitations.

macOS and Linux are not supported.

### Important notes

{% hint style="warning" %}
Never share your `.ROBLOSECURITY` cookie. It can access your Roblox account.
{% endhint %}

Roblox updates can affect multi-instance behavior. Roblox Manager is independent and unaffiliated with Roblox Corporation.

### Next steps

1. [installing.md](installing.md "mention").
2. [first-session.md](first-session.md "mention").
3. [privacy-and-security.md](../security/privacy-and-security.md "mention").
