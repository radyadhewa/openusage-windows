# OpenUsage

A re-imagined menu bar app for tracking AI coding tool subscriptions like Cursor, Claude, Codex and more, all in one place with a heavy focus on UX, stability, and speed.

Inspired by [CodexBar](https://github.com/steipete/CodexBar) by [@steipete](https://github.com/steipete).

Same idea, very different approach:

- **Plugin architecture** — add/update providers without waiting on maintainers
- **Cross-platform** — Tauri instead of native Swift
- **Web tech** — React + TypeScript frontend

## Status

Early development.

## Stack

- Tauri 2 + Rust
- React 19, Tailwind 4, Base UI
- Vite 7, bun

## Install

### From source

```bash
git clone https://github.com/robinebers/openusage
cd openusage
bun install
bun tauri build
```

Built app lands in `src-tauri/target/release/bundle/`.

### Development

```bash
bun install
bun tauri dev
```

## Providers

Current (mocked):
- [Cursor](docs/providers/cursor.md) — plan, usage, on-demand
- [Claude](docs/providers/claude.md) — session, weekly, extra usage
- [Codex](docs/providers/codex.md) — session, weekly, code reviews, extra usage

Adding a provider = adding a plugin. Docs coming.

## Contributing

PRs welcome. Keep it simple:

- No feature creep
- No AI-generated commit messages
- Test your changes

## License

MIT

## Credits

- [@steipete](https://github.com/steipete) for CodexBar and all the people that contributed to it.
