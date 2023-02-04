# tuic

Delicately-TUICed 0-RTT proxy protocol

[![Version](https://img.shields.io/crates/v/tuic.svg?style=flat)](https://crates.io/crates/tuic)
[![Documentation](https://img.shields.io/badge/docs-release-brightgreen.svg?style=flat)](https://docs.rs/tuic)
[![License](https://img.shields.io/crates/l/tuic.svg?style=flat)](https://github.com/EAimTY/tuic/blob/dev/LICENSE)

## Overview

The TUIC protocol specification can be found in [SPEC.md](https://github.com/EAimTY/tuic/blob/dev/tuic/SPEC.md). This crate provides the low-level abstract of the TUIC protocol in Rust.

Some optional features that can be enabled:

- `model` - Provides a model of the TUIC protocol, with packet fragmentation and task counter built-in. No I/O operation is involved.
- `marshal` - Provides methods for (un)marsalling the protocol in sync flavor.
- `async_marshal` - Provides methods for (un)marsalling the protocol in async flavor.

The root of the protocol abstraction is the [`Header`](https://docs.rs/tuic/latest/tuic/enum.Header.html).

## License

GNU General Public License v3.0
