# 🔥 TokenTorch

Menu bar app that shows your Claude AI usage limits — and warns you before you hit them.

## Features

- **Dual progress bars in the menu bar** — session (5h) and weekly (7d) usage at a glance
- **Predictive colors** — extrapolates your burn rate to warn you *before* you hit limits
- **Blink animation** — tray icon blinks red when a limit is imminent
- **Auto-refreshing session** — picks up rotated session keys automatically
- **Update notifications** — checks GitHub releases and prompts when a new version is available
- **Cross-platform** — macOS (universal) and Windows

## Install

Download the latest release from [**Releases**](https://github.com/TekSiDoT/tokentorch/releases):

| Platform | File |
|----------|------|
| macOS | `TokenTorch_x.x.x_universal.dmg` |
| Windows | `TokenTorch_x.x.x_x64-setup.exe` |

> macOS: Right-click → Open on first launch (app is unsigned).

## Setup

1. Open [claude.ai](https://claude.ai) in your browser (logged in)
2. DevTools → Application → Cookies → `https://claude.ai`
3. Copy `sessionKey` and `lastActiveOrg` values
4. Paste into the TokenTorch setup window

## Colors

| Color | Meaning |
|-------|---------|
| 🟢 Green | On pace — you're fine |
| 🟡 Yellow | Elevated — usage trending toward the limit |
| 🔴 Red | Projected to hit the limit before reset |
| 🔴 Blink | Limit imminent or already hit |

Colors are based on *projected* usage at reset time, not just current utilization.

## Build from source

Requires: [Rust](https://rustup.rs), [Node.js](https://nodejs.org) ≥ 20

```sh
git clone https://github.com/TekSiDoT/tokentorch.git
cd tokentorch
npm install
npx tauri build
```

Output: `src-tauri/target/release/bundle/`

## Disclaimer

**This is an unofficial tool** and is not affiliated with, endorsed by, or supported by Anthropic PBC.

This application accesses Claude's web API using browser-based authentication methods. **This may violate Anthropic's Terms of Service.** By using TokenTorch, you acknowledge that:

- Anthropic may block, restrict, or terminate access at any time
- Your Claude account could be affected by using unofficial API clients
- **Use at your own risk** — the developer assumes no liability for any consequences

**Data privacy:**

- Session keys are stored in the OS keychain (macOS Keychain / Windows Credential Manager) — encrypted, device-local only
- No data is sent to third-party servers or collected by the developer
- The only outbound connections are to `claude.ai` (usage API) and `api.github.com` (update checks)

## Acknowledgments

Development supported by [Skippy](https://beercan-with-a-soul.dev/).

## License

[MIT](LICENSE)
