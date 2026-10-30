# dotty

Dotty deploys folders from a YAML manifest. It creates symlinks on Unix and NTFS junctions on Windows.

Create `dotty.yaml` in the current directory:

```yaml
version: 1

profiles:
  base:
    folders:
      - source: shell
        target: ~/.config/shell
      - source: nvim
        target: ~/.config/nvim

  work:
    extends: [base]
    folders:
      - source: work/tools
        target: ~/.local/share/work-tools
```

Sources are directories relative to `dotty.yaml`. Targets must be absolute paths, start with `~` for the home directory, or use a preset prefix: `config:`, `config-local:`, `data:`, `data-local:`, `cache:`, or `state:`. For example, `config:/nvim` resolves to `%APPDATA%\nvim` on Windows and `$XDG_CONFIG_HOME/nvim` (usually `~/.config/nvim`) on Linux. Preset names are case-sensitive, and `state:` is unavailable on Windows and macOS. `~/.config` stays literal and is never rewritten. Parent profiles are applied before the profile that extends them. Duplicate targets are rejected.

Apply a profile with:

```sh
cargo run -- apply work
```

Logs default to `info`; set `RUST_LOG=debug` for additional diagnostics.

Dotty creates missing parent directories. It leaves existing targets untouched, except that reapplying a link already pointing to the same source succeeds without changing it.

Package installation and individual file links are not supported yet.
