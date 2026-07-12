# AGENTS.md

This file provides guidance to Lingma (lingma.aliyun.com) when working with code in this repository.

## Build & Run

```bash
# System dependencies (Fedora)
sudo dnf install gtk4-devel libadwaita-devel

# Dev build
cargo build

# Release build + install to /usr
meson setup builddir --prefix=/usr --buildtype=release
sudo meson install -C builddir

# Install extension only (user-level, no sudo)
meson compile -C builddir install-extension
```

## Architecture

Two-process GNOME app: Rust/GTK4 frontend + GNOME Shell extension for select-to-translate (Wayland, GNOME 45–50 only).

### Rust Process (`src/`)

- `src/engine/` — Dictionary loading (StarDict, MDX), search, fuzzy matching, online translation
- `src/views/` — GTK4 UI (sidebar, collapsible per-dict articles, search bar)
- `src/config.rs` — All config via **GSettings** (no state.json). Schema: `schemas/io.github.tigertall.QuickDict.gschema.xml`
- `src/application.rs` — App setup, D-Bus actions, dict loading, single-instance window via `connect_activate` + `connect_open`
- `src/preferences.rs` — Modal preferences over main window (`set_transient_for`)
- `src/capture/dbus_service.rs` — zbus-based `io.github.tigertall.QuickDict.Translator` for extension lookup

### DictManager

`Vec<Arc<dyn Dictionary>>` with parallel `Vec<bool>` for active states. All dict types implement `Dictionary` trait:
- `lookup_local(word)` — local dicts only (StarDict, MDX), returns `Vec<ArticleData>`
- `try_online(word)` — first online dict hit, returns `Option<ArticleData>`. No hardcoded Baidu-specific logic
- **D-Bus handler**: local first, empty → online. **Main window**: local + online parallel

To add a new online dictionary: implement `Dictionary` trait with `DictKind::Online("id")`, register via `add_online_dict()`. Done — no business code changes.

### GNOME Shell Extension (`ext/quickdict-focus@tigertall.github.com/`)

- D-Bus: `io.github.tigertall.QuickDict.Translator.Lookup(word)` → JSON results
- Schema is auto-compiled by `gnome-extensions install`; manual `cp` requires `glib-compile-schemas` afterward
- All signals connected in `enable()` must be disconnected in `disable()` (EGO-L-003)

### Desktop Integration

- `.desktop`: `data/io.github.tigertall.QuickDict.desktop` (manual) / `.desktop.in` (meson template, `i18n.merge_file` generates to builddir)
- `DBusActivatable=false`, `StartupNotify=false` — direct Exec launch, consistent across GNOME launch methods
- AppId: `io.github.tigertall.QuickDict`
- Binary installed to `/usr/bin/`, schema to `/usr/share/glib-2.0/schemas/` — no wrapper scripts, no custom env vars

## Key Design Decisions

1. **Online dict abstraction** — `Dictionary` trait + `DictKind::Online("id")`, never hardcode specific online service names in business logic
2. **Single-instance** — explicit check in `connect_activate` + `connect_open` (both signals share same closure), backed by `adw::Application` `application_id`
3. **Dead code** — `#[allow(dead_code)]` is forbidden
4. **Version** — `env!("CARGO_PKG_VERSION")` in Rust, `project('quickdict', version: ...)` in meson. Cargo.toml is the source of truth

### Query Logic Rules

Two distinct lookup strategies, must not be mixed:

| Scenario | Strategy | Implementation |
|----------|----------|---------------|
| **Select-to-translate** (extension D-Bus) | Local first, online fallback only if local empty | `lookup_local` → empty check → `try_online` |
| **Main window search / Open in Dictionary** | Local + online parallel | `lookup_local` + `try_online`, merge results |

Rationale: select-to-translate prioritizes speed and API cost; main window shows all available sources.

### Development Process

- Implement strictly according to agreed technical design; report back if a requirement proves infeasible
- Do NOT change implementation approach without explicit authorization — no workarounds, no shortcuts

## Things to Avoid

- Do NOT create D-Bus `.service` files — they persist and cause silent launch failures
- Do NOT use `let _ = std::fs::write(...)` — handle write errors explicitly
- Do NOT add `%U` to `.desktop` Exec — QuickDict is not a URL handler
- Do NOT use `MESON_INSTALL_DESTDIR_PREFIX` — use `get_option('prefix')` instead
- Extension JS template literals (backtick strings) can cause syntax errors on GNOME 45 — prefer single quotes with concatenation where needed