# mk61_keyboard

A pure-Rust, modular Linux controller for the **Gamakay MK61 / SK61L** keyboard
(USB `1EA7:9018`). The project controls the keyboard's on-board lighting and
profiles directly through the vendor HID configuration interface.

This is a **clean-room** implementation: the protocol handling, data structures
and safety logic in this project were written independently from observed
hardware behaviour and public documentation, without copying the code of the
original Windows driver or any other existing implementation.


## Requirements

- Linux with hidraw support (`CONFIG_HIDRAW`).
- A stable Rust toolchain. `rust-toolchain.toml` pins the `stable` channel and
  installs `rustfmt` and `clippy`.
- No C compiler or C library is required.

## Build and test

```sh
cargo build --workspace
cargo test --workspace
mk61 help
mk61 --version
```

Quality gates used by this project:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tests never require a physical keyboard: the mock transport and a fake clock
make them fast and deterministic.

## Workspace layout

```text
crates/
  mk61-core/      pure domain model, no I/O, no unsafe code
  mk61-device/    hidraw discovery and transport, plus a mock transport
  mk61-service/   use-cases, backups, flash cooldown, typed results
  mk61-cli/       `mk61` binary; argument parsing and presentation only
```

Dependency direction is strictly downward:

```text
mk61-cli ---> mk61-service ---> mk61-device
    |               |
    +--> mk61-core <+
```

`mk61-core` must not depend on any other workspace crate. Only `mk61-device`
may eventually contain `unsafe` code, and every block must have a `SAFETY`
comment.

## License

GPL-2.0-only. See [`LICENSE`](LICENSE).

The protocol knowledge in this project was derived from independent observation
and reverse engineering of the hardware, and is documented here as a clean-room
implementation.
