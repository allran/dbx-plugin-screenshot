# Screenshot

Capture displays and keep a local screenshot history inside a DBX workbench.

Uses a Rust Sidecar (`xcap`) for system-level screen capture — not the Host clipboard API.

## Features (0.1.0)

- Proactive Screen Recording permission prompt (request / open Settings / recheck)
- Capture primary or selected display
- Interactive region capture (macOS system selection UI)
- Optional 3s / 5s delay
- History gallery with preview, copy, favorite, reveal-in-folder, clear
- Command palette + app toolbar entry

## Permissions

None declared. Sidecar reads the screen with the current OS user privileges.

**macOS:** grant **Screen Recording** to DBX in System Settings → Privacy & Security, then restart DBX.

## Develop

Requires Node.js 22+ and a Rust toolchain.

```bash
npm install
dbx-plugin dev --path . --port 5190
```

UI builds from `src/` into `ui/`. Full capture must be verified in the real DBX desktop host (dev host does not simulate screen capture permissions).

## Package

```bash
npm ci && npm run build
dbx-plugin package .
```

Produces an unsigned platform-specific candidate under `dist/` (native Sidecar — not `universal`).

## License

Apache-2.0
